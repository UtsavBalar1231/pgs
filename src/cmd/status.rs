use clap::Args;

use crate::error::PgsError;
use crate::git::{diff, repo};
use crate::output::view::{CommandOutput, StatusOutput};

#[derive(Args)]
pub struct StatusArgs {
    /// Include every staged hunk with its id and lines, to unstage by hunk id.
    #[arg(long)]
    pub full: bool,
}

/// Report what is staged (HEAD -> index), optionally with every staged hunk.
///
/// # Errors
///
/// Returns an error if the repository cannot be opened or diffed.
pub fn execute(
    repo_path: Option<&str>,
    context: u32,
    full: bool,
) -> Result<CommandOutput, PgsError> {
    let repository = repo::open(repo_path)?;
    let d = diff::diff_head_to_index(&repository, context)?;
    let report = diff::build_status_report(&d)?;
    let output = StatusOutput::from(report);
    if !full {
        return Ok(output.into());
    }
    // `unstage` resolves hunk ids against this same HEAD -> index diff, so the
    // ids reported here are the ones it accepts.
    let staged = diff::build_scan_result(&repository, &d, None)?;
    Ok(output.with_hunks(staged.files).into())
}
