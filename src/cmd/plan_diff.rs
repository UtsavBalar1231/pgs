//! `pgs plan-diff`: reconcile an agent-supplied [`CommitPlan`] against a fresh scan.
//!
//! Each planned selection is classified as `still_valid`, `shifted`, or
//! `gone` so the agent can tell whether a saved plan still applies. A6
//! additive fields on [`CommitPlan`] / [`PlannedCommit`] enable higher-
//! confidence matching but remain optional.

use std::io::{self, Read};

use clap::Args;

use crate::error::PgsError;
use crate::git::{diff, repo};
use crate::models::{CommitPlan, HunkInfo, PlannedCommit, ScanResult, SelectionSpec};
use crate::output::view::{
    CommandOutput, PlanDiffEntry, PlanDiffMatchConfidence, PlanDiffOutput, PlanDiffShift,
};
use crate::selection::{parse::detect_selection, resolve};

#[derive(Args)]
pub struct PlanDiffArgs {
    /// Path to a `CommitPlan` JSON file. Mutually exclusive with `--stdin`.
    #[arg(long, conflicts_with = "stdin")]
    pub plan: Option<String>,
    /// Read the `CommitPlan` JSON from stdin. Default when `--plan` is omitted.
    #[arg(long)]
    pub stdin: bool,
}

/// Run plan-diff over the provided repository path with CLI-supplied args.
///
/// # Errors
/// Returns [`PgsError::InvalidSelection`] when the plan JSON is malformed,
/// [`PgsError::Io`] when `--plan` reads fail, or any underlying git/scan error.
#[allow(clippy::needless_pass_by_value)]
pub fn execute(
    repo_path: Option<&str>,
    context: u32,
    args: PlanDiffArgs,
) -> Result<CommandOutput, PgsError> {
    let plan = load_plan(&args)?;
    run_with_plan(repo_path, context, &plan)
}

/// Run plan-diff with a pre-built [`CommitPlan`] (MCP entry point).
///
/// # Errors
/// Returns underlying git/scan failures.
pub fn run_with_plan(
    repo_path: Option<&str>,
    context: u32,
    plan: &CommitPlan,
) -> Result<CommandOutput, PgsError> {
    let repository = repo::open(repo_path)?;
    let d = diff::diff_index_to_workdir(&repository, context)?;
    let scan = diff::build_scan_result(&repository, &d, None)?;
    Ok(diff_plan(plan, &scan).into())
}

fn load_plan(args: &PlanDiffArgs) -> Result<CommitPlan, PgsError> {
    let raw = match (&args.plan, args.stdin) {
        (Some(path), _) => std::fs::read_to_string(path).map_err(|e| PgsError::Io {
            path: path.into(),
            source: e,
        })?,
        (None, _) => read_stdin()?,
    };
    serde_json::from_str(&raw).map_err(|e| PgsError::InvalidSelection {
        detail: format!("malformed CommitPlan JSON: {e}"),
    })
}

fn read_stdin() -> Result<String, PgsError> {
    let mut buf = String::new();
    io::stdin()
        .read_to_string(&mut buf)
        .map_err(|e| PgsError::Io {
            path: "<stdin>".into(),
            source: e,
        })?;
    Ok(buf)
}

/// Pure core: reconcile a plan against a pre-built [`ScanResult`].
fn diff_plan(plan: &CommitPlan, scan: &ScanResult) -> PlanDiffOutput {
    let mut still_valid: Vec<PlanDiffEntry> = Vec::new();
    let mut shifted: Vec<PlanDiffShift> = Vec::new();
    let mut gone: Vec<PlanDiffEntry> = Vec::new();

    for commit in &plan.commits {
        for selection in &commit.selections {
            classify_entry(
                commit,
                selection,
                scan,
                &mut still_valid,
                &mut shifted,
                &mut gone,
            );
        }
    }

    PlanDiffOutput::new(still_valid, shifted, gone)
}

fn classify_entry(
    commit: &PlannedCommit,
    selection: &str,
    scan: &ScanResult,
    still_valid: &mut Vec<PlanDiffEntry>,
    shifted: &mut Vec<PlanDiffShift>,
    gone: &mut Vec<PlanDiffEntry>,
) {
    let Ok(spec) = detect_selection(selection) else {
        gone.push(PlanDiffEntry {
            commit_id: commit.id.clone(),
            selection: selection.to_owned(),
            file_path: selection.to_owned(),
            hunk_id: None,
            reason: Some("invalid_selection".to_owned()),
        });
        return;
    };

    let file_path = spec_file_path(&spec);

    // Direct resolution — selection points to live hunks / path.
    if let Ok(resolved) = resolve::resolve_selection(scan, &spec) {
        let hunk_id = resolved_hunk_id(&spec, scan, &resolved);
        let (hunk_field, reason) = match hunk_id {
            Some(id) => (Some(id), None),
            None => (None, Some("file_unchanged".to_owned())),
        };
        still_valid.push(PlanDiffEntry {
            commit_id: commit.id.clone(),
            selection: selection.to_owned(),
            file_path: resolved.file_path,
            hunk_id: hunk_field,
            reason,
        });
        return;
    }

    // Resolution failed — try to locate a file context (either from the spec
    // or by searching scan for any hunk with the right id).
    let file = match file_path {
        Some(path) => scan.files.iter().find(|f| f.path == path),
        None => find_file_for_unresolved_hunk(scan),
    };

    let path_for_report = file.map_or_else(
        || file_path.map_or_else(|| selection.to_owned(), str::to_owned),
        |f| f.path.clone(),
    );

    // When the selection anchored a path but that path is missing → `path_missing`.
    if file.is_none() && file_path.is_some() {
        gone.push(PlanDiffEntry {
            commit_id: commit.id.clone(),
            selection: selection.to_owned(),
            file_path: path_for_report,
            hunk_id: None,
            reason: Some("path_missing".to_owned()),
        });
        return;
    }

    // When the selection is a bare hunk id and no file in the scan still has
    // unstaged hunks, report as consumed-by-commit.
    if file.is_none() && scan.files.iter().all(|f| f.hunks.is_empty()) {
        gone.push(PlanDiffEntry {
            commit_id: commit.id.clone(),
            selection: selection.to_owned(),
            file_path: path_for_report,
            hunk_id: None,
            reason: Some("covered_by_commit".to_owned()),
        });
        return;
    }

    if let Some(file) = file {
        if file.hunks.is_empty() {
            gone.push(PlanDiffEntry {
                commit_id: commit.id.clone(),
                selection: selection.to_owned(),
                file_path: path_for_report,
                hunk_id: None,
                reason: Some("covered_by_commit".to_owned()),
            });
            return;
        }

        let captured_id = captured_hunk_id_for(commit, &spec);
        if let Some(old_id) = captured_id {
            if let Some((new_hunk, confidence)) =
                find_fuzzy_match(commit, &spec, file, old_id.as_str(), scan)
            {
                shifted.push(PlanDiffShift {
                    commit_id: commit.id.clone(),
                    selection: selection.to_owned(),
                    file_path: path_for_report,
                    old_hunk_id: old_id,
                    new_hunk_id: new_hunk.hunk_id.clone(),
                    match_confidence: confidence,
                });
                return;
            }
        }
    }

    gone.push(PlanDiffEntry {
        commit_id: commit.id.clone(),
        selection: selection.to_owned(),
        file_path: path_for_report,
        hunk_id: None,
        reason: Some("no_match".to_owned()),
    });
}

/// When the plan's selection is a bare hunk id that no longer resolves,
/// return the first file with live hunks so fuzzy matching has a scope.
fn find_file_for_unresolved_hunk(scan: &ScanResult) -> Option<&crate::models::FileInfo> {
    scan.files.iter().find(|f| !f.hunks.is_empty())
}

fn spec_file_path(spec: &SelectionSpec) -> Option<&str> {
    match spec {
        SelectionSpec::File { path }
        | SelectionSpec::Lines { path, .. }
        | SelectionSpec::Directory { prefix: path } => Some(path.as_str()),
        SelectionSpec::Hunk { .. } => None,
    }
}

fn resolved_hunk_id(
    spec: &SelectionSpec,
    scan: &ScanResult,
    resolved: &crate::models::ResolvedSelection,
) -> Option<String> {
    let file = scan.files.iter().find(|f| f.path == resolved.file_path)?;
    match spec {
        SelectionSpec::Hunk { hunk_id } => Some(hunk_id.clone()),
        _ => resolved
            .hunk_indices
            .first()
            .and_then(|idx| file.hunks.get(*idx))
            .map(|h| h.hunk_id.clone()),
    }
}

/// When the selection itself is a hunk id, treat that as the captured id.
/// Otherwise fall back to the A6 `captured_hunk_id` field on the commit.
fn captured_hunk_id_for(commit: &PlannedCommit, spec: &SelectionSpec) -> Option<String> {
    match spec {
        SelectionSpec::Hunk { hunk_id } => Some(hunk_id.clone()),
        _ => commit.captured_hunk_id.clone(),
    }
}

/// Descriptive fuzzy match — returns the best candidate hunk + confidence.
/// Never upgrades to `still_valid`; always classifies as shifted.
fn find_fuzzy_match<'a>(
    commit: &PlannedCommit,
    spec: &SelectionSpec,
    file: &'a crate::models::FileInfo,
    _old_id: &str,
    _scan: &ScanResult,
) -> Option<(&'a HunkInfo, PlanDiffMatchConfidence)> {
    // High: checksum matches (plan recorded expected_checksum).
    if let Some(expected) = commit.expected_checksum.as_deref() {
        if let Some(h) = file.hunks.iter().find(|h| h.checksum == expected) {
            return Some((h, PlanDiffMatchConfidence::High));
        }
    }

    // Medium: overlap heuristic when the old selection points at a line range.
    if let SelectionSpec::Lines { ranges, .. } = spec {
        if let Some(range) = ranges.first() {
            if let Some(h) = file
                .hunks
                .iter()
                .max_by_key(|h| overlap_fraction(range.start, range.end, h))
            {
                let overlap = overlap_fraction(range.start, range.end, h);
                let confidence = if overlap >= 50 {
                    PlanDiffMatchConfidence::Medium
                } else {
                    PlanDiffMatchConfidence::Low
                };
                return Some((h, confidence));
            }
        }
    }

    // Low: same file, no further evidence — return the first hunk.
    file.hunks
        .first()
        .map(|h| (h, PlanDiffMatchConfidence::Low))
}

/// Percentage (0..=100) of `[old_start..=old_end]` covered by `hunk`'s new range.
fn overlap_fraction(old_start: u32, old_end: u32, hunk: &HunkInfo) -> u32 {
    let hunk_start = hunk.new_start;
    let hunk_end = hunk
        .new_start
        .saturating_add(hunk.new_lines.saturating_sub(1));
    let overlap_start = old_start.max(hunk_start);
    let overlap_end = old_end.min(hunk_end);
    if overlap_end < overlap_start {
        return 0;
    }
    let overlap_len = overlap_end - overlap_start + 1;
    let old_len = old_end.saturating_sub(old_start).saturating_add(1).max(1);
    (overlap_len.saturating_mul(100)) / old_len
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        DiffLineInfo, FileInfo, FileStatus, HunkInfo, LineOrigin, ScanResult, ScanSummary,
    };

    fn hunk(id: &str, new_start: u32, new_lines: u32, checksum: &str) -> HunkInfo {
        HunkInfo {
            hunk_id: id.into(),
            old_start: new_start,
            old_lines: new_lines,
            new_start,
            new_lines,
            header: "@@ -x,y +x,y @@".into(),
            lines: vec![DiffLineInfo {
                line_number: new_start,
                origin: LineOrigin::Addition,
                content: "x".into(),
            }],
            checksum: checksum.into(),
            whitespace_only: false,
        }
    }

    fn scan_one_file(path: &str, hunks: Vec<HunkInfo>) -> ScanResult {
        ScanResult {
            files: vec![FileInfo {
                path: path.into(),
                status: FileStatus::Modified,
                file_checksum: String::new(),
                is_binary: false,
                old_mode: 0o100_644,
                new_mode: 0o100_644,
                hunks,
            }],
            summary: ScanSummary {
                total_files: 1,
                total_hunks: 1,
                modified: 1,
                ..ScanSummary::default()
            },
        }
    }

    #[test]
    fn direct_file_hit_classifies_still_valid() {
        let scan = scan_one_file("f.rs", vec![hunk("h1xxxxxxxxx0", 1, 2, "cs-a")]);
        let plan = CommitPlan {
            version: "v1".into(),
            commits: vec![PlannedCommit {
                id: Some("c1".into()),
                selections: vec!["f.rs".into()],
                ..PlannedCommit::default()
            }],
            ..CommitPlan::default()
        };
        let result = diff_plan(&plan, &scan);
        assert_eq!(result.still_valid.len(), 1);
        assert!(result.shifted.is_empty());
        assert!(result.gone.is_empty());
    }

    #[test]
    fn missing_path_classifies_gone_with_reason() {
        let scan = scan_one_file("f.rs", vec![hunk("h1xxxxxxxxx0", 1, 2, "cs-a")]);
        let plan = CommitPlan {
            version: "v1".into(),
            commits: vec![PlannedCommit {
                id: Some("c1".into()),
                selections: vec!["ghost.rs".into()],
                ..PlannedCommit::default()
            }],
            ..CommitPlan::default()
        };
        let result = diff_plan(&plan, &scan);
        assert_eq!(result.gone.len(), 1);
        assert_eq!(result.gone[0].reason.as_deref(), Some("path_missing"));
        assert!(result.has_drift());
    }

    #[test]
    fn unresolved_hunk_id_falls_back_to_low_confidence_shift() {
        let scan = scan_one_file("f.rs", vec![hunk("cccccc111111", 1, 2, "cs-a")]);
        let plan = CommitPlan {
            version: "v1".into(),
            commits: vec![PlannedCommit {
                id: Some("c1".into()),
                selections: vec!["abcdef123456".into()],
                ..PlannedCommit::default()
            }],
            ..CommitPlan::default()
        };
        let result = diff_plan(&plan, &scan);
        // Unknown hunk id + one live hunk in scan → Low-confidence shift.
        assert_eq!(result.shifted.len(), 1);
        assert!(matches!(
            result.shifted[0].match_confidence,
            PlanDiffMatchConfidence::Low
        ));
    }

    #[test]
    fn checksum_match_upgrades_to_high_confidence() {
        let scan = scan_one_file("f.rs", vec![hunk("newhunkid1234", 5, 3, "stable-cs")]);
        let plan = CommitPlan {
            version: "v1".into(),
            commits: vec![PlannedCommit {
                id: Some("c1".into()),
                selections: vec!["f.rs:1-3".into()],
                expected_checksum: Some("stable-cs".into()),
                captured_hunk_id: Some("oldhunkid0000".into()),
                ..PlannedCommit::default()
            }],
            ..CommitPlan::default()
        };
        let result = diff_plan(&plan, &scan);
        assert_eq!(result.shifted.len(), 1);
        assert!(matches!(
            result.shifted[0].match_confidence,
            PlanDiffMatchConfidence::High
        ));
    }
}
