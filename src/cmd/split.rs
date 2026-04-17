use clap::Args;

use crate::error::PgsError;
use crate::git::{diff, repo};
use crate::models::ScanResult;
use crate::output::view::{CommandOutput, OriginMixView, SplitHunkOutput, SplitRangeView};
use crate::selection::resolve;

#[derive(Args)]
pub struct SplitArgs {
    /// 12-hex hunk ID (from `pgs scan`) to classify.
    pub hunk_id: String,
}

/// Classify a single hunk into contiguous runs (addition, deletion, or mixed).
/// # Errors
/// Returns `NoChanges` for a quiescent tree, `UnknownHunkId` when the id is absent from the fresh scan, or `StaleScan` when workdir content drifted since scan.
#[allow(clippy::needless_pass_by_value)] // clap dispatches Args by value
pub fn execute(
    repo_path: Option<&str>,
    context: u32,
    args: SplitArgs,
) -> Result<CommandOutput, PgsError> {
    let repository = repo::open(repo_path)?;
    let d = diff::diff_index_to_workdir(&repository, context)?;
    let scan = diff::build_scan_result(&repository, &d, None)?;

    if scan.files.is_empty() {
        return Err(PgsError::NoChanges);
    }

    let (file_path, hunk) = locate_hunk(&scan, &args.hunk_id)?;

    // Freshness: fail retryable if workdir content drifted after scan construction.
    resolve::validate_freshness(&repository, &scan, &file_path)?;

    let splits = diff::suggest_splits(hunk);
    let ranges: Vec<SplitRangeView> = splits
        .into_iter()
        .map(|s| SplitRangeView {
            start: s.start,
            end: s.end,
            origin_mix: OriginMixView::from_line_origin(s.origin_mix),
        })
        .collect();

    Ok(SplitHunkOutput::new(args.hunk_id, ranges).into())
}

fn locate_hunk<'a>(
    scan: &'a ScanResult,
    hunk_id: &str,
) -> Result<(String, &'a crate::models::HunkInfo), PgsError> {
    for file in &scan.files {
        if let Some(hunk) = file.hunks.iter().find(|h| h.hunk_id == hunk_id) {
            return Ok((file.path.clone(), hunk));
        }
    }
    Err(PgsError::UnknownHunkId {
        hunk_id: hunk_id.to_owned(),
    })
}
