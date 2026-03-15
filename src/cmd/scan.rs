use clap::Args;

use crate::error::PgsError;
use crate::git::{diff, repo};
use crate::output::view::{CommandOutput, ScanOutput};

#[derive(Args)]
pub struct ScanArgs {
    /// Files to scan (omit for all changed files).
    pub files: Vec<String>,

    /// Include full diff line content and checksums.
    #[arg(long)]
    pub full: bool,
}

#[allow(clippy::needless_pass_by_value)] // clap dispatches Args by value
pub fn execute(
    repo_path: Option<&str>,
    context: u32,
    args: ScanArgs,
) -> Result<CommandOutput, PgsError> {
    let repository = repo::open(repo_path)?;
    let d = diff::diff_index_to_workdir(&repository, context)?;

    let file_filter: Option<&[String]> = if args.files.is_empty() {
        None
    } else {
        Some(&args.files)
    };

    let result = diff::build_scan_result(&repository, &d, file_filter)?;

    if result.files.is_empty() {
        return Err(PgsError::NoChanges);
    }

    let output = if args.full {
        ScanOutput::full(result)
    } else {
        ScanOutput::compact(&result)
    };

    Ok(output.into())
}
