use clap::Args;
use git2::Repository;

use crate::cmd::work_plan;
use crate::error::PgsError;
use crate::git::{
    diff, read_index_blob, repo,
    staging::{count_lines, line_selection_for},
    unstaging,
};
use crate::models::{OperationStatus, ResolvedSelection, ScanResult, SelectionSpec};
use crate::output::view::{CommandOutput, OperationOutput, OutputCommand};
use crate::safety::{backup, lock};

#[derive(Args)]
pub struct UnstageArgs {
    /// Selections to unstage (auto-detected: file path, 12-hex hunk ID, path:range).
    pub selections: Vec<String>,

    /// Exclude selections (same auto-detect syntax).
    #[arg(long)]
    pub exclude: Vec<String>,

    /// Validate without modifying the index.
    #[arg(long)]
    pub dry_run: bool,
}

/// Remove the selected changes from the index.
///
/// Selectors resolve against the HEAD -> index diff, so hunk ids come from
/// `status --full`, not `scan`.
///
/// # Errors
///
/// Repository and selection errors, and any unstaging failure (the index is
/// restored from its backup first).
#[allow(clippy::needless_pass_by_value)] // clap dispatches Args by value
pub fn execute(
    repo_path: Option<&str>,
    context: u32,
    args: UnstageArgs,
) -> Result<CommandOutput, PgsError> {
    let repository = repo::open(repo_path)?;
    lock::wait_for_lock_release(&repository, 5)?;
    let d = diff::diff_head_to_index(&repository, context)?;
    let scan = diff::build_scan_result(&repository, &d, None)?;
    if scan.files.is_empty() {
        return Err(PgsError::NoChanges);
    }

    let line_count = |path: &str| index_line_count(&repository, path);
    let plan = work_plan::plan(&scan, &args.selections, &args.exclude, &line_count)?;
    let items = work_plan::report_items(&scan, &plan, &line_count);

    if args.dry_run {
        return Ok(OperationOutput::new(
            OutputCommand::Unstage,
            OperationStatus::DryRun,
            items,
            vec![],
            None,
        )
        .into());
    }

    let backup_id = backup::create_backup(&repository)?;
    for (spec, resolved) in &plan.per_file {
        if let Err(error) = execute_single_unstage(&repository, &scan, spec, resolved) {
            return Err(backup::rollback(&repository, &backup_id, error));
        }
    }

    Ok(OperationOutput::new(
        OutputCommand::Unstage,
        OperationStatus::Ok,
        items,
        vec![],
        Some(backup_id),
    )
    .into())
}

/// Unstage one file's merged selection: whole file, or the selected lines.
fn execute_single_unstage(
    repo: &Repository,
    scan: &ScanResult,
    spec: &SelectionSpec,
    resolved: &ResolvedSelection,
) -> Result<(), PgsError> {
    let path = resolved.file_path.as_str();
    if resolved.line_ranges.is_none() && !matches!(spec, SelectionSpec::Hunk { .. }) {
        let all_hunks = scan
            .files
            .iter()
            .find(|f| f.path == path)
            .is_some_and(|f| resolved.hunk_indices.len() == f.hunks.len());
        if all_hunks || work_plan::is_whole_file_operation(scan, path) {
            return unstaging::unstage_file(repo, path).map(drop);
        }
    }
    let sel = line_selection_for(scan, resolved, index_line_count(repo, path));
    unstaging::unstage_lines(repo, path, &sel).map(drop)
}

/// New-side line count for an `unstage` selection, which diffs HEAD -> Index,
/// so the new side is the index blob rather than the workdir file.
///
/// A path absent from the index counts as zero; it has no staged content and so
/// never reaches the line-range path.
fn index_line_count(repo: &Repository, file_path: &str) -> u32 {
    read_index_blob(repo, file_path).map_or(0, |bytes| count_lines(&bytes))
}
