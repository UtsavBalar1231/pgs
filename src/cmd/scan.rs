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

    /// Include ignored files under the explicitly named paths.
    #[arg(long)]
    pub include_ignored: bool,
}

#[allow(clippy::needless_pass_by_value)] // clap dispatches Args by value
pub fn execute(
    repo_path: Option<&str>,
    context: u32,
    args: ScanArgs,
) -> Result<CommandOutput, PgsError> {
    let repository = repo::open(repo_path)?;
    let (result, skipped) =
        diff::scan_workdir(&repository, context, &args.files, args.include_ignored)?;

    if result.files.is_empty() && skipped.is_empty() {
        return Err(PgsError::NoChanges);
    }

    let mut output = if args.full {
        ScanOutput::full(result)
    } else {
        ScanOutput::compact(&result)
    };

    output.skipped = skipped;
    Ok(output.into())
}
