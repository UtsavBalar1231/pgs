use crate::error::PgsError;
use crate::output::view::{CommandOutput, OverviewOutput, ScanOutput, StatusOutput};

use super::{scan, status};

/// Fuse unstaged (`scan`) and staged (`status`) envelopes; propagate `NoChanges` only when both sides are empty.
/// # Errors
/// Returns any non-`NoChanges` error from the scan or status handlers, or `PgsError::NoChanges` when the tree is quiescent.
pub fn execute(repo_path: Option<&str>, context: u32) -> Result<CommandOutput, PgsError> {
    let unstaged = match scan::execute(
        repo_path,
        context,
        scan::ScanArgs {
            files: Vec::new(),
            full: false,
        },
    ) {
        Ok(CommandOutput::Scan(scan_output)) => Some(scan_output),
        Ok(_) => {
            return Err(PgsError::Internal(
                "scan handler returned non-scan output".to_owned(),
            ));
        }
        Err(PgsError::NoChanges) => None,
        Err(other) => return Err(other),
    };

    let staged = match status::execute(repo_path, context) {
        Ok(CommandOutput::Status(status_output)) => Some(status_output),
        Ok(_) => {
            return Err(PgsError::Internal(
                "status handler returned non-status output".to_owned(),
            ));
        }
        Err(PgsError::NoChanges) => None,
        Err(other) => return Err(other),
    };

    if unstaged.is_none() && staged_is_empty(staged.as_ref()) {
        return Err(PgsError::NoChanges);
    }

    let unstaged = unstaged.unwrap_or_else(empty_scan_output);
    let staged = staged.unwrap_or_else(empty_status_output);

    Ok(OverviewOutput::new(unstaged, staged).into())
}

fn staged_is_empty(staged: Option<&StatusOutput>) -> bool {
    staged.is_none_or(|output| output.files.is_empty())
}

fn empty_scan_output() -> ScanOutput {
    ScanOutput::compact(&crate::models::ScanResult {
        files: Vec::new(),
        summary: crate::models::ScanSummary::default(),
    })
}

fn empty_status_output() -> StatusOutput {
    StatusOutput::from(crate::models::StatusReport {
        staged_files: Vec::new(),
        summary: crate::models::StatusSummary::default(),
    })
}
