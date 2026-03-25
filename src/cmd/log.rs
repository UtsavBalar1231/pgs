use chrono::{DateTime, FixedOffset};
use git2::{DiffOptions, ErrorCode, Sort};

use crate::error::PgsError;
use crate::git::repo;
use crate::output::view::{
    CommandOutput, CommitEntryView, LogOutput, OUTPUT_VERSION, OutputCommand,
};

/// Maximum number of commits to walk before setting `truncated = true`.
const WALK_LIMIT: usize = 1000;

/// Arguments for the `log` command.
#[derive(Clone, Debug, clap::Args)]
pub struct LogArgs {
    /// Maximum number of commits to return.
    #[arg(long, default_value = "20")]
    pub max_count: u32,
    /// File paths to filter commits by.
    #[arg(last = true)]
    pub paths: Vec<String>,
}

/// Execute the `log` command and return commit history.
///
/// # Errors
///
/// Returns `PgsError::Git` on repository or walk failures.
#[allow(clippy::needless_pass_by_value)] // clap dispatches Args by value
pub fn execute(repo_path: Option<&str>, args: LogArgs) -> Result<CommandOutput, PgsError> {
    let repository = repo::open(repo_path)?;
    let mut walk = repository.revwalk()?;

    // On an empty repo (unborn branch), push_head() fails because HEAD's
    // target reference doesn't exist yet. Catch this and return an empty log
    // rather than surfacing a git error. The error code is NotFound with
    // class Reference — match on either dimension for robustness.
    match walk.push_head() {
        Ok(()) => {}
        Err(e) if e.code() == ErrorCode::NotFound || e.class() == git2::ErrorClass::Reference => {
            return Ok(LogOutput {
                version: OUTPUT_VERSION,
                command: OutputCommand::Log,
                commits: vec![],
                total: 0,
                truncated: false,
            }
            .into());
        }
        Err(e) => return Err(PgsError::Git(e)),
    }

    walk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;

    let mut commits = Vec::new();
    let mut truncated = false;

    for (traversed, oid_result) in walk.enumerate() {
        if traversed >= WALK_LIMIT {
            truncated = true;
            break;
        }

        let oid = oid_result?;
        let commit = repository.find_commit(oid)?;

        if !args.paths.is_empty() && !commit_touches_paths(&repository, &commit, &args.paths)? {
            continue;
        }

        commits.push(build_entry(&commit));

        if commits.len() >= args.max_count as usize {
            break;
        }
    }

    let total = commits.len();
    Ok(LogOutput {
        version: OUTPUT_VERSION,
        command: OutputCommand::Log,
        commits,
        total,
        truncated,
    }
    .into())
}

/// Return `true` if `commit` touches any of the given `paths`.
fn commit_touches_paths(
    repo: &git2::Repository,
    commit: &git2::Commit,
    paths: &[String],
) -> Result<bool, git2::Error> {
    let parent_tree = if commit.parent_count() > 0 {
        Some(commit.parent(0)?.tree()?)
    } else {
        None
    };

    let mut opts = DiffOptions::new();
    for path in paths {
        opts.pathspec(path);
    }

    let diff =
        repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&commit.tree()?), Some(&mut opts))?;

    Ok(diff.deltas().len() > 0)
}

/// Build a [`CommitEntryView`] from a git2 commit.
fn build_entry(commit: &git2::Commit) -> CommitEntryView {
    let hash = commit.id().to_string();
    let short_hash: String = hash.chars().take(12).collect();
    let sig = commit.author();
    let author = format!(
        "{} <{}>",
        sig.name().unwrap_or("Unknown"),
        sig.email().unwrap_or("unknown")
    );
    let date = git_time_to_iso8601(&sig.when());
    let message = commit.message().unwrap_or("").to_owned();

    CommitEntryView {
        hash,
        short_hash,
        author,
        date,
        message,
    }
}

/// Convert a [`git2::Time`] to an RFC 3339 string.
fn git_time_to_iso8601(t: &git2::Time) -> String {
    let offset_secs = t.offset_minutes() * 60;
    let tz = FixedOffset::east_opt(offset_secs)
        .unwrap_or_else(|| FixedOffset::east_opt(0).expect("UTC is valid"));
    DateTime::from_timestamp(t.seconds(), 0)
        .map(|dt| dt.with_timezone(&tz).to_rfc3339())
        .unwrap_or_default()
}
