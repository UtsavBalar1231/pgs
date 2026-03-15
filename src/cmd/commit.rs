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
}

#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::needless_pass_by_value)] // clap dispatches Args by value
pub fn execute(repo_path: Option<&str>, args: CommitArgs) -> Result<CommandOutput, PgsError> {
    let repository = repo::open(repo_path)?;
    let sig = repository.signature()?;

    let mut index = repository.index()?;
    let tree_oid = index.write_tree()?;
    let tree = repository.find_tree(tree_oid)?;

    let head_ref = repository.head()?;
    let parent = head_ref.peel_to_commit()?;
    let parent_tree = parent.tree()?;

    // Check if nothing staged: compare tree OIDs
    if tree_oid == parent_tree.id() {
        return Err(PgsError::NoChanges);
    }

    let commit_oid =
        repository.commit(Some("HEAD"), &sig, &sig, &args.message, &tree, &[&parent])?;

    // Compute insertions/deletions from parent tree to new tree
    let stat_diff = repository.diff_tree_to_tree(Some(&parent_tree), Some(&tree), None)?;
    let stats = stat_diff.stats()?;

    let result = CommitResult {
        commit_hash: commit_oid.to_string(),
        message: args.message,
        author: format!(
            "{} <{}>",
            sig.name().unwrap_or("unknown"),
            sig.email().unwrap_or("unknown")
        ),
        files_changed: stats.files_changed(),
        insertions: stats.insertions() as u32,
        deletions: stats.deletions() as u32,
    };

    Ok(CommitOutput::from(result).into())
}
