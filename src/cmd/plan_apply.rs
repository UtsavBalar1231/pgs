//! `pgs plan-apply`: turn a validated [`CommitPlan`] into commits, one per
//! planned commit, in order.
//!
//! Each planned commit goes through `commit <selections>`, so it is staged and
//! committed atomically. Line ranges and paths name workdir content, which
//! committing does not change, so they stay valid from one commit to the next.
//! Hunk ids do not: committing one hunk can shift the index-side position of a
//! later hunk in the same file, and the id hashes that position. Each hunk id is
//! therefore pinned up front to its file, workdir start line and content
//! checksum, all of which stay put, and re-resolved before every commit.

use std::collections::HashMap;

use clap::Args;
use git2::Repository;

use crate::cmd::commit::{self, CommitRequest};
use crate::cmd::stage::StageRequest;
use crate::cmd::{plan_check, plan_io, work_plan};
use crate::error::PgsError;
use crate::git::staging::workdir_line_count;
use crate::git::{diff, repo};
use crate::models::{CommitPlan, PlannedCommit, ScanResult, SelectionSpec};
use crate::output::view::{
    CommandOutput, OperationItemView, PlanAppliedCommit, PlanApplyFailure, PlanApplyOutput,
    PlanApplyStatus,
};
use crate::selection::parse::detect_selection;

#[derive(Args)]
pub struct PlanApplyArgs {
    /// Path to a `CommitPlan` JSON file. Mutually exclusive with `--stdin`.
    #[arg(long)]
    pub plan: Option<String>,
    /// Read the `CommitPlan` JSON from stdin. Default when `--plan` is omitted.
    #[arg(long, conflicts_with = "plan")]
    pub stdin: bool,
    /// Resolve every planned commit without staging or committing anything.
    #[arg(long)]
    pub dry_run: bool,
    /// Commit even though pre-commit, prepare-commit-msg or commit-msg hooks
    /// exist; pgs cannot run them.
    #[arg(long)]
    pub no_verify: bool,
}

/// How a plan is applied.
#[derive(Debug, Clone, Copy, Default)]
pub struct ApplyOptions {
    /// Resolve without writing.
    pub dry_run: bool,
    /// Commit despite hooks pgs cannot run.
    pub no_verify: bool,
}

/// A hunk id pinned to what stays put while earlier planned commits land.
struct HunkPin {
    path: String,
    new_start: u32,
    checksum: String,
}

/// Run `pgs plan-apply` from parsed CLI arguments.
///
/// # Errors
///
/// As [`run_with_plan`], plus plan loading errors.
#[allow(clippy::needless_pass_by_value)] // clap dispatches Args by value
pub fn execute(
    repo_path: Option<&str>,
    context: u32,
    args: PlanApplyArgs,
) -> Result<CommandOutput, PgsError> {
    let plan = plan_io::load_commit_plan(args.plan.as_deref())?;
    let options = ApplyOptions {
        dry_run: args.dry_run,
        no_verify: args.no_verify,
    };
    run_with_plan(repo_path, context, &plan, options)
}

/// Apply `plan` in the repository at `repo_path`.
///
/// Stops at the first planned commit that fails and reports it in `failed`;
/// the commits before it stay. Nothing is written on a dry run.
///
/// # Errors
///
/// Refuses before writing anything when a planned commit has no message, the
/// plan has no commits, something is already staged, commit hooks are active
/// (without `no_verify`), or plan-check reports overlaps, unsafe selectors,
/// unknown paths or unknown hunk ids. Repository errors propagate.
pub fn run_with_plan(
    repo_path: Option<&str>,
    context: u32,
    plan: &CommitPlan,
    options: ApplyOptions,
) -> Result<CommandOutput, PgsError> {
    let repository = repo::open(repo_path)?;
    let scan = check_preconditions(&repository, context, plan, options)?;
    // Resolving every planned commit before the first one lands refuses a plan
    // with a selection that names nothing, rather than stopping partway.
    let previews = plan
        .commits
        .iter()
        .map(|planned| preview(&repository, &scan, planned))
        .collect::<Result<Vec<_>, _>>()?;
    if options.dry_run {
        let commits = plan
            .commits
            .iter()
            .zip(previews)
            .enumerate()
            .map(|(index, (planned, staged))| PlanAppliedCommit {
                id: commit_label(planned, index),
                message: planned.message.clone().unwrap_or_default(),
                commit_hash: None,
                staged,
            })
            .collect();
        return Ok(PlanApplyOutput::new(PlanApplyStatus::DryRun, commits, None).into());
    }

    let pins = pin_hunks(&scan);
    let mut commits = Vec::with_capacity(plan.commits.len());
    for (index, planned) in plan.commits.iter().enumerate() {
        let id = commit_label(planned, index);
        match apply_one(repo_path, context, planned, &pins) {
            Ok(created) => commits.push(PlanAppliedCommit {
                id,
                message: created.message,
                commit_hash: Some(created.commit_hash),
                staged: created.staged.unwrap_or_default(),
            }),
            Err(error) => {
                let failed = PlanApplyFailure {
                    id,
                    code: error.code(),
                    message: error.to_string(),
                    exit_code: error.exit_code(),
                };
                return Ok(
                    PlanApplyOutput::new(PlanApplyStatus::Partial, commits, Some(failed)).into(),
                );
            }
        }
    }

    Ok(PlanApplyOutput::new(PlanApplyStatus::Applied, commits, None).into())
}

/// A planned commit's `id`, or its 1-based position when it has none.
fn commit_label(planned: &PlannedCommit, index: usize) -> String {
    planned
        .id
        .clone()
        .unwrap_or_else(|| format!("#{}", index + 1))
}

/// Everything that must hold before the first commit, so a plan that cannot
/// finish is refused before it starts. Returns the scan the plan was checked on.
fn check_preconditions(
    repository: &Repository,
    context: u32,
    plan: &CommitPlan,
    options: ApplyOptions,
) -> Result<ScanResult, PgsError> {
    if plan.commits.is_empty() {
        return Err(PgsError::InvalidArguments {
            detail: "the plan has no commits to apply".to_owned(),
        });
    }
    for (index, planned) in plan.commits.iter().enumerate() {
        if planned
            .message
            .as_deref()
            .is_none_or(|m| m.trim().is_empty())
        {
            let id = commit_label(planned, index);
            return Err(PgsError::InvalidArguments {
                detail: format!("planned commit {id} has no message; plan-apply needs one each"),
            });
        }
    }
    if !options.no_verify {
        let hooks = repo::active_commit_hooks(repository)?;
        if !hooks.is_empty() {
            return Err(PgsError::CommitHooksPresent { hooks });
        }
    }
    let staged = diff::diff_head_to_index(repository, context)?
        .deltas()
        .count();
    if staged > 0 {
        return Err(PgsError::InvalidArguments {
            detail: format!(
                "plan-apply starts from an empty index, but {staged} file(s) are staged; \
                 commit or unstage them first"
            ),
        });
    }

    let d = diff::diff_index_to_workdir(repository, context)?;
    let scan = diff::build_scan_result(repository, &d, None)?;
    if scan.files.is_empty() {
        return Err(PgsError::NoChanges);
    }
    let check = plan_check::check_plan(plan, &scan);
    let blocking = check.overlaps.len()
        + check.unsafe_selectors.len()
        + check.unknown_paths.len()
        + check.unknown_hunk_ids.len();
    if blocking > 0 {
        return Err(PgsError::InvalidArguments {
            detail: format!(
                "plan-check reports {} overlap(s), {} unsafe selector(s), {} unknown path(s) \
                 and {} unknown hunk id(s); run pgs plan-check for details",
                check.overlaps.len(),
                check.unsafe_selectors.len(),
                check.unknown_paths.len(),
                check.unknown_hunk_ids.len()
            ),
        });
    }
    Ok(scan)
}

fn pin_hunks(scan: &ScanResult) -> HashMap<String, HunkPin> {
    scan.files
        .iter()
        .flat_map(|file| {
            file.hunks.iter().map(|hunk| {
                let pin = HunkPin {
                    path: file.path.clone(),
                    new_start: hunk.new_start,
                    checksum: hunk.checksum.clone(),
                };
                (hunk.hunk_id.clone(), pin)
            })
        })
        .collect()
}

/// What a planned commit would stage, resolved against the starting scan.
fn preview(
    repository: &Repository,
    scan: &ScanResult,
    planned: &PlannedCommit,
) -> Result<Vec<OperationItemView>, PgsError> {
    let line_count = |path: &str| workdir_line_count(repository, path);
    let plan = work_plan::plan(scan, &planned.selections, &planned.exclude, &line_count)?;
    Ok(work_plan::report_items(scan, &plan, &line_count))
}

/// Stage and commit one planned commit against a fresh scan.
fn apply_one(
    repo_path: Option<&str>,
    context: u32,
    planned: &PlannedCommit,
    pins: &HashMap<String, HunkPin>,
) -> Result<crate::output::view::CommitOutput, PgsError> {
    let repository = repo::open(repo_path)?;
    let d = diff::diff_index_to_workdir(&repository, context)?;
    let scan = diff::build_scan_result(&repository, &d, None)?;
    let selections = current_selectors(&planned.selections, &scan, pins)?;
    let re_resolved: HashMap<String, &String> = selections
        .iter()
        .zip(&planned.selections)
        .filter(|(now, planned)| now != planned)
        .map(|(now, planned)| (now.clone(), planned))
        .collect();
    let request = CommitRequest {
        message: planned.message.clone(),
        // Hooks were checked once before the first commit.
        no_verify: true,
        stage: Some(StageRequest {
            selections,
            exclude: current_excludes(&planned.exclude, &scan, pins),
            ..StageRequest::default()
        }),
        ..CommitRequest::default()
    };
    let mut created = commit::commit(repo_path, context, &request, false)?;
    // Report each item under the selector the plan named, so callers can match
    // items to their plan.
    for item in created.staged.iter_mut().flatten() {
        if let Some(planned) = re_resolved.get(&item.selection) {
            item.selection.clone_from(planned);
        }
    }
    Ok(created)
}

/// Rewrite planned hunk ids to the ids the same hunks carry in `scan`.
fn current_selectors(
    selectors: &[String],
    scan: &ScanResult,
    pins: &HashMap<String, HunkPin>,
) -> Result<Vec<String>, PgsError> {
    selectors
        .iter()
        .map(|selector| match detect_selection(selector)? {
            SelectionSpec::Hunk { hunk_id } => {
                current_hunk_id(scan, pins, &hunk_id).ok_or(PgsError::UnknownHunkId { hunk_id })
            }
            _ => Ok(selector.clone()),
        })
        .collect()
}

/// Like [`current_selectors`], but an exclude whose hunk is already gone keeps
/// its stale id, which excludes nothing, as an unmatched `--exclude` does.
fn current_excludes(
    selectors: &[String],
    scan: &ScanResult,
    pins: &HashMap<String, HunkPin>,
) -> Vec<String> {
    selectors
        .iter()
        .map(|selector| match detect_selection(selector) {
            Ok(SelectionSpec::Hunk { hunk_id }) => {
                current_hunk_id(scan, pins, &hunk_id).unwrap_or(hunk_id)
            }
            _ => selector.clone(),
        })
        .collect()
}

fn current_hunk_id(
    scan: &ScanResult,
    pins: &HashMap<String, HunkPin>,
    hunk_id: &str,
) -> Option<String> {
    let pin = pins.get(hunk_id)?;
    scan.files
        .iter()
        .filter(|file| file.path == pin.path)
        .flat_map(|file| &file.hunks)
        .find(|hunk| hunk.new_start == pin.new_start && hunk.checksum == pin.checksum)
        .map(|hunk| hunk.hunk_id.clone())
}
