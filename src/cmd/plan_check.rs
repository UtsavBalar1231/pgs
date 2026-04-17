//! `pgs plan-check`: validate an agent-supplied [`CommitPlan`] against a fresh
//! scan.
//!
//! Reports overlap, uncovered hunks, unsafe line-range selectors, and paths
//! missing from the scan. Descriptive — no mutation.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read};

use clap::Args;

use crate::error::PgsError;
use crate::git::{diff, repo};
use crate::models::{CommitPlan, LineRange, PlannedCommit, ScanResult, SelectionSpec};
use crate::output::view::{CommandOutput, HunkRef, PlanCheckOutput, PlanOverlap, UnsafeSelector};
use crate::selection::{parse::detect_selection, resolve};

#[derive(Args)]
pub struct PlanCheckArgs {
    /// Path to a `CommitPlan` JSON file. Mutually exclusive with `--stdin`.
    #[arg(long, conflicts_with = "stdin")]
    pub plan: Option<String>,
    /// Read the `CommitPlan` JSON from stdin. Default when `--plan` is omitted.
    #[arg(long)]
    pub stdin: bool,
}

/// Validate a plan against a fresh scan of the working tree.
///
/// # Errors
/// Returns [`PgsError::InvalidSelection`] when the plan JSON is malformed or
/// missing, [`PgsError::Io`] when the `--plan` file cannot be read, or any
/// underlying git/scan error.
#[allow(clippy::needless_pass_by_value)]
pub fn execute(
    repo_path: Option<&str>,
    context: u32,
    args: PlanCheckArgs,
) -> Result<CommandOutput, PgsError> {
    let plan = load_plan(&args)?;
    run_with_plan(repo_path, context, &plan)
}

/// Run plan-check with a pre-built [`CommitPlan`] (MCP entry point).
///
/// # Errors
/// Returns underlying git/scan failures; the plan itself is always analyzed,
/// never rejected here.
pub fn run_with_plan(
    repo_path: Option<&str>,
    context: u32,
    plan: &CommitPlan,
) -> Result<CommandOutput, PgsError> {
    let repository = repo::open(repo_path)?;
    let d = diff::diff_index_to_workdir(&repository, context)?;
    let scan = diff::build_scan_result(&repository, &d, None)?;
    Ok(check_plan(plan, &scan).into())
}

fn load_plan(args: &PlanCheckArgs) -> Result<CommitPlan, PgsError> {
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

/// Run the plan-check analysis with a pre-built [`ScanResult`]. Broken out for
/// unit tests that want to avoid real git I/O.
fn check_plan(plan: &CommitPlan, scan: &ScanResult) -> PlanCheckOutput {
    // hunk_id -> (file_path, file_idx, hunk_idx)
    let mut scan_hunks: BTreeMap<String, (String, usize, usize)> = BTreeMap::new();
    let mut scan_paths: BTreeSet<String> = BTreeSet::new();
    for (file_idx, file) in scan.files.iter().enumerate() {
        scan_paths.insert(file.path.clone());
        for (hunk_idx, hunk) in file.hunks.iter().enumerate() {
            scan_hunks.insert(
                hunk.hunk_id.clone(),
                (file.path.clone(), file_idx, hunk_idx),
            );
        }
    }

    // hunk_id -> commit_ids that cover it (uses BTreeSet to keep output stable).
    let mut coverage: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut unsafe_selectors: Vec<UnsafeSelector> = Vec::new();
    let mut unknown_paths: BTreeSet<String> = BTreeSet::new();

    for (commit_idx, commit) in plan.commits.iter().enumerate() {
        let commit_label = commit_label(commit, commit_idx);
        analyze_commit(
            commit,
            &commit_label,
            scan,
            &scan_paths,
            &mut coverage,
            &mut unsafe_selectors,
            &mut unknown_paths,
        );
    }

    let overlaps: Vec<PlanOverlap> = coverage
        .iter()
        .filter(|(_, commits)| commits.len() >= 2)
        .map(|(hunk_id, commits)| PlanOverlap {
            hunk_id: hunk_id.clone(),
            commits: commits.iter().cloned().collect(),
        })
        .collect();

    let covered_ids: BTreeSet<&String> = coverage.keys().collect();
    let uncovered: Vec<HunkRef> = scan_hunks
        .iter()
        .filter(|(id, _)| !covered_ids.contains(id))
        .map(|(id, (path, _, _))| HunkRef {
            file_path: path.clone(),
            hunk_id: id.clone(),
        })
        .collect();

    PlanCheckOutput::new(
        overlaps,
        uncovered,
        unsafe_selectors,
        unknown_paths.into_iter().collect(),
    )
}

fn commit_label(commit: &PlannedCommit, index: usize) -> String {
    commit
        .id
        .clone()
        .unwrap_or_else(|| format!("commit-{index}"))
}

fn analyze_commit(
    commit: &PlannedCommit,
    commit_label: &str,
    scan: &ScanResult,
    scan_paths: &BTreeSet<String>,
    coverage: &mut BTreeMap<String, BTreeSet<String>>,
    unsafe_selectors: &mut Vec<UnsafeSelector>,
    unknown_paths: &mut BTreeSet<String>,
) {
    for selection in &commit.selections {
        let Ok(spec) = detect_selection(selection) else {
            // Malformed selectors are reported as unsafe so the plan author sees them.
            unsafe_selectors.push(UnsafeSelector {
                commit_id: commit.id.clone(),
                selection: selection.clone(),
                reason: "invalid_selection".to_owned(),
            });
            continue;
        };

        match &spec {
            SelectionSpec::Lines { path, ranges } => {
                if !scan_paths.contains(path) {
                    unknown_paths.insert(path.clone());
                    continue;
                }
                if ranges_span_hunk_boundary(scan, path, ranges) {
                    unsafe_selectors.push(UnsafeSelector {
                        commit_id: commit.id.clone(),
                        selection: selection.clone(),
                        reason: "spans_hunk_boundary".to_owned(),
                    });
                    continue;
                }
            }
            SelectionSpec::File { path } | SelectionSpec::Directory { prefix: path } => {
                if !scan_has_path_or_prefix(scan_paths, path) {
                    unknown_paths.insert(path.clone());
                    continue;
                }
            }
            SelectionSpec::Hunk { .. } => {} // resolved below
        }

        if let Ok(resolved) = resolve::resolve_selection(scan, &spec) {
            if let Some(file) = scan.files.iter().find(|f| f.path == resolved.file_path) {
                for idx in &resolved.hunk_indices {
                    if let Some(hunk) = file.hunks.get(*idx) {
                        coverage
                            .entry(hunk.hunk_id.clone())
                            .or_default()
                            .insert(commit_label.to_owned());
                    }
                }
            }
        } else if let SelectionSpec::Hunk { hunk_id } = &spec {
            // Unknown hunk id → surface via unknown_paths keyed by hunk id so the
            // agent sees it instead of silently mis-counting coverage.
            unknown_paths.insert(hunk_id.clone());
        }
    }
}

fn scan_has_path_or_prefix(scan_paths: &BTreeSet<String>, path: &str) -> bool {
    if scan_paths.contains(path) {
        return true;
    }
    let prefix = format!("{path}/");
    scan_paths.iter().any(|p| p.starts_with(&prefix))
}

fn ranges_span_hunk_boundary(scan: &ScanResult, path: &str, ranges: &[LineRange]) -> bool {
    let Some(file) = scan.files.iter().find(|f| f.path == path) else {
        return false;
    };
    for range in ranges {
        let mut hits = 0usize;
        for hunk in &file.hunks {
            let (hunk_start, hunk_end) = if hunk.new_lines > 0 {
                (
                    hunk.new_start,
                    hunk.new_start + hunk.new_lines.saturating_sub(1),
                )
            } else {
                (
                    hunk.old_start,
                    hunk.old_start + hunk.old_lines.saturating_sub(1),
                )
            };
            if range.start <= hunk_end && range.end >= hunk_start {
                hits += 1;
                if hits >= 2 {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        CommitPlan, DiffLineInfo, FileInfo, FileStatus, HunkInfo, LineOrigin, PlannedCommit,
        ScanResult, ScanSummary,
    };

    fn hunk(id: &str, new_start: u32, new_lines: u32) -> HunkInfo {
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
            checksum: "c".into(),
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
    fn overlap_is_flagged_when_two_commits_claim_same_hunk() {
        let scan = scan_one_file("f.rs", vec![hunk("aaa111bbb222", 1, 3)]);
        let plan = CommitPlan {
            version: "v1".into(),
            commits: vec![
                PlannedCommit {
                    id: Some("A".into()),
                    selections: vec!["f.rs".into()],
                    exclude: Vec::new(),
                    message: None,
                },
                PlannedCommit {
                    id: Some("B".into()),
                    selections: vec!["aaa111bbb222".into()],
                    exclude: Vec::new(),
                    message: None,
                },
            ],
        };
        let result = check_plan(&plan, &scan);
        assert_eq!(result.overlaps.len(), 1);
        assert_eq!(result.overlaps[0].hunk_id, "aaa111bbb222");
        assert!(result.uncovered.is_empty());
    }

    #[test]
    fn empty_plan_flags_every_hunk_uncovered() {
        let scan = scan_one_file(
            "f.rs",
            vec![hunk("id00000000a", 1, 1), hunk("id00000000b", 10, 1)],
        );
        let plan = CommitPlan {
            version: "v1".into(),
            commits: Vec::new(),
        };
        let result = check_plan(&plan, &scan);
        assert_eq!(result.uncovered.len(), 2);
        assert!(result.has_issues());
    }

    #[test]
    fn unknown_path_recorded_when_selection_misses_scan() {
        let scan = scan_one_file("f.rs", vec![hunk("aaa111bbb222", 1, 1)]);
        let plan = CommitPlan {
            version: "v1".into(),
            commits: vec![PlannedCommit {
                id: Some("ghost".into()),
                selections: vec!["does/not/exist.rs".into()],
                exclude: Vec::new(),
                message: None,
            }],
        };
        let result = check_plan(&plan, &scan);
        assert!(
            result
                .unknown_paths
                .contains(&"does/not/exist.rs".to_owned()),
            "expected ghost path in unknown_paths, got: {:?}",
            result.unknown_paths
        );
    }

    #[test]
    fn line_range_spanning_two_hunks_flags_unsafe() {
        let scan = scan_one_file(
            "f.rs",
            vec![hunk("h1xxxxxxxxx", 3, 1), hunk("h2xxxxxxxxx", 35, 1)],
        );
        let plan = CommitPlan {
            version: "v1".into(),
            commits: vec![PlannedCommit {
                id: Some("wide".into()),
                selections: vec!["f.rs:1-40".into()],
                exclude: Vec::new(),
                message: None,
            }],
        };
        let result = check_plan(&plan, &scan);
        assert_eq!(result.unsafe_selectors.len(), 1);
        assert_eq!(result.unsafe_selectors[0].reason, "spans_hunk_boundary");
    }

    #[test]
    fn commit_plan_tolerates_unknown_fields_and_defaults() {
        let raw = r#"{
            "version": "v1",
            "commits": [ { "selections": ["f.rs"], "future_field": 42 } ]
        }"#;
        let plan: CommitPlan = serde_json::from_str(raw).expect("deserialize with unknown field");
        assert_eq!(plan.commits.len(), 1);
        assert!(plan.commits[0].id.is_none());
        assert!(plan.commits[0].exclude.is_empty());
        assert!(plan.commits[0].message.is_none());
    }
}
