use clap::Args;

use crate::error::PgsError;
use crate::git::repo;
use crate::models::CommitResult;
use crate::output::view::{CommandOutput, CommitOutput};

#[derive(Args)]
pub struct CommitArgs {
    /// Commit message.
    #[arg(short, long)]
    pub message: String,
    /// Replace the current HEAD commit instead of creating a new child commit.
    #[arg(long)]
    pub amend: bool,
}

#[allow(clippy::needless_pass_by_value)] // clap dispatches Args by value
pub fn execute(repo_path: Option<&str>, args: CommitArgs) -> Result<CommandOutput, PgsError> {
    // Checked first so `--amend` cannot destroy the existing message before failing.
    if args.message.trim().is_empty() {
        return Err(PgsError::EmptyCommitMessage);
    }

    let repository = repo::open(repo_path)?;
    let sig = repository.signature()?;

    let mut index = repository.index()?;
    let tree_oid = index.write_tree()?;
    let tree = repository.find_tree(tree_oid)?;

    let head_ref = repository.head()?;
    let head_commit = head_ref.peel_to_commit()?;
    let author = if args.amend {
        head_commit.author()
    } else {
        sig.clone()
    };
    let parent_commits = if args.amend {
        let mut parents = Vec::with_capacity(head_commit.parent_count());
        for parent in head_commit.parents() {
            parents.push(parent);
        }
        parents
    } else {
        vec![repository.find_commit(head_commit.id())?]
    };
    let parent_refs: Vec<&git2::Commit<'_>> = parent_commits.iter().collect();
    let base_tree = match parent_commits.first() {
        Some(parent) => Some(parent.tree()?),
        None => None,
    };

    // A plain commit needs staged changes. Amend can intentionally be message-only.
    if !args.amend
        && base_tree
            .as_ref()
            .is_some_and(|parent| tree_oid == parent.id())
    {
        return Err(PgsError::NoChanges);
    }

    let commit_oid = if args.amend {
        head_commit.amend(
            Some("HEAD"),
            None,
            Some(&sig),
            None,
            Some(&args.message),
            Some(&tree),
        )?
    } else {
        repository.commit(
            Some("HEAD"),
            &author,
            &sig,
            &args.message,
            &tree,
            &parent_refs,
        )?
    };

    // Compute insertions/deletions from the commit's first parent to the new tree.
    let stat_diff = repository.diff_tree_to_tree(base_tree.as_ref(), Some(&tree), None)?;
    let stats = stat_diff.stats()?;

    let result = CommitResult {
        commit_hash: commit_oid.to_string(),
        message: args.message,
        author: format!(
            "{} <{}>",
            author.name().unwrap_or("unknown"),
            author.email().unwrap_or("unknown")
        ),
        files_changed: stats.files_changed(),
        insertions: crate::saturating_u32(stats.insertions()),
        deletions: crate::saturating_u32(stats.deletions()),
    };

    Ok(CommitOutput::from(result).into())
}
