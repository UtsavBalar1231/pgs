use std::collections::{HashMap, HashSet};

use clap::Args;

use crate::error::AgstageError;
use crate::git::{diff, repo, unstaging};
use crate::models::{
    OperationStatus, ResolvedSelection, SelectionSpec, StageResult, StagedItem, format_selection,
};
use crate::safety::{backup, lock};
use crate::selection::{parse, resolve};

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

#[allow(clippy::needless_pass_by_value)] // clap dispatches Args by value
pub fn execute(
    repo_path: Option<&str>,
    context: u32,
    args: UnstageArgs,
) -> Result<(), AgstageError> {
    // 1. Open repo
    let repository = repo::open(repo_path)?;

    // 2. Wait for index lock
    lock::wait_for_lock_release(&repository, 5)?;

    // 3-4. Compute HEAD-to-index diff and build scan result
    let d = diff::diff_head_to_index(&repository, context)?;
    let scan = diff::build_scan_result(&repository, &d, None)?;

    // 5. Guard: nothing staged
    if scan.files.is_empty() {
        return Err(AgstageError::NoChanges);
    }

    // 6. Parse positional args
    let specs: Vec<SelectionSpec> = args
        .selections
        .iter()
        .map(|s| parse::detect_selection(s))
        .collect::<Result<Vec<_>, _>>()?;

    // 7. Guard: empty selections
    if specs.is_empty() {
        return Err(AgstageError::SelectionEmpty);
    }

    // 8-9. Validate constraints
    for spec in &specs {
        resolve::validate_binary_constraints(&scan, spec)?;
        resolve::validate_whole_file_constraints(&scan, spec)?;
    }

    // 10. Resolve each spec (keep paired with original spec)
    let mut spec_resolved: Vec<(SelectionSpec, ResolvedSelection)> = Vec::new();
    for spec in specs {
        let resolved = resolve::resolve_selection(&scan, &spec)?;
        spec_resolved.push((spec, resolved));
    }

    // 11. Parse --exclude
    let exclude_specs: Vec<SelectionSpec> = args
        .exclude
        .iter()
        .map(|s| parse::detect_selection(s))
        .collect::<Result<Vec<_>, _>>()?;

    // 12. Build exclusion set: (file_path, hunk_index)
    let mut exclusion_set: HashSet<(String, usize)> = HashSet::new();
    for ex_spec in &exclude_specs {
        if let Ok(ex_resolved) = resolve::resolve_selection(&scan, ex_spec) {
            for &idx in &ex_resolved.hunk_indices {
                exclusion_set.insert((ex_resolved.file_path.clone(), idx));
            }
        }
    }

    // 13. Filter: remove excluded hunks from resolved selections
    for (_spec, resolved) in &mut spec_resolved {
        resolved
            .hunk_indices
            .retain(|&idx| !exclusion_set.contains(&(resolved.file_path.clone(), idx)));
    }

    // 14. Dedup: merge selections by file_path
    let mut merged: HashMap<String, (SelectionSpec, ResolvedSelection)> = HashMap::new();
    for (spec, resolved) in spec_resolved {
        let entry = merged
            .entry(resolved.file_path.clone())
            .or_insert_with(|| (spec.clone(), resolved.clone()));
        if entry.1.file_path == resolved.file_path {
            for idx in &resolved.hunk_indices {
                if !entry.1.hunk_indices.contains(idx) {
                    entry.1.hunk_indices.push(*idx);
                }
            }
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

    // 15. Guard: no resolved hunks remain
    let has_hunks = work_items.iter().any(|(_, r)| !r.hunk_indices.is_empty());
    if !has_hunks {
        return Err(AgstageError::SelectionEmpty);
    }

    // 16. Validate freshness (for unstage, skip — HEAD-to-index is always fresh)
    // No freshness validation needed for unstage since we're operating on the index directly.

    // 17. Dry-run
    if args.dry_run {
        let succeeded: Vec<StagedItem> = work_items
            .iter()
            .map(|(spec, resolved)| {
                let lines = estimate_lines(&scan, resolved);
                StagedItem {
                    selection: format_selection(spec),
                    lines_staged: lines,
                }
            })
            .collect();

        let result = StageResult {
            status: OperationStatus::DryRun,
            succeeded,
            failed: vec![],
            warnings: vec![],
            backup_id: String::new(),
        };
        let json = serde_json::to_string_pretty(&result)?;
        println!("{json}");
        return Ok(());
    }

    // 18. Create backup
    let backup_info = backup::create_backup(&repository)?;

    // 19. Execute unstaging for each resolved selection
    let mut succeeded: Vec<StagedItem> = Vec::new();

    for (spec, resolved) in &work_items {
        let file_path = &resolved.file_path;

        let unstage_result = execute_single_unstage(&repository, &scan, spec, resolved, file_path);

        match unstage_result {
            Ok(lines_unstaged) => {
                succeeded.push(StagedItem {
                    selection: format_selection(spec),
                    lines_staged: lines_unstaged,
                });
            }
            Err(e) => {
                let _ = backup::restore_backup(&repository, &backup_info.backup_id);
                return Err(e);
            }
        }
    }

    // 20-21. Build and output result
    let result = StageResult {
        status: OperationStatus::Ok,
        succeeded,
        failed: vec![],
        warnings: vec![],
        backup_id: backup_info.backup_id,
    };
    let json = serde_json::to_string_pretty(&result)?;
    println!("{json}");
    Ok(())
}

/// Execute unstaging for a single resolved selection.
fn execute_single_unstage(
    repo: &git2::Repository,
    scan: &crate::models::ScanResult,
    spec: &SelectionSpec,
    resolved: &ResolvedSelection,
    file_path: &str,
) -> Result<u32, AgstageError> {
    let is_lines = resolved.line_ranges.is_some();
    let is_hunk = matches!(spec, SelectionSpec::Hunk { .. });

    match (is_lines, is_hunk) {
        // Lines selection
        (true, _) => {
            let ranges = resolved.line_ranges.as_ref().expect("checked is_lines");
            let mut selected = HashSet::new();
            for range in ranges {
                for line in range.start..=range.end {
                    selected.insert(line);
                }
            }
            unstaging::unstage_lines(repo, file_path, &selected)
        }

        // Hunk selection
        (false, true) => {
            let mut total_lines: u32 = 0;
            let file_info = scan.files.iter().find(|f| f.path == file_path);
            for &hunk_idx in &resolved.hunk_indices {
                if let Some(fi) = file_info {
                    if let Some(hunk) = fi.hunks.get(hunk_idx) {
                        let lines = unstaging::unstage_hunk(repo, file_path, hunk)?;
                        total_lines += lines;
                    }
                }
            }
            Ok(total_lines)
        }

        // File-level selection (handles all status cases: modified, added, deleted)
        (false, false) => unstaging::unstage_file(repo, file_path),
    }
}

/// Estimate lines for dry-run reporting.
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
                #[allow(clippy::cast_possible_truncation)]
                let count = h
                    .lines
                    .iter()
                    .filter(|l| {
                        l.origin == crate::models::LineOrigin::Addition
                            || l.origin == crate::models::LineOrigin::Deletion
                    })
                    .count() as u32;
                count
            })
            .sum()
    }
}
