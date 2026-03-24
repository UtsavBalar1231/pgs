use std::collections::{HashMap, HashSet};

use clap::Args;

use crate::error::PgsError;
use crate::git::{diff, repo, staging};
use crate::models::{
    FileStatus, OperationStatus, ResolvedSelection, SelectionSpec, format_selection,
};
use crate::output::view::{CommandOutput, OperationItemView, OperationOutput};
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
}

#[allow(clippy::needless_pass_by_value)] // clap dispatches Args by value
pub fn execute(
    repo_path: Option<&str>,
    context: u32,
    args: StageArgs,
) -> Result<CommandOutput, PgsError> {
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

    let reportable_items: Vec<(SelectionSpec, ResolvedSelection)> = spec_resolved
        .iter()
        .filter(|(_, resolved)| is_reportable_selection(&scan, resolved))
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

    let work_items: Vec<(SelectionSpec, ResolvedSelection)> = merged.into_values().collect();

    let has_work = work_items
        .iter()
        .any(|(_, r)| !r.hunk_indices.is_empty() || is_whole_file_operation(&scan, &r.file_path));
    if !has_work {
        return Err(PgsError::SelectionEmpty);
    }

    for (_, resolved) in &work_items {
        resolve::validate_freshness(&repository, &scan, &resolved.file_path)?;
    }

    if args.dry_run {
        let items: Vec<OperationItemView> = reportable_items
            .iter()
            .map(|(spec, resolved)| {
                operation_item(format_selection(spec), estimate_lines(&scan, resolved))
            })
            .collect();

        return Ok(OperationOutput::stage(OperationStatus::DryRun, items, vec![], None).into());
    }

    let backup_info = backup::create_backup(&repository)?;

    let mut actual_lines_by_file: HashMap<String, u32> = HashMap::new();

    for (spec, resolved) in &work_items {
        // Skip work items whose hunks were fully excluded (but not whole-file ops)
        if resolved.hunk_indices.is_empty() && !is_whole_file_operation(&scan, &resolved.file_path)
        {
            continue;
        }

        let file_path = &resolved.file_path;
        let file_info = scan
            .files
            .iter()
            .find(|f| f.path == *file_path)
            .ok_or_else(|| PgsError::FileNotInDiff {
                path: file_path.clone(),
            })?;

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
                // Rollback on failure
                let _ = backup::restore_backup(&repository, &backup_info.backup_id);
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

    Ok(OperationOutput::stage(
        OperationStatus::Ok,
        items,
        vec![],
        Some(backup_info.backup_id),
    )
    .into())
}

const fn operation_item(selection: String, lines_affected: u32) -> OperationItemView {
    OperationItemView::new(selection, lines_affected)
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
            let ranges = resolved.line_ranges.as_ref().expect("checked is_lines");
            let mut selected = HashSet::new();
            for range in ranges {
                for line in range.start..=range.end {
                    selected.insert(line);
                }
            }
            staging::stage_lines(repo, file_path, &selected)
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

            // Otherwise collect all selected line numbers across hunks
            // and make a single stage_lines call (avoids overwriting index per-hunk).
            let mut selected = HashSet::new();
            if let Some(fi) = file_info {
                for &hunk_idx in &resolved.hunk_indices {
                    if let Some(hunk) = fi.hunks.get(hunk_idx) {
                        for line in &hunk.lines {
                            if matches!(
                                line.origin,
                                crate::models::LineOrigin::Addition
                                    | crate::models::LineOrigin::Context
                                    | crate::models::LineOrigin::Deletion
                            ) {
                                selected.insert(line.line_number);
                            }
                        }
                    }
                }
            }
            staging::stage_lines(repo, file_path, &selected)
        }

        // Binary or Added file-level: stage the whole file
        (_, _, _, true) | (FileStatus::Added, _, _, _) => {
            let file_info = scan.files.iter().find(|f| f.path == file_path);
            let mode_override = file_info.map(|fi| fi.new_mode);
            staging::stage_file(repo, file_path, mode_override)
        }
    }
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

fn is_reportable_selection(scan: &crate::models::ScanResult, resolved: &ResolvedSelection) -> bool {
    !resolved.hunk_indices.is_empty() || is_whole_file_operation(scan, &resolved.file_path)
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
