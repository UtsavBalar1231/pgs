use std::collections::{HashMap, HashSet};

use clap::Args;

use crate::error::PgsError;
use crate::git::staging::{line_selection_for, workdir_line_count};
use crate::git::{diff, read_head_mode, repo, staging};
use crate::models::{
    FileStatus, OperationPreview, OperationStatus, ResolvedSelection, SelectionSpec,
    format_selection,
};
use crate::output::view::{CommandOutput, OperationItemView, OperationOutput, OutputCommand};
use crate::safety::{backup, lock};
use crate::selection::{parse, resolve};

#[derive(Args)]
pub struct StageArgs {
    /// Selections to stage (auto-detected: file path, 12-hex hunk ID, path:range).
    pub selections: Vec<String>,

    /// Exclude selections (same auto-detect syntax).
    #[arg(long)]
    pub exclude: Vec<String>,

    /// Validate without modifying the index.
    #[arg(long)]
    pub dry_run: bool,

    /// Emit exact per-file line preview (requires `--dry-run`).
    #[arg(long)]
    pub explain: bool,

    /// Per-file preview cap (default 200, 0 = unlimited); applies only with --dry-run --explain.
    #[arg(long, default_value_t = 200)]
    pub limit: u32,

    /// Assert that a file still has a given SHA-256 checksum from a prior scan
    /// (`PATH=SHA` format, repeatable). Fails with `StaleScan` (exit 3) when the
    /// file changed between the agent's scan and this stage call.
    #[arg(long = "expect", value_name = "PATH=SHA")]
    pub expect: Vec<String>,
}

#[allow(clippy::needless_pass_by_value)] // clap dispatches Args by value
pub fn execute(
    repo_path: Option<&str>,
    context: u32,
    args: StageArgs,
) -> Result<CommandOutput, PgsError> {
    // 0. --explain requires --dry-run
    if args.explain && !args.dry_run {
        return Err(PgsError::ExplainWithoutDryRun);
    }

    // Parse --expect pairs early; fail fast on format errors before any I/O.
    let expected_checksums = parse_expect_pairs(&args.expect)?;

    // 1. Open repo
    let repository = repo::open(repo_path)?;

    // 2. Wait for index lock
    lock::wait_for_lock_release(&repository, 5)?;

    // 3-4. Compute diff and build scan result
    let d = diff::diff_index_to_workdir(&repository, context)?;
    let scan = diff::build_scan_result(&repository, &d, None)?;

    // 5. Guard: no changes
    if scan.files.is_empty() {
        return Err(PgsError::NoChanges);
    }

    // 6. Parse positional args
    let specs: Vec<SelectionSpec> = args
        .selections
        .iter()
        .map(|s| parse::detect_selection(s))
        .collect::<Result<Vec<_>, _>>()?;

    // 7. Guard: empty selections
    if specs.is_empty() {
        return Err(PgsError::SelectionEmpty);
    }

    // 8-9. Validate constraints
    for spec in &specs {
        resolve::validate_binary_constraints(&scan, spec)?;
        resolve::validate_whole_file_constraints(&scan, spec)?;
    }

    // 10. Resolve each spec (keep paired with original spec)
    let mut spec_resolved: Vec<(SelectionSpec, ResolvedSelection)> = Vec::new();
    for spec in specs {
        if let SelectionSpec::Directory { prefix } = &spec {
            let resolved_list = resolve::resolve_directory(&scan, prefix)?;
            for resolved in resolved_list {
                let file_spec = SelectionSpec::File {
                    path: resolved.file_path.clone(),
                };
                spec_resolved.push((file_spec, resolved));
            }
        } else {
            let resolved = resolve::resolve_selection(&scan, &spec)?;
            spec_resolved.push((spec, resolved));
        }
    }

    // 11. Parse --exclude
    let exclude_specs: Vec<SelectionSpec> = args
        .exclude
        .iter()
        .map(|s| parse::detect_selection(s))
        .collect::<Result<Vec<_>, _>>()?;

    // 12. Build exclusion sets: per-hunk and per-file
    let mut exclusion_set: HashSet<(String, usize)> = HashSet::new();
    let mut excluded_files: HashSet<String> = HashSet::new();
    for ex_spec in &exclude_specs {
        if let SelectionSpec::Directory { prefix } = ex_spec {
            if let Ok(ex_resolved_list) = resolve::resolve_directory(&scan, prefix) {
                for ex_resolved in ex_resolved_list {
                    if ex_resolved.hunk_indices.is_empty() {
                        excluded_files.insert(ex_resolved.file_path.clone());
                    }
                    for &idx in &ex_resolved.hunk_indices {
                        exclusion_set.insert((ex_resolved.file_path.clone(), idx));
                    }
                }
            }
        } else if let Ok(ex_resolved) = resolve::resolve_selection(&scan, ex_spec) {
            if ex_resolved.hunk_indices.is_empty() {
                // File has no hunks (binary/deleted/renamed) — exclude entire file
                excluded_files.insert(ex_resolved.file_path.clone());
            }
            for &idx in &ex_resolved.hunk_indices {
                exclusion_set.insert((ex_resolved.file_path.clone(), idx));
            }
        }
    }

    // 13. Filter: remove excluded hunks and fully-excluded files
    spec_resolved.retain(|(_, resolved)| !excluded_files.contains(&resolved.file_path));
    for (_spec, resolved) in &mut spec_resolved {
        resolved
            .hunk_indices
            .retain(|&idx| !exclusion_set.contains(&(resolved.file_path.clone(), idx)));
    }

    reject_mixed_selector_kinds(&spec_resolved)?;

    let reportable_items: Vec<(SelectionSpec, ResolvedSelection)> = spec_resolved
        .iter()
        .filter(|(_, resolved)| is_reportable_selection(&repository, &scan, resolved))
        .cloned()
        .collect();

    let mut merged: HashMap<String, (SelectionSpec, ResolvedSelection)> = HashMap::new();
    for (spec, resolved) in spec_resolved {
        let entry = merged
            .entry(resolved.file_path.clone())
            .or_insert_with(|| (spec.clone(), resolved.clone()));
        if entry.1.file_path == resolved.file_path {
            // Merge hunk_indices (dedup)
            for idx in &resolved.hunk_indices {
                if !entry.1.hunk_indices.contains(idx) {
                    entry.1.hunk_indices.push(*idx);
                }
            }
            // Merge line_ranges
            if let Some(new_ranges) = &resolved.line_ranges {
                if let Some(existing) = &mut entry.1.line_ranges {
                    existing.extend_from_slice(new_ranges);
                } else {
                    entry.1.line_ranges = Some(new_ranges.clone());
                }
            }
        }
    }

    let mut work_items: Vec<(SelectionSpec, ResolvedSelection)> = merged.into_values().collect();
    work_items.retain(|(_, r)| is_reportable_selection(&repository, &scan, r));
    if work_items.is_empty() {
        return Err(PgsError::SelectionEmpty);
    }

    // Reject --expect paths that name a file not in the resolved selection.
    let resolved_paths: HashSet<&str> = work_items
        .iter()
        .map(|(_, r)| r.file_path.as_str())
        .collect();
    for path in expected_checksums.keys() {
        if !resolved_paths.contains(path.as_str()) {
            return Err(PgsError::InvalidSelection {
                detail: format!("--expect path '{path}' is not part of the staging selection"),
            });
        }
    }

    for (_, resolved) in &work_items {
        let expected = expected_checksums
            .get(&resolved.file_path)
            .map(String::as_str);
        resolve::validate_freshness(&repository, &scan, &resolved.file_path, expected)?;
    }

    if args.dry_run {
        let items: Vec<OperationItemView> = reportable_items
            .iter()
            .map(|(spec, resolved)| {
                operation_item(format_selection(spec), estimate_lines(&scan, resolved))
            })
            .collect();

        let output = OperationOutput::new(
            OutputCommand::Stage,
            OperationStatus::DryRun,
            items,
            vec![],
            None,
        );
        if args.explain {
            let previews = compute_previews(&repository, &scan, &work_items, args.limit)?;
            return Ok(output.with_previews(previews).into());
        }
        return Ok(output.into());
    }

    let backup_info = backup::create_backup(&repository)?;

    let mut actual_lines_by_file: HashMap<String, u32> = HashMap::new();
    let mut warnings: Vec<String> = Vec::new();

    for (spec, resolved) in &work_items {
        let file_path = &resolved.file_path;
        let file_info = scan
            .files
            .iter()
            .find(|f| f.path == *file_path)
            .ok_or_else(|| PgsError::FileNotInDiff {
                path: file_path.clone(),
            })?;

        // Detect symlinks: check the scan's new_mode, or fall back to HEAD mode for
        // untracked symlinks that haven't been committed yet.
        let is_symlink = file_info.new_mode == 0o120_000
            || read_head_mode(&repository, file_path).ok() == Some(0o120_000);

        if is_symlink {
            let is_hunk = matches!(spec, SelectionSpec::Hunk { .. });
            if is_hunk || resolved.line_ranges.is_some() {
                warnings.push(format!(
                    "symlink '{file_path}' staged whole; line/hunk selection ignored"
                ));
            }
        }

        let stage_result = execute_single_stage(
            &repository,
            &scan,
            spec,
            resolved,
            &file_info.status,
            file_path,
            file_info.is_binary,
        );

        match stage_result {
            Ok(lines_affected) => {
                actual_lines_by_file.insert(file_path.clone(), lines_affected);
            }
            Err(e) => {
                if let Err(restore_err) =
                    backup::restore_backup(&repository, &backup_info.backup_id)
                {
                    return Err(PgsError::RestoreFailed {
                        backup_id: backup_info.backup_id.clone(),
                        op_error: e.to_string(),
                        restore_error: restore_err.to_string(),
                    });
                }
                return Err(e);
            }
        }
    }

    let mut selection_count_by_file: HashMap<String, usize> = HashMap::new();
    for (_spec, resolved) in &reportable_items {
        *selection_count_by_file
            .entry(resolved.file_path.clone())
            .or_insert(0) += 1;
    }

    let items: Vec<OperationItemView> = reportable_items
        .iter()
        .map(|(spec, resolved)| {
            let file_selection_count = selection_count_by_file
                .get(&resolved.file_path)
                .copied()
                .unwrap_or(0);
            let lines_affected = if file_selection_count == 1 {
                actual_lines_by_file
                    .get(&resolved.file_path)
                    .copied()
                    .unwrap_or_else(|| estimate_lines(&scan, resolved))
            } else {
                estimate_lines(&scan, resolved)
            };
            operation_item(format_selection(spec), lines_affected)
        })
        .collect();

    Ok(OperationOutput::new(
        OutputCommand::Stage,
        OperationStatus::Ok,
        items,
        warnings,
        Some(backup_info.backup_id),
    )
    .into())
}

const fn operation_item(selection: String, lines_affected: u32) -> OperationItemView {
    OperationItemView::new(selection, lines_affected)
}

/// Build per-file `OperationPreview` entries for `--dry-run --explain`.
fn compute_previews(
    repository: &git2::Repository,
    scan: &crate::models::ScanResult,
    work_items: &[(SelectionSpec, ResolvedSelection)],
    limit: u32,
) -> Result<Vec<OperationPreview>, PgsError> {
    let mut previews: Vec<OperationPreview> = Vec::with_capacity(work_items.len());
    for (spec, resolved) in work_items {
        let selection = format_selection(spec);
        let request = staging::PreviewRequest {
            scan,
            resolved,
            selection: &selection,
            limit,
        };
        previews.push(staging::preview_stage(repository, &request)?);
    }
    Ok(previews)
}

/// Execute staging for a single resolved selection based on file status and selection type.
fn execute_single_stage(
    repo: &git2::Repository,
    scan: &crate::models::ScanResult,
    spec: &SelectionSpec,
    resolved: &ResolvedSelection,
    file_status: &FileStatus,
    file_path: &str,
    is_binary: bool,
) -> Result<u32, PgsError> {
    // Determine selection type:
    // - If resolved.line_ranges is Some → lines selection
    // - Else if original spec is Hunk → hunk-level staging
    // - Else → file-level staging
    let is_lines = resolved.line_ranges.is_some();
    let is_hunk = matches!(spec, SelectionSpec::Hunk { .. });

    match (file_status, is_lines, is_hunk, is_binary) {
        // Deleted files: stage_deletion
        (FileStatus::Deleted, _, _, _) => {
            staging::stage_deletion(repo, file_path)?;
            Ok(0)
        }

        // Renamed files: stage_rename
        (FileStatus::Renamed { old_path }, _, _, _) => {
            let file_info = scan.files.iter().find(|f| f.path == file_path);
            let mode_override = file_info.map(|fi| fi.new_mode);
            staging::stage_rename(repo, old_path, file_path, mode_override)?;
            Ok(0)
        }

        // Modified + lines selection
        (FileStatus::Modified, true, _, false) => {
            let sel = line_selection_for(scan, resolved, workdir_line_count(repo, file_path));
            let mode_override = scan
                .files
                .iter()
                .find(|f| f.path == file_path)
                .filter(|fi| fi.old_mode != fi.new_mode)
                .map(|fi| fi.new_mode);
            staging::stage_lines(repo, file_path, &sel, mode_override)
        }

        // Modified + hunk selection (or file selection with excluded hunks)
        (FileStatus::Modified, false, true | false, false) => {
            // If this is a file-level spec with ALL hunks present, stage the whole file.
            let file_info = scan.files.iter().find(|f| f.path == file_path);
            let all_hunks_present =
                file_info.is_some_and(|fi| resolved.hunk_indices.len() == fi.hunks.len());

            if !is_hunk && all_hunks_present {
                let mode_override = file_info
                    .filter(|fi| fi.old_mode != fi.new_mode)
                    .map(|fi| fi.new_mode);
                return staging::stage_file(repo, file_path, mode_override);
            }

            // Collect selected lines across hunks via line_selection_for, then
            // make a single stage_lines call (avoids overwriting index per-hunk).
            let sel = line_selection_for(scan, resolved, workdir_line_count(repo, file_path));
            let mode_override = file_info
                .filter(|fi| fi.old_mode != fi.new_mode)
                .map(|fi| fi.new_mode);
            staging::stage_lines(repo, file_path, &sel, mode_override)
        }

        // Binary or Added file-level: stage the whole file
        (_, _, _, true) | (FileStatus::Added, _, _, _) => {
            let file_info = scan.files.iter().find(|f| f.path == file_path);
            let mode_override = file_info.map(|fi| fi.new_mode);
            staging::stage_file(repo, file_path, mode_override)
        }
    }
}

/// Reject a same-file mix of a line-range selector with a file or hunk selector.
///
/// Per-file selections merge into one [`ResolvedSelection`], and that merge is lossy
/// across kinds: resolving a line range also fills `hunk_indices` with the hunks the
/// range incidentally intersects, so the staging layer cannot tell an explicit hunk
/// pick from a range-incidental one and stages only the ranges. Rejecting here, where
/// the per-spec kinds still exist, keeps a dropped selection from being reported staged.
///
/// # Errors
///
/// [`PgsError::InvalidSelection`] when one file carries both selector kinds.
pub fn reject_mixed_selector_kinds(
    items: &[(SelectionSpec, ResolvedSelection)],
) -> Result<(), PgsError> {
    let mut kind_by_path: HashMap<&str, bool> = HashMap::new();
    for (spec, resolved) in items {
        let is_lines = matches!(spec, SelectionSpec::Lines { .. });
        if let Some(previous) = kind_by_path.insert(resolved.file_path.as_str(), is_lines)
            && previous != is_lines
        {
            return Err(PgsError::InvalidSelection {
                detail: format!(
                    "file '{}' mixes a line-range selector with a file or hunk selector; \
                     use one selector kind per file, or issue them as separate calls",
                    resolved.file_path
                ),
            });
        }
    }
    Ok(())
}

/// Check if a file requires whole-file handling (binary, added, deleted, renamed, mode-only).
fn is_whole_file_operation(scan: &crate::models::ScanResult, file_path: &str) -> bool {
    scan.files.iter().any(|f| {
        f.path == file_path
            && (f.is_binary
                || (f.old_mode != f.new_mode && f.hunks.is_empty())
                || matches!(
                    f.status,
                    FileStatus::Added | FileStatus::Deleted | FileStatus::Renamed { .. }
                ))
    })
}

/// A line range can overlap a hunk and still name nothing that changed. Calling
/// that reportable would print `status: "ok"` for a silent no-op.
fn is_reportable_selection(
    repo: &git2::Repository,
    scan: &crate::models::ScanResult,
    resolved: &ResolvedSelection,
) -> bool {
    if is_whole_file_operation(scan, &resolved.file_path) {
        return true;
    }
    if resolved.line_ranges.is_some() {
        let total = workdir_line_count(repo, &resolved.file_path);
        let sel = line_selection_for(scan, resolved, total);
        return !sel.new_lines.is_empty() || !sel.old_lines.is_empty();
    }
    !resolved.hunk_indices.is_empty()
}

/// Parse `--expect PATH=SHA` pairs into a map, rejecting malformed or duplicate entries.
///
/// # Errors
///
/// [`PgsError::InvalidSelection`] on missing `=` separator or duplicate path.
fn parse_expect_pairs(pairs: &[String]) -> Result<HashMap<String, String>, PgsError> {
    let mut map = HashMap::new();
    for pair in pairs {
        // rsplit_once: SHA is hex (no '='), so split on the LAST '=' to handle
        // file paths that contain '=' (e.g. "src/foo=bar.rs").
        let (path, sha) = pair
            .rsplit_once('=')
            .ok_or_else(|| PgsError::InvalidSelection {
                detail: format!("--expect requires PATH=SHA format, got: '{pair}'"),
            })?;
        if map.insert(path.to_owned(), sha.to_owned()).is_some() {
            return Err(PgsError::InvalidSelection {
                detail: format!("--expect has duplicate entry for path: '{path}'"),
            });
        }
    }
    Ok(map)
}

/// Estimate lines staged for dry-run reporting.
fn estimate_lines(scan: &crate::models::ScanResult, resolved: &ResolvedSelection) -> u32 {
    let file_info = scan.files.iter().find(|f| f.path == resolved.file_path);
    let Some(fi) = file_info else { return 0 };

    if let Some(ranges) = &resolved.line_ranges {
        ranges.iter().map(|r| r.end - r.start + 1).sum()
    } else {
        resolved
            .hunk_indices
            .iter()
            .filter_map(|&idx| fi.hunks.get(idx))
            .map(|h| {
                crate::saturating_u32(
                    h.lines
                        .iter()
                        .filter(|l| l.origin == crate::models::LineOrigin::Addition)
                        .count(),
                )
            })
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::parse_expect_pairs;

    #[test]
    fn parse_expect_pairs_path_with_equals_sign_splits_on_last_equals() {
        // A file path containing '=' must not truncate the path at the first '='.
        let pairs = vec!["src/foo=bar.rs=abc123def456abc1".to_owned()];
        let map = parse_expect_pairs(&pairs).expect("parse");
        assert_eq!(
            map.get("src/foo=bar.rs").map(String::as_str),
            Some("abc123def456abc1")
        );
    }

    #[test]
    fn parse_expect_pairs_simple_path_parses_correctly() {
        let pairs = vec!["src/main.rs=deadbeefcafe0000".to_owned()];
        let map = parse_expect_pairs(&pairs).expect("parse");
        assert_eq!(
            map.get("src/main.rs").map(String::as_str),
            Some("deadbeefcafe0000")
        );
    }

    #[test]
    fn parse_expect_pairs_missing_equals_returns_error() {
        let pairs = vec!["src/main.rs".to_owned()];
        assert!(parse_expect_pairs(&pairs).is_err());
    }
}
