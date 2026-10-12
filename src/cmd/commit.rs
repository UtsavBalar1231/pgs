use std::io::Read as _;

use clap::Args;

use crate::error::PgsError;
use crate::git::repo;
use crate::models::CommitResult;
use crate::output::view::{CommandOutput, CommitOutput};

/// `message_file` value that means "read the message from stdin".
const STDIN_SENTINEL: &str = "-";

#[derive(Args)]
pub struct CommitArgs {
    /// Commit message.
    #[arg(short, long, conflicts_with = "message_file")]
    pub message: Option<String>,
    /// Read the commit message from a file, or from stdin when the path is `-`.
    #[arg(short = 'F', long, conflicts_with = "message")]
    pub message_file: Option<String>,
    /// Replace the current HEAD commit instead of creating a new child commit.
    #[arg(long)]
    pub amend: bool,
}

/// Apply git's `--cleanup=whitespace` normalization to a raw commit message.
///
/// Normalizes CRLF to LF, strips trailing whitespace from every line, drops
/// leading and trailing blank lines, collapses runs of blank lines to one, and
/// terminates the result with exactly one newline. Comment (`#`) lines are kept
/// verbatim — stripping them is git's `strip` mode, which applies to editor
/// input only. A message with no content normalizes to the empty string.
fn normalize_message(raw: &str) -> String {
    let mut normalized = String::with_capacity(raw.len());
    let mut pending_blank = false;

    // `str::lines` already splits on CRLF, and `trim_end` below strips a lone trailing `\r`.
    for line in raw.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            pending_blank = !normalized.is_empty();
            continue;
        }
        if pending_blank {
            normalized.push('\n');
            pending_blank = false;
        }
        normalized.push_str(trimmed);
        normalized.push('\n');
    }

    normalized
}

/// Read the raw message bytes named by `--message-file`.
fn read_message_file(path: &str, allow_stdin: bool) -> Result<String, PgsError> {
    if path == STDIN_SENTINEL {
        if !allow_stdin {
            return Err(PgsError::InvalidArguments {
                detail: "message_file `-` reads from stdin and is available on the CLI only; \
                         supply an absolute file path or an inline message instead"
                    .to_owned(),
            });
        }
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| PgsError::input_file_unreadable("<stdin>", e))?;
        return Ok(buf);
    }

    std::fs::read_to_string(path).map_err(|e| PgsError::input_file_unreadable(path, e))
}

/// Resolve the commit message from exactly one of `-m` / `-F`, then normalize it.
///
/// The invariant lives here rather than at the clap or MCP boundary because both
/// front ends build [`CommitArgs`] and only one of them goes through clap.
fn resolve_message(args: &CommitArgs, allow_stdin: bool) -> Result<String, PgsError> {
    let raw = match (args.message.as_deref(), args.message_file.as_deref()) {
        (Some(_), Some(_)) => {
            return Err(PgsError::InvalidArguments {
                detail: "message and message_file are mutually exclusive; supply exactly one"
                    .to_owned(),
            });
        }
        (None, None) => {
            return Err(PgsError::InvalidArguments {
                detail: "a commit message is required; supply either message or message_file"
                    .to_owned(),
            });
        }
        (Some(message), None) => message.to_owned(),
        (None, Some(path)) => read_message_file(path, allow_stdin)?,
    };

    let normalized = normalize_message(&raw);
    if normalized.is_empty() {
        return Err(PgsError::EmptyCommitMessage);
    }

    Ok(normalized)
}

/// Create or amend a commit from the current index.
///
/// `allow_stdin` gates the `-F -` form: the CLI permits it, the MCP path does
/// not, because an MCP request has no stdin of its own to consume.
///
/// # Errors
/// Returns [`PgsError::InvalidArguments`] when neither or both message sources
/// are supplied, [`PgsError::InputFileUnreadable`] when `-F <path>` cannot be
/// read, [`PgsError::EmptyCommitMessage`] when the normalized message is empty,
/// and [`PgsError::NoChanges`] when a non-amend commit has nothing staged.
#[allow(clippy::needless_pass_by_value)] // clap dispatches Args by value
pub fn execute(
    repo_path: Option<&str>,
    args: CommitArgs,
    allow_stdin: bool,
) -> Result<CommandOutput, PgsError> {
    // Resolved first so `--amend` cannot destroy the existing message before failing.
    let message = resolve_message(&args, allow_stdin)?;

    let repository = repo::open(repo_path)?;
    let sig = repository.signature()?;

    let mut index = repository.index()?;
    let tree_oid = index.write_tree()?;
    let tree = repository.find_tree(tree_oid)?;

    // Before the first commit there is no HEAD: a plain commit becomes the
    // root commit, and there is nothing to amend.
    let head_commit = match repository.head() {
        Ok(head) => Some(head.peel_to_commit()?),
        Err(e) if e.code() == git2::ErrorCode::UnbornBranch => None,
        Err(e) => return Err(e.into()),
    };
    if args.amend && head_commit.is_none() {
        return Err(PgsError::InvalidArguments {
            detail: "--amend needs an existing commit, but the branch has none yet".to_owned(),
        });
    }
    let author = match (&head_commit, args.amend) {
        (Some(head), true) => head.author(),
        _ => sig.clone(),
    };
    let parent_commits: Vec<git2::Commit<'_>> = match (&head_commit, args.amend) {
        (Some(head), true) => head.parents().collect(),
        (Some(head), false) => vec![repository.find_commit(head.id())?],
        (None, _) => Vec::new(),
    };
    let parent_refs: Vec<&git2::Commit<'_>> = parent_commits.iter().collect();
    let base_tree = match parent_commits.first() {
        Some(parent) => Some(parent.tree()?),
        None => None,
    };

    // A plain commit needs staged changes. Amend can intentionally be message-only.
    let nothing_staged = base_tree
        .as_ref()
        .map_or(tree.is_empty(), |parent| tree_oid == parent.id());
    if !args.amend && nothing_staged {
        return Err(PgsError::NoChanges);
    }

    let commit_oid = if let (Some(head), true) = (&head_commit, args.amend) {
        head.amend(
            Some("HEAD"),
            None,
            Some(&sig),
            None,
            Some(&message),
            Some(&tree),
        )?
    } else {
        repository.commit(Some("HEAD"), &author, &sig, &message, &tree, &parent_refs)?
    };

    // Compute insertions/deletions from the commit's first parent to the new tree.
    let stat_diff = repository.diff_tree_to_tree(base_tree.as_ref(), Some(&tree), None)?;
    let stats = stat_diff.stats()?;

    let result = CommitResult {
        commit_hash: commit_oid.to_string(),
        message,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_message_crlf_becomes_lf() {
        assert_eq!(normalize_message("subject\r\nbody\r\n"), "subject\nbody\n");
    }

    #[test]
    fn normalize_message_strips_trailing_whitespace_per_line() {
        assert_eq!(
            normalize_message("subject   \t\nbody  \n"),
            "subject\nbody\n"
        );
    }

    #[test]
    fn normalize_message_removes_leading_blank_lines() {
        assert_eq!(normalize_message("\n\nsubject\nbody\n"), "subject\nbody\n");
    }

    #[test]
    fn normalize_message_collapses_consecutive_blank_lines() {
        assert_eq!(
            normalize_message("subject\n\n\n\nbody\n"),
            "subject\n\nbody\n"
        );
    }

    #[test]
    fn normalize_message_appends_exactly_one_trailing_newline() {
        assert_eq!(normalize_message("subject\nbody"), "subject\nbody\n");
        assert_eq!(
            normalize_message("subject\nbody\n\n\n\n"),
            "subject\nbody\n"
        );
    }

    #[test]
    fn normalize_message_preserves_comment_lines() {
        assert_eq!(
            normalize_message("subject\n# a comment\nbody\n"),
            "subject\n# a comment\nbody\n"
        );
    }

    #[test]
    fn normalize_message_preserves_leading_indentation() {
        assert_eq!(
            normalize_message("subject\n    indented\n"),
            "subject\n    indented\n"
        );
    }

    #[test]
    fn normalize_message_whitespace_only_becomes_empty() {
        for raw in ["", "   ", "\t", "\n\n", " \t \n ", "\r\n", "\r"] {
            assert_eq!(normalize_message(raw), "", "raw: {raw:?}");
        }
    }

    fn args(message: Option<&str>, message_file: Option<&str>) -> CommitArgs {
        CommitArgs {
            message: message.map(ToOwned::to_owned),
            message_file: message_file.map(ToOwned::to_owned),
            amend: false,
        }
    }

    #[test]
    fn resolve_message_both_sources_returns_invalid_arguments() {
        let err = resolve_message(&args(Some("m"), Some("f")), true)
            .expect_err("both sources must be refused");
        assert!(matches!(err, PgsError::InvalidArguments { .. }), "{err:?}");
    }

    #[test]
    fn resolve_message_no_source_returns_invalid_arguments() {
        let err = resolve_message(&args(None, None), true).expect_err("no source must be refused");
        assert!(matches!(err, PgsError::InvalidArguments { .. }), "{err:?}");
    }

    #[test]
    fn resolve_message_stdin_sentinel_without_stdin_returns_invalid_arguments() {
        let err = resolve_message(&args(None, Some(STDIN_SENTINEL)), false)
            .expect_err("`-` must be refused when stdin is unavailable");
        assert!(matches!(err, PgsError::InvalidArguments { .. }), "{err:?}");
    }

    #[test]
    fn resolve_message_whitespace_only_inline_returns_empty_commit_message() {
        let err = resolve_message(&args(Some("  \n\t\n"), None), true)
            .expect_err("whitespace-only message must be refused");
        assert!(matches!(err, PgsError::EmptyCommitMessage), "{err:?}");
    }

    #[test]
    fn resolve_message_missing_file_returns_input_file_unreadable() {
        let dir = tempfile::TempDir::new().unwrap();
        let missing = dir.path().join("missing-message.txt");
        let err = resolve_message(&args(None, Some(missing.to_str().unwrap())), true)
            .expect_err("missing message file must be refused");
        assert!(
            matches!(err, PgsError::InputFileUnreadable { .. }),
            "{err:?}"
        );
        assert_eq!(err.exit_code(), 2);
    }
}
