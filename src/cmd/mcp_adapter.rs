use serde::{Deserialize, Serialize};

use crate::error::PgsError;
use crate::models::CommitPlan;
use crate::output::view::OutputCommand;

/// Typed command output returned by [`execute`]. The `output` module is
/// crate-private; this re-export is how library callers name the type.
pub use crate::output::view::CommandOutput;

use super::{commit, log, overview, plan_check, plan_diff, scan, split, stage, status, unstage};

/// Typed MCP payload for `pgs_scan` requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpScanRequest {
    /// Explicit repository path supplied by the MCP caller.
    pub repo_path: String,
    /// Unified diff context lines to request from the underlying scan command.
    pub context: u32,
    /// Optional file filters forwarded to the scan command.
    pub files: Vec<String>,
    /// Whether the scan should include full line-level hunk content.
    pub full: bool,
}

/// Typed MCP payload for `pgs_status` requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpStatusRequest {
    /// Explicit repository path supplied by the MCP caller.
    pub repo_path: String,
    /// Unified diff context lines to request from the status command.
    pub context: u32,
    /// Whether to include every staged hunk with its id and lines.
    pub full: bool,
}

/// Typed MCP payload for `pgs_stage` requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpStageRequest {
    /// Explicit repository path supplied by the MCP caller.
    pub repo_path: String,
    /// File, hunk, or line-range selections to stage.
    pub selections: Vec<String>,
    /// Selections to exclude from the staging request.
    pub exclude: Vec<String>,
    /// Whether to validate the request without mutating the index.
    pub dry_run: bool,
    /// Whether to include exact per-file preview lines. Requires `dry_run`.
    pub explain: bool,
    /// Per-file preview cap when `explain` is enabled.
    pub limit: u32,
    /// Unified diff context lines used while resolving selections.
    pub context: u32,
    /// Per-file checksums captured from a prior scan (path → SHA-256). When
    /// present for a file, `StaleScan` (exit 3) is returned if the file changed
    /// between the agent's scan and this stage call.
    pub expected_checksums: std::collections::HashMap<String, String>,
}

/// Typed MCP payload for `pgs_unstage` requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpUnstageRequest {
    /// Explicit repository path supplied by the MCP caller.
    pub repo_path: String,
    /// File, hunk, or line-range selections to remove from the index.
    pub selections: Vec<String>,
    /// Selections to exclude from the unstaging request.
    pub exclude: Vec<String>,
    /// Whether to validate the request without mutating the index.
    pub dry_run: bool,
    /// Unified diff context lines used while resolving selections.
    pub context: u32,
}

/// Typed MCP payload for `pgs_log` requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpLogRequest {
    /// Explicit repository path supplied by the MCP caller.
    pub repo_path: String,
    /// Maximum number of commits to return.
    pub max_count: u32,
    /// Optional file path filters forwarded to the log command.
    pub paths: Vec<String>,
}

/// Typed MCP payload for `pgs_commit` requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpCommitRequest {
    /// Explicit repository path supplied by the MCP caller.
    pub repo_path: String,
    /// Inline commit message, mutually exclusive with `message_file`.
    pub message: Option<String>,
    /// Path to a file holding the commit message, mutually exclusive with `message`.
    pub message_file: Option<String>,
    /// Whether to replace the current HEAD commit instead of creating a child commit.
    pub amend: bool,
    /// Whether to commit despite hooks pgs cannot run.
    pub no_verify: bool,
    /// Selections to stage before committing; empty commits the index as it is.
    pub selections: Vec<String>,
    /// Selections to leave out when staging.
    pub exclude: Vec<String>,
    /// Per-file checksums checked before staging.
    pub expected_checksums: std::collections::HashMap<String, String>,
    /// Unified diff context lines used while resolving selections.
    pub context: u32,
}

/// Typed MCP payload for `pgs_overview` requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpOverviewRequest {
    /// Explicit repository path supplied by the MCP caller.
    pub repo_path: String,
    /// Unified diff context lines forwarded to the scan and status stages.
    pub context: u32,
}

/// Typed MCP payload for `pgs_split_hunk` requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpSplitHunkRequest {
    /// Explicit repository path supplied by the MCP caller.
    pub repo_path: String,
    /// 12-hex hunk id to classify.
    pub hunk_id: String,
    /// Unified diff context lines for the fresh scan used during id resolution.
    pub context: u32,
}

/// Typed MCP payload for `pgs_plan_check` requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpPlanCheckRequest {
    /// Explicit repository path supplied by the MCP caller.
    pub repo_path: String,
    /// Agent-supplied plan to validate against a fresh scan.
    pub plan: CommitPlan,
    /// Unified diff context lines forwarded to the fresh scan.
    pub context: u32,
}

/// Typed MCP payload for `pgs_plan_diff` requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpPlanDiffRequest {
    /// Explicit repository path supplied by the MCP caller.
    pub repo_path: String,
    /// Agent-supplied plan to reconcile against a fresh scan.
    pub plan: CommitPlan,
    /// Unified diff context lines forwarded to the fresh scan.
    pub context: u32,
}

/// Typed MCP command routed into the existing CLI execution paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpCommandRequest {
    /// Run the scan command for unstaged changes.
    Scan(McpScanRequest),
    /// Run the status command for staged changes.
    Status(McpStatusRequest),
    /// Run the stage command for selected changes.
    Stage(McpStageRequest),
    /// Run the unstage command for selected changes.
    Unstage(McpUnstageRequest),
    /// Run the commit command for currently staged changes.
    Commit(McpCommitRequest),
    /// Run the log command for recent commit history.
    Log(McpLogRequest),
    /// Run the overview command (scan + status fusion).
    Overview(McpOverviewRequest),
    /// Classify a single hunk's contiguous runs (split-hunk).
    SplitHunk(McpSplitHunkRequest),
    /// Validate an agent-supplied commit plan against a fresh scan.
    PlanCheck(McpPlanCheckRequest),
    /// Reconcile a saved commit plan against a fresh scan (plan-diff).
    PlanDiff(McpPlanDiffRequest),
}

/// Stable MCP-facing error metadata derived from a CLI command failure.
#[derive(Debug)]
pub struct McpAdapterError {
    /// Command that produced the failure.
    pub command: OutputCommand,
    /// Original command error preserved for downstream mapping.
    pub source: PgsError,
}

impl McpAdapterError {
    pub(crate) const fn new(command: OutputCommand, source: PgsError) -> Self {
        Self { command, source }
    }

    /// Stable machine-readable error code of the underlying [`PgsError`].
    pub const fn code(&self) -> &'static str {
        self.source.code()
    }

    /// Exit code associated with the underlying [`PgsError`].
    pub const fn exit_code(&self) -> i32 {
        self.source.exit_code()
    }
}

/// Execute a typed MCP command through the existing CLI handlers.
///
/// This keeps MCP behavior aligned with the native CLI without reimplementing
/// git, selection, or output logic in a second code path.
///
/// # Errors
///
/// Returns [`McpAdapterError`] when the underlying command fails. The error
/// preserves the originating command, stable error code, exit code, and source
/// [`PgsError`] for later MCP result mapping.
pub fn execute(request: McpCommandRequest) -> Result<CommandOutput, McpAdapterError> {
    match request {
        McpCommandRequest::Scan(request) => scan::execute(
            Some(request.repo_path.as_str()),
            request.context.max(1),
            scan::ScanArgs {
                files: request.files,
                full: request.full,
            },
        )
        .map_err(|source| McpAdapterError::new(OutputCommand::Scan, source)),
        McpCommandRequest::Status(request) => status::execute(
            Some(request.repo_path.as_str()),
            request.context.max(1),
            request.full,
        )
        .map_err(|source| McpAdapterError::new(OutputCommand::Status, source)),
        McpCommandRequest::Stage(request) => stage::run(
            Some(request.repo_path.as_str()),
            request.context.max(1),
            &stage::StageRequest {
                selections: request.selections,
                exclude: request.exclude,
                dry_run: request.dry_run,
                explain: request.explain,
                limit: request.limit,
                expected_checksums: request.expected_checksums,
            },
        )
        .map_err(|source| McpAdapterError::new(OutputCommand::Stage, source)),
        McpCommandRequest::Unstage(request) => unstage::execute(
            Some(request.repo_path.as_str()),
            request.context.max(1),
            unstage::UnstageArgs {
                selections: request.selections,
                exclude: request.exclude,
                dry_run: request.dry_run,
            },
        )
        .map_err(|source| McpAdapterError::new(OutputCommand::Unstage, source)),
        McpCommandRequest::Commit(request) => commit::run(
            Some(request.repo_path.as_str()),
            request.context.max(1),
            &commit::CommitRequest {
                message: request.message,
                message_file: request.message_file,
                amend: request.amend,
                no_verify: request.no_verify,
                stage: (!request.selections.is_empty()).then(|| stage::StageRequest {
                    selections: request.selections,
                    exclude: request.exclude,
                    expected_checksums: request.expected_checksums,
                    ..stage::StageRequest::default()
                }),
            },
            false,
        )
        .map_err(|source| McpAdapterError::new(OutputCommand::Commit, source)),
        McpCommandRequest::Log(request) => log::execute(
            Some(request.repo_path.as_str()),
            log::LogArgs {
                max_count: request.max_count,
                paths: request.paths,
            },
        )
        .map_err(|source| McpAdapterError::new(OutputCommand::Log, source)),
        McpCommandRequest::Overview(request) => {
            overview::execute(Some(request.repo_path.as_str()), request.context.max(1))
                .map_err(|source| McpAdapterError::new(OutputCommand::Overview, source))
        }
        McpCommandRequest::SplitHunk(request) => split::execute(
            Some(request.repo_path.as_str()),
            request.context.max(1),
            split::SplitArgs {
                hunk_id: request.hunk_id,
            },
        )
        .map_err(|source| McpAdapterError::new(OutputCommand::SplitHunk, source)),
        McpCommandRequest::PlanCheck(request) => plan_check::run_with_plan(
            Some(request.repo_path.as_str()),
            request.context.max(1),
            &request.plan,
        )
        .map_err(|source| McpAdapterError::new(OutputCommand::PlanCheck, source)),
        McpCommandRequest::PlanDiff(request) => plan_diff::run_with_plan(
            Some(request.repo_path.as_str()),
            request.context.max(1),
            &request.plan,
        )
        .map_err(|source| McpAdapterError::new(OutputCommand::PlanDiff, source)),
    }
}
