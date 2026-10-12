//! Turns `stage` / `unstage` selectors into what a call reports and what it runs,
//! resolved against one scan. Both commands share it, so a selector means the
//! same thing in either direction.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, HashMap, HashSet};

use crate::error::PgsError;
use crate::git::staging::line_selection_for;
use crate::models::{
    FileStatus, LineOrigin, ResolvedSelection, ScanResult, SelectionSpec, format_selection,
};
use crate::output::view::OperationItemView;
use crate::saturating_u32;
use crate::selection::{parse, resolve};

/// A selector paired with what it resolved to.
pub type Selection = (SelectionSpec, ResolvedSelection);

/// A file's new-side line count: the workdir for `stage`, the index for `unstage`.
pub type LineCount<'a> = &'a dyn Fn(&str) -> u32;

/// What a call's selectors resolved to.
pub struct WorkPlan {
    /// Every selector that names a change, in the order given: what the output reports.
    pub reported: Vec<Selection>,
    /// The same selections merged into one entry per file, in path order: what runs.
    pub per_file: Vec<Selection>,
}

/// Resolve `selections` minus `excludes` against `scan`.
///
/// # Errors
///
/// Selector parse and validation errors; `InvalidSelection` when one file mixes a
/// line range with a file or hunk selector; `SelectionEmpty` when nothing that
/// changed is named.
pub fn plan(
    scan: &ScanResult,
    selections: &[String],
    excludes: &[String],
    new_line_count: LineCount<'_>,
) -> Result<WorkPlan, PgsError> {
    let specs = parse_all(selections)?;
    if specs.is_empty() {
        return Err(PgsError::SelectionEmpty);
    }
    for spec in &specs {
        resolve::validate_binary_constraints(scan, spec)?;
        resolve::validate_whole_file_constraints(scan, spec)?;
    }
    let mut resolved = resolve_all(scan, specs)?;
    apply_excludes(scan, &parse_all(excludes)?, &mut resolved);
    reject_mixed_selector_kinds(&resolved)?;

    let reported: Vec<Selection> = resolved
        .iter()
        .filter(|(_, r)| names_a_change(scan, r, new_line_count))
        .cloned()
        .collect();
    let per_file: Vec<Selection> = merge_per_file(resolved)
        .into_iter()
        .filter(|(_, r)| names_a_change(scan, r, new_line_count))
        .collect();
    if per_file.is_empty() {
        return Err(PgsError::SelectionEmpty);
    }
    Ok(WorkPlan { reported, per_file })
}

/// One output item per reported selection, with its [`changed_lines`].
pub fn report_items(
    scan: &ScanResult,
    plan: &WorkPlan,
    new_line_count: LineCount<'_>,
) -> Vec<OperationItemView> {
    plan.reported
        .iter()
        .map(|(spec, resolved)| {
            OperationItemView::new(
                format_selection(spec),
                changed_lines(scan, resolved, new_line_count),
            )
        })
        .collect()
}

/// Changed lines a selection moves into (stage) or out of (unstage) the index:
/// its additions plus the deletions it carries.
///
/// Computed from the selection rather than from what the write returned, so a
/// dry run and a real run report the same number.
pub fn changed_lines(
    scan: &ScanResult,
    resolved: &ResolvedSelection,
    new_line_count: LineCount<'_>,
) -> u32 {
    if resolved.line_ranges.is_some() {
        let sel = line_selection_for(scan, resolved, new_line_count(&resolved.file_path));
        return saturating_u32(sel.new_lines.len() + sel.old_lines.len());
    }
    let Some(file) = scan.files.iter().find(|f| f.path == resolved.file_path) else {
        return 0;
    };
    let changed = resolved
        .hunk_indices
        .iter()
        .filter_map(|&idx| file.hunks.get(idx))
        .flat_map(|hunk| &hunk.lines)
        .filter(|line| line.origin != LineOrigin::Context)
        .count();
    saturating_u32(changed)
}

/// Whether a file can only be staged or unstaged whole: binary, added, deleted,
/// renamed, type-changed, or a mode-only change.
pub fn is_whole_file_operation(scan: &ScanResult, file_path: &str) -> bool {
    scan.files.iter().any(|f| {
        f.path == file_path
            && (f.is_binary
                || (f.old_mode != f.new_mode && f.hunks.is_empty())
                || matches!(
                    f.status,
                    FileStatus::Added
                        | FileStatus::Deleted
                        | FileStatus::Renamed { .. }
                        | FileStatus::TypeChanged
                ))
    })
}

fn parse_all(selectors: &[String]) -> Result<Vec<SelectionSpec>, PgsError> {
    selectors
        .iter()
        .map(|s| parse::detect_selection(s))
        .collect()
}

fn resolve_all(scan: &ScanResult, specs: Vec<SelectionSpec>) -> Result<Vec<Selection>, PgsError> {
    let mut resolved = Vec::with_capacity(specs.len());
    for spec in specs {
        if let SelectionSpec::Directory { prefix } = &spec {
            for file in resolve::resolve_directory(scan, prefix)? {
                let file_spec = SelectionSpec::File {
                    path: file.file_path.clone(),
                };
                resolved.push((file_spec, file));
            }
        } else {
            let selection = resolve::resolve_selection(scan, &spec)?;
            resolved.push((spec, selection));
        }
    }
    Ok(resolved)
}

/// Drop excluded hunks, and whole files whose exclusion names no hunk (binary,
/// deleted, renamed). An exclude that names nothing in this scan excludes nothing.
fn apply_excludes(scan: &ScanResult, excludes: &[SelectionSpec], selections: &mut Vec<Selection>) {
    let mut excluded_files: HashSet<String> = HashSet::new();
    let mut excluded_hunks: HashMap<String, HashSet<usize>> = HashMap::new();
    for spec in excludes {
        let resolved = match spec {
            SelectionSpec::Directory { prefix } => {
                resolve::resolve_directory(scan, prefix).unwrap_or_default()
            }
            other => resolve::resolve_selection(scan, other)
                .into_iter()
                .collect(),
        };
        for selection in resolved {
            if selection.hunk_indices.is_empty() {
                excluded_files.insert(selection.file_path.clone());
            }
            excluded_hunks
                .entry(selection.file_path)
                .or_default()
                .extend(selection.hunk_indices);
        }
    }

    selections.retain(|(_, r)| !excluded_files.contains(&r.file_path));
    for (_, resolved) in selections.iter_mut() {
        if let Some(hunks) = excluded_hunks.get(&resolved.file_path) {
            resolved.hunk_indices.retain(|idx| !hunks.contains(idx));
        }
    }
}

/// Reject a same-file mix of a line-range selector with a file or hunk selector.
///
/// Per-file selections merge into one [`ResolvedSelection`], and that merge is lossy
/// across kinds: resolving a line range also fills `hunk_indices` with the hunks the
/// range incidentally intersects, so the write cannot tell an explicit hunk pick from
/// a range-incidental one and applies only the ranges. Rejecting here, where the
/// per-selector kinds still exist, keeps a dropped selection from being reported done.
fn reject_mixed_selector_kinds(items: &[Selection]) -> Result<(), PgsError> {
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

/// Merge every selection on one file into a single entry, keeping hunk order and
/// dropping duplicate hunks. Path order makes execution and errors deterministic.
fn merge_per_file(selections: Vec<Selection>) -> Vec<Selection> {
    let mut merged: BTreeMap<String, Selection> = BTreeMap::new();
    for (spec, resolved) in selections {
        match merged.entry(resolved.file_path.clone()) {
            Entry::Vacant(slot) => {
                slot.insert((spec, resolved));
            }
            Entry::Occupied(mut slot) => {
                let existing = &mut slot.get_mut().1;
                for idx in resolved.hunk_indices {
                    if !existing.hunk_indices.contains(&idx) {
                        existing.hunk_indices.push(idx);
                    }
                }
                if let Some(ranges) = resolved.line_ranges {
                    existing
                        .line_ranges
                        .get_or_insert_with(Vec::new)
                        .extend(ranges);
                }
            }
        }
    }
    merged.into_values().collect()
}

/// A line range can overlap a hunk and still name nothing that changed. Calling
/// that a change would report `status: "ok"` for a silent no-op.
fn names_a_change(
    scan: &ScanResult,
    resolved: &ResolvedSelection,
    new_line_count: LineCount<'_>,
) -> bool {
    if is_whole_file_operation(scan, &resolved.file_path) {
        return true;
    }
    if resolved.line_ranges.is_some() {
        let sel = line_selection_for(scan, resolved, new_line_count(&resolved.file_path));
        return !sel.new_lines.is_empty() || !sel.old_lines.is_empty();
    }
    !resolved.hunk_indices.is_empty()
}
