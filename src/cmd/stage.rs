use std::collections::HashMap;

use clap::Args;
use git2::Repository;

use crate::cmd::work_plan::{self, WorkPlan};
use crate::error::PgsError;
use crate::git::staging::{line_selection_for, workdir_line_count};
use crate::git::{diff, read_head_mode, repo, staging};
use crate::models::{
    FileStatus, OperationPreview, OperationStatus, ResolvedSelection, ScanResult, SelectionSpec,
    format_selection,
};
use crate::output::view::{CommandOutput, OperationOutput, OutputCommand};
use crate::safety::{backup, lock};
use crate::selection::resolve;

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

/// A stage call in typed form, as the CLI, the MCP server and `commit` build it.
#[derive(Debug, Clone, Default)]
pub struct StageRequest {
    /// Selections to stage.
    pub selections: Vec<String>,
    /// Selections to leave out.
    pub exclude: Vec<String>,
    /// Validate without modifying the index.
    pub dry_run: bool,
    /// Include exact per-file preview lines; requires `dry_run`.
    pub explain: bool,
    /// Per-file preview cap (`0` = unlimited).
    pub limit: u32,
    /// Per-file SHA-256 each workdir file must still have (path to checksum).
    pub expected_checksums: HashMap<String, String>,
}

impl TryFrom<StageArgs> for StageRequest {
    type Error = PgsError;

    fn try_from(args: StageArgs) -> Result<Self, Self::Error> {
        Ok(Self {
            expected_checksums: parse_expect_pairs(&args.expect)?,
            selections: args.selections,
            exclude: args.exclude,
            dry_run: args.dry_run,
            explain: args.explain,
            limit: args.limit,
        })
    }
}

/// Run `pgs stage` from parsed CLI arguments.
///
/// # Errors
///
/// See [`run`]; also `InvalidSelection` for a malformed or duplicate `--expect`.
#[allow(clippy::needless_pass_by_value)] // clap dispatches Args by value
pub fn execute(
    repo_path: Option<&str>,
    context: u32,
    args: StageArgs,
) -> Result<CommandOutput, PgsError> {
    if args.explain && !args.dry_run {
        return Err(PgsError::ExplainWithoutDryRun);
    }
    run(repo_path, context, &StageRequest::try_from(args)?)
}

/// Stage `request` in the repository at `repo_path`.
///
/// # Errors
///
/// `ExplainWithoutDryRun`, repository and selection errors, `StaleScan` when a
/// file changed since the caller's scan, and any staging failure (the index is
/// restored from its backup first).
pub fn run(
    repo_path: Option<&str>,
    context: u32,
    request: &StageRequest,
) -> Result<CommandOutput, PgsError> {
    if request.explain && !request.dry_run {
        return Err(PgsError::ExplainWithoutDryRun);
    }
    let repository = repo::open(repo_path)?;
    apply(&repository, context, request).map(Into::into)
}

/// Stage `request` in an open repository. `request.explain` is not checked here.
///
/// # Errors
///
/// As [`run`], minus the explain check.
pub fn apply(
    repository: &Repository,
    context: u32,
    request: &StageRequest,
) -> Result<OperationOutput, PgsError> {
    lock::wait_for_lock_release(repository, 5)?;
    let d = diff::diff_index_to_workdir(repository, context)?;
    let scan = diff::build_scan_result(repository, &d, None)?;
    if scan.files.is_empty() {
        return Err(PgsError::NoChanges);
    }

    let line_count = |path: &str| workdir_line_count(repository, path);
    let plan = work_plan::plan(&scan, &request.selections, &request.exclude, &line_count)?;
    check_freshness(repository, &scan, &plan, &request.expected_checksums)?;
    let items = work_plan::report_items(&scan, &plan, &line_count);

    if request.dry_run {
        let output = OperationOutput::new(
            OutputCommand::Stage,
            OperationStatus::DryRun,
            items,
            vec![],
            None,
        );
        if request.explain {
            let previews = compute_previews(repository, &scan, &plan.per_file, request.limit)?;
            return Ok(output.with_previews(previews));
        }
        return Ok(output);
    }

    let backup_id = backup::create_backup(repository)?;
    let mut warnings = Vec::new();
    for (spec, resolved) in &plan.per_file {
        warnings.extend(symlink_warning(repository, &scan, spec, resolved));
        if let Err(error) = execute_single_stage(repository, &scan, spec, resolved) {
            return Err(backup::rollback(repository, &backup_id, error));
        }
    }

    Ok(OperationOutput::new(
        OutputCommand::Stage,
        OperationStatus::Ok,
        items,
        warnings,
        Some(backup_id),
    ))
}

/// Refuse to stage a file that changed since the caller's scan.
fn check_freshness(
    repository: &Repository,
    scan: &ScanResult,
    plan: &WorkPlan,
    expected: &HashMap<String, String>,
) -> Result<(), PgsError> {
    for path in expected.keys() {
        if !plan.per_file.iter().any(|(_, r)| r.file_path == *path) {
            return Err(PgsError::InvalidSelection {
                detail: format!("--expect path '{path}' is not part of the staging selection"),
            });
        }
    }
    for (_, resolved) in &plan.per_file {
        let expected_sha = expected.get(&resolved.file_path).map(String::as_str);
        resolve::validate_freshness(repository, scan, &resolved.file_path, expected_sha)?;
    }
    Ok(())
}

/// A symlink has one line (its target), so a hunk or line selector on it stages it whole.
fn symlink_warning(
    repository: &Repository,
    scan: &ScanResult,
    spec: &SelectionSpec,
    resolved: &ResolvedSelection,
) -> Option<String> {
    let granular = matches!(spec, SelectionSpec::Hunk { .. }) || resolved.line_ranges.is_some();
    if !granular {
        return None;
    }
    let path = &resolved.file_path;
    // An untracked symlink has no index mode yet; HEAD may still record one.
    let is_symlink = scan
        .files
        .iter()
        .any(|f| f.path == *path && f.new_mode == 0o120_000)
        || read_head_mode(repository, path).ok() == Some(0o120_000);
    is_symlink.then(|| format!("symlink '{path}' staged whole; line/hunk selection ignored"))
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

/// Stage one file's merged selection, choosing the write its status needs.
fn execute_single_stage(
    repo: &Repository,
    scan: &ScanResult,
    spec: &SelectionSpec,
    resolved: &ResolvedSelection,
) -> Result<(), PgsError> {
    let path = resolved.file_path.as_str();
    let file =
        scan.files
            .iter()
            .find(|f| f.path == path)
            .ok_or_else(|| PgsError::FileNotInDiff {
                path: path.to_owned(),
            })?;
    let mode_change = (file.old_mode != file.new_mode).then_some(file.new_mode);

    match &file.status {
        FileStatus::Deleted => staging::stage_deletion(repo, path),
        FileStatus::Renamed { old_path } => {
            staging::stage_rename(repo, old_path, path, Some(file.new_mode))
        }
        FileStatus::Added | FileStatus::TypeChanged => {
            staging::stage_file(repo, path, Some(file.new_mode)).map(drop)
        }
        FileStatus::Modified if file.is_binary => {
            staging::stage_file(repo, path, Some(file.new_mode)).map(drop)
        }
        FileStatus::Modified => {
            let whole_file = resolved.line_ranges.is_none()
                && !matches!(spec, SelectionSpec::Hunk { .. })
                && resolved.hunk_indices.len() == file.hunks.len();
            if whole_file {
                return staging::stage_file(repo, path, mode_change).map(drop);
            }
            let sel = line_selection_for(scan, resolved, workdir_line_count(repo, path));
            staging::stage_lines(repo, path, &sel, mode_change).map(drop)
        }
    }
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
