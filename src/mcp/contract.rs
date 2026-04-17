use rmcp::{
    model::{CallToolResult, Content, TaskSupport, Tool, ToolAnnotations, ToolExecution},
    schemars::{self, JsonSchema},
};
use serde::{Deserialize, Serialize};

use crate::{
    cmd::mcp_adapter::{
        McpAdapterError, McpCommitRequest, McpLogRequest, McpOverviewRequest, McpPlanCheckRequest,
        McpPlanDiffRequest, McpScanRequest, McpSplitHunkRequest, McpStageRequest, McpStatusRequest,
        McpTypedOutput, McpUnstageRequest,
    },
    error::PgsError,
    models::CommitPlan,
    output::view::{
        CommitOutput, LogOutput, OperationOutput, OutputCommand, OverviewOutput, PlanCheckOutput,
        PlanDiffOutput, ScanOutput, SplitHunkOutput, StatusOutput,
    },
};

/// MCP tool name for repository scan operations.
pub const PGS_SCAN_TOOL: &str = "pgs_scan";
/// MCP tool name for staged-status operations.
pub const PGS_STATUS_TOOL: &str = "pgs_status";
/// MCP tool name for staging operations.
pub const PGS_STAGE_TOOL: &str = "pgs_stage";
/// MCP tool name for unstaging operations.
pub const PGS_UNSTAGE_TOOL: &str = "pgs_unstage";
/// MCP tool name for commit creation operations.
pub const PGS_COMMIT_TOOL: &str = "pgs_commit";
/// MCP tool name for commit log operations.
pub const PGS_LOG_TOOL: &str = "pgs_log";
/// MCP tool name for unified unstaged + staged overview operations.
pub const PGS_OVERVIEW_TOOL: &str = "pgs_overview";
/// MCP tool name for hunk run-classification (split-hunk) operations.
pub const PGS_SPLIT_HUNK_TOOL: &str = "pgs_split_hunk";
/// MCP tool name for commit-plan validation operations.
pub const PGS_PLAN_CHECK_TOOL: &str = "pgs_plan_check";
/// MCP tool name for plan-diff (saved plan reconciliation against fresh scan).
pub const PGS_PLAN_DIFF_TOOL: &str = "pgs_plan_diff";

const DEFAULT_CONTEXT: u32 = 3;

/// JSON input schema for the `pgs_scan` MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ScanToolInput {
    /// Explicit repository path to inspect.
    pub repo_path: String,
    /// Optional diff context override.
    pub context: Option<u32>,
    /// Optional file filters.
    pub files: Option<Vec<String>>,
    /// Whether to return full line-level hunk content.
    pub full: Option<bool>,
}

impl From<ScanToolInput> for McpScanRequest {
    fn from(value: ScanToolInput) -> Self {
        Self {
            repo_path: value.repo_path,
            context: value.context.unwrap_or(DEFAULT_CONTEXT),
            files: value.files.unwrap_or_default(),
            full: value.full.unwrap_or(false),
        }
    }
}

/// JSON input schema for the `pgs_status` MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct StatusToolInput {
    /// Explicit repository path to inspect.
    pub repo_path: String,
    /// Optional diff context override.
    pub context: Option<u32>,
}

impl From<StatusToolInput> for McpStatusRequest {
    fn from(value: StatusToolInput) -> Self {
        Self {
            repo_path: value.repo_path,
            context: value.context.unwrap_or(DEFAULT_CONTEXT),
        }
    }
}

/// JSON input schema for the `pgs_stage` MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct StageToolInput {
    /// Explicit repository path to mutate.
    pub repo_path: String,
    /// File, hunk, or line-range selections to stage.
    pub selections: Vec<String>,
    /// Optional selections to exclude.
    pub exclude: Option<Vec<String>>,
    /// Whether to validate without mutating the index.
    pub dry_run: Option<bool>,
    /// Optional diff context override used while resolving selections.
    pub context: Option<u32>,
}

impl From<StageToolInput> for McpStageRequest {
    fn from(value: StageToolInput) -> Self {
        Self {
            repo_path: value.repo_path,
            selections: value.selections,
            exclude: value.exclude.unwrap_or_default(),
            dry_run: value.dry_run.unwrap_or(false),
            context: value.context.unwrap_or(DEFAULT_CONTEXT),
        }
    }
}

/// JSON input schema for the `pgs_unstage` MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct UnstageToolInput {
    /// Explicit repository path to mutate.
    pub repo_path: String,
    /// File, hunk, or line-range selections to unstage.
    pub selections: Vec<String>,
    /// Optional selections to exclude.
    pub exclude: Option<Vec<String>>,
    /// Whether to validate without mutating the index.
    pub dry_run: Option<bool>,
    /// Optional diff context override used while resolving selections.
    pub context: Option<u32>,
}

impl From<UnstageToolInput> for McpUnstageRequest {
    fn from(value: UnstageToolInput) -> Self {
        Self {
            repo_path: value.repo_path,
            selections: value.selections,
            exclude: value.exclude.unwrap_or_default(),
            dry_run: value.dry_run.unwrap_or(false),
            context: value.context.unwrap_or(DEFAULT_CONTEXT),
        }
    }
}

/// JSON input schema for the `pgs_commit` MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct CommitToolInput {
    /// Explicit repository path to mutate.
    pub repo_path: String,
    #[schemars(length(min = 1))]
    /// Non-empty commit message.
    pub message: String,
}

impl From<CommitToolInput> for McpCommitRequest {
    fn from(value: CommitToolInput) -> Self {
        Self {
            repo_path: value.repo_path,
            message: value.message,
        }
    }
}

/// JSON input schema for the `pgs_log` MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct LogToolInput {
    /// Explicit repository path to inspect.
    pub repo_path: String,
    /// Maximum number of commits to return.
    pub max_count: Option<u32>,
    /// Optional file path filters.
    pub paths: Option<Vec<String>>,
}

impl From<LogToolInput> for McpLogRequest {
    fn from(value: LogToolInput) -> Self {
        Self {
            repo_path: value.repo_path,
            max_count: value.max_count.unwrap_or(20),
            paths: value.paths.unwrap_or_default(),
        }
    }
}

/// JSON input schema for the `pgs_overview` MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct OverviewToolInput {
    /// Explicit repository path to inspect.
    pub repo_path: String,
    /// Optional diff context override applied to both scan and status.
    pub context: Option<u32>,
}

impl From<OverviewToolInput> for McpOverviewRequest {
    fn from(value: OverviewToolInput) -> Self {
        Self {
            repo_path: value.repo_path,
            context: value.context.unwrap_or(DEFAULT_CONTEXT),
        }
    }
}

/// JSON input schema for the `pgs_split_hunk` MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SplitHunkToolInput {
    /// Explicit repository path to inspect.
    pub repo_path: String,
    /// 12-hex content-addressed hunk id from a prior `pgs_scan`.
    pub hunk_id: String,
    /// Optional diff context override.
    pub context: Option<u32>,
}

impl From<SplitHunkToolInput> for McpSplitHunkRequest {
    fn from(value: SplitHunkToolInput) -> Self {
        Self {
            repo_path: value.repo_path,
            hunk_id: value.hunk_id,
            context: value.context.unwrap_or(DEFAULT_CONTEXT),
        }
    }
}

/// JSON input schema for the `pgs_plan_check` MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct PlanCheckToolInput {
    /// Explicit repository path to inspect.
    pub repo_path: String,
    /// Agent-supplied `CommitPlan` to validate against a fresh scan.
    pub plan: CommitPlan,
    /// Optional diff context override.
    pub context: Option<u32>,
}

impl From<PlanCheckToolInput> for McpPlanCheckRequest {
    fn from(value: PlanCheckToolInput) -> Self {
        Self {
            repo_path: value.repo_path,
            plan: value.plan,
            context: value.context.unwrap_or(DEFAULT_CONTEXT),
        }
    }
}

/// JSON input schema for the `pgs_plan_diff` MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct PlanDiffToolInput {
    /// Explicit repository path to inspect.
    pub repo_path: String,
    /// Agent-supplied `CommitPlan` to reconcile against a fresh scan.
    pub plan: CommitPlan,
    /// Optional diff context override.
    pub context: Option<u32>,
}

impl From<PlanDiffToolInput> for McpPlanDiffRequest {
    fn from(value: PlanDiffToolInput) -> Self {
        Self {
            repo_path: value.repo_path,
            plan: value.plan,
            context: value.context.unwrap_or(DEFAULT_CONTEXT),
        }
    }
}

/// Outcome classification surfaced in MCP tool results.
#[derive(Debug, Clone, Copy, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolOutcome {
    /// The request completed with structured `pgs` output.
    Ok,
    /// The request completed successfully but matched no work.
    NoEffect,
    /// The request failed and carries `pgs_error` metadata.
    Error,
}

/// Stable MCP error category derived from the underlying `pgs` failure.
#[derive(Debug, Clone, Copy, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PgsToolErrorKind {
    /// The request was valid but produced no changes.
    NoEffect,
    /// The caller supplied invalid input.
    User,
    /// The caller can retry after refreshing state or waiting.
    Retryable,
    /// The server hit an unexpected internal failure.
    Internal,
}

/// Structured error payload preserved in MCP tool responses.
#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
pub struct PgsToolError {
    /// Coarse error category for policy and retry handling.
    pub kind: PgsToolErrorKind,
    /// Stable machine-readable error code.
    pub code: String,
    /// Human-readable error message.
    pub message: String,
    /// Original `pgs` exit code.
    pub exit_code: i32,
    /// Whether retrying may succeed without changing the request shape.
    pub retryable: bool,
    /// Short remediation guidance for the caller.
    pub guidance: String,
}

macro_rules! define_tool_output {
    ($name:ident, $pgs:ty) => {
        /// Structured MCP tool result envelope for the associated `pgs` payload.
        #[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
        pub struct $name {
            /// High-level request outcome.
            pub outcome: ToolOutcome,
            #[serde(skip_serializing_if = "Option::is_none")]
            /// Typed `pgs` payload when the request produced structured output.
            pub pgs: Option<$pgs>,
            #[serde(skip_serializing_if = "Option::is_none")]
            /// Stable error metadata when the request had no effect or failed.
            pub pgs_error: Option<PgsToolError>,
        }
    };
}

define_tool_output!(ScanToolOutput, ScanOutput);
define_tool_output!(StatusToolOutput, StatusOutput);
define_tool_output!(OperationToolOutput, OperationOutput);
define_tool_output!(CommitToolOutput, CommitOutput);
define_tool_output!(LogToolOutput, LogOutput);
define_tool_output!(OverviewToolOutput, OverviewOutput);
define_tool_output!(SplitHunkToolOutput, SplitHunkOutput);
define_tool_output!(PlanCheckToolOutput, PlanCheckOutput);
define_tool_output!(PlanDiffToolOutput, PlanDiffOutput);

/// Return the frozen MCP tool definitions exposed by `pgs-mcp`.
pub fn tool_definitions() -> Vec<Tool> {
    vec![
        scan_tool(),
        status_tool(),
        stage_tool(),
        unstage_tool(),
        commit_tool(),
        log_tool(),
        overview_tool(),
        split_hunk_tool(),
        plan_check_tool(),
        plan_diff_tool(),
    ]
}

/// Look up a frozen MCP tool definition by its MCP name.
pub fn tool_definition(name: &str) -> Option<Tool> {
    tool_definitions()
        .into_iter()
        .find(|tool| tool.name.as_ref() == name)
}

/// Map adapter execution output into the MCP tool result envelope.
///
/// # Errors
///
/// Returns [`PgsError`] if the structured MCP response payload cannot be
/// serialized while building the final `CallToolResult`.
pub fn map_execution_result(
    result: Result<McpTypedOutput, McpAdapterError>,
) -> Result<CallToolResult, PgsError> {
    match result {
        Ok(output) => success_result(output),
        Err(error) if is_no_effect(&error.source) => no_effect_result(&error),
        Err(error) => error_result(&error),
    }
}

fn scan_tool() -> Tool {
    Tool::new(
        PGS_SCAN_TOOL,
        "Inspect unstaged working-tree changes for an explicit local repository path without mutating the repository.",
        serde_json::Map::new(),
    )
    .with_title("Scan repository changes")
    .with_input_schema::<ScanToolInput>()
    .with_output_schema::<ScanToolOutput>()
    .with_annotations(
        ToolAnnotations::new()
            .read_only(true)
            .destructive(false)
            .idempotent(true)
            .open_world(false),
    )
    .with_execution(ToolExecution::new().with_task_support(TaskSupport::Optional))
}

fn status_tool() -> Tool {
    Tool::new(
        PGS_STATUS_TOOL,
        "Inspect staged index changes for an explicit local repository path without mutating the repository.",
        serde_json::Map::new(),
    )
    .with_title("Show staged status")
    .with_input_schema::<StatusToolInput>()
    .with_output_schema::<StatusToolOutput>()
    .with_annotations(
        ToolAnnotations::new()
            .read_only(true)
            .destructive(false)
            .idempotent(true)
            .open_world(false),
    )
    .with_execution(ToolExecution::new().with_task_support(TaskSupport::Optional))
}

fn stage_tool() -> Tool {
    Tool::new(
        PGS_STAGE_TOOL,
        "Stage file, hunk, or line-range selections into the git index for an explicit local repository path. This mutates repository state.",
        serde_json::Map::new(),
    )
    .with_title("Stage selections")
    .with_input_schema::<StageToolInput>()
    .with_output_schema::<OperationToolOutput>()
    .with_annotations(
        ToolAnnotations::new()
            .read_only(false)
            .destructive(true)
            .idempotent(false)
            .open_world(false),
    )
    .with_execution(ToolExecution::new().with_task_support(TaskSupport::Forbidden))
}

fn unstage_tool() -> Tool {
    Tool::new(
        PGS_UNSTAGE_TOOL,
        "Remove file, hunk, or line-range selections from the git index for an explicit local repository path. This mutates repository state.",
        serde_json::Map::new(),
    )
    .with_title("Unstage selections")
    .with_input_schema::<UnstageToolInput>()
    .with_output_schema::<OperationToolOutput>()
    .with_annotations(
        ToolAnnotations::new()
            .read_only(false)
            .destructive(true)
            .idempotent(false)
            .open_world(false),
    )
    .with_execution(ToolExecution::new().with_task_support(TaskSupport::Forbidden))
}

fn commit_tool() -> Tool {
    Tool::new(
        PGS_COMMIT_TOOL,
        "Create a git commit from currently staged changes in an explicit local repository path. This mutates repository history.",
        serde_json::Map::new(),
    )
    .with_title("Create commit")
    .with_input_schema::<CommitToolInput>()
    .with_output_schema::<CommitToolOutput>()
    .with_annotations(
        ToolAnnotations::new()
            .read_only(false)
            .destructive(true)
            .idempotent(false)
            .open_world(false),
    )
    .with_execution(ToolExecution::new().with_task_support(TaskSupport::Forbidden))
}

fn log_tool() -> Tool {
    Tool::new(
        PGS_LOG_TOOL,
        "Retrieve recent commit history for an explicit local repository path without mutating the repository.",
        serde_json::Map::new(),
    )
    .with_title("Show commit log")
    .with_input_schema::<LogToolInput>()
    .with_output_schema::<LogToolOutput>()
    .with_annotations(
        ToolAnnotations::new()
            .read_only(true)
            .destructive(false)
            .idempotent(true)
            .open_world(false),
    )
    .with_execution(ToolExecution::new().with_task_support(TaskSupport::Optional))
}

fn overview_tool() -> Tool {
    Tool::new(
        PGS_OVERVIEW_TOOL,
        "Return a unified view of both unstaged (scan) and staged (status) changes for an explicit local repository path without mutating the repository.",
        serde_json::Map::new(),
    )
    .with_title("Overview of unstaged and staged changes")
    .with_input_schema::<OverviewToolInput>()
    .with_output_schema::<OverviewToolOutput>()
    .with_annotations(
        ToolAnnotations::new()
            .read_only(true)
            .destructive(false)
            .idempotent(true)
            .open_world(false),
    )
    .with_execution(ToolExecution::new().with_task_support(TaskSupport::Optional))
}

fn split_hunk_tool() -> Tool {
    Tool::new(
        PGS_SPLIT_HUNK_TOOL,
        "Classify a hunk's contiguous line runs (addition, deletion, mixed) for an explicit local repository path without mutating the repository. Descriptive output — does not stage or unstage.",
        serde_json::Map::new(),
    )
    .with_title("Classify hunk runs (split-hunk)")
    .with_input_schema::<SplitHunkToolInput>()
    .with_output_schema::<SplitHunkToolOutput>()
    .with_annotations(
        ToolAnnotations::new()
            .read_only(true)
            .destructive(false)
            .idempotent(true)
            .open_world(false),
    )
    .with_execution(ToolExecution::new().with_task_support(TaskSupport::Optional))
}

fn plan_check_tool() -> Tool {
    Tool::new(
        PGS_PLAN_CHECK_TOOL,
        "Validate an agent-supplied CommitPlan against a fresh scan for an explicit local repository path without mutating the repository. Reports overlaps, uncovered hunks, unsafe selectors (line ranges crossing hunk boundaries), and unknown paths.",
        serde_json::Map::new(),
    )
    .with_title("Validate commit plan")
    .with_input_schema::<PlanCheckToolInput>()
    .with_output_schema::<PlanCheckToolOutput>()
    .with_annotations(
        ToolAnnotations::new()
            .read_only(true)
            .destructive(false)
            .idempotent(true)
            .open_world(false),
    )
    .with_execution(ToolExecution::new().with_task_support(TaskSupport::Optional))
}

fn plan_diff_tool() -> Tool {
    Tool::new(
        PGS_PLAN_DIFF_TOOL,
        "Reconcile a saved CommitPlan against a fresh scan of an explicit local repository path without mutating the repository. Classifies each planned selection as still_valid, shifted (content moved to a new hunk id), or gone (no matching change).",
        serde_json::Map::new(),
    )
    .with_title("Diff saved commit plan")
    .with_input_schema::<PlanDiffToolInput>()
    .with_output_schema::<PlanDiffToolOutput>()
    .with_annotations(
        ToolAnnotations::new()
            .read_only(true)
            .destructive(false)
            .idempotent(true)
            .open_world(false),
    )
    .with_execution(ToolExecution::new().with_task_support(TaskSupport::Optional))
}

fn log_summary_text(log: &LogOutput) -> String {
    if log.truncated {
        format!(
            "Found {} commit(s) (truncated; walk limit reached).",
            log.total
        )
    } else {
        format!("Found {} commit(s).", log.total)
    }
}

fn success_result(output: McpTypedOutput) -> Result<CallToolResult, PgsError> {
    match output {
        McpTypedOutput::Scan(scan) => structured_tool_result(
            ScanToolOutput {
                outcome: ToolOutcome::Ok,
                pgs: Some(scan.clone()),
                pgs_error: None,
            },
            scan_summary_text(&scan),
            false,
        ),
        McpTypedOutput::Operation(operation) => structured_tool_result(
            OperationToolOutput {
                outcome: ToolOutcome::Ok,
                pgs: Some(operation.clone()),
                pgs_error: None,
            },
            operation_summary_text(&operation),
            false,
        ),
        McpTypedOutput::Status(status) => structured_tool_result(
            StatusToolOutput {
                outcome: ToolOutcome::Ok,
                pgs: Some(status.clone()),
                pgs_error: None,
            },
            status_summary_text(&status),
            false,
        ),
        McpTypedOutput::Commit(commit) => structured_tool_result(
            CommitToolOutput {
                outcome: ToolOutcome::Ok,
                pgs: Some(commit.clone()),
                pgs_error: None,
            },
            commit_summary_text(&commit),
            false,
        ),
        McpTypedOutput::Log(log) => structured_tool_result(
            LogToolOutput {
                outcome: ToolOutcome::Ok,
                pgs: Some(log.clone()),
                pgs_error: None,
            },
            log_summary_text(&log),
            false,
        ),
        McpTypedOutput::Overview(overview) => structured_tool_result(
            OverviewToolOutput {
                outcome: ToolOutcome::Ok,
                pgs: Some(overview.clone()),
                pgs_error: None,
            },
            overview_summary_text(&overview),
            false,
        ),
        McpTypedOutput::SplitHunk(split) => structured_tool_result(
            SplitHunkToolOutput {
                outcome: ToolOutcome::Ok,
                pgs: Some(split.clone()),
                pgs_error: None,
            },
            split_hunk_summary_text(&split),
            false,
        ),
        McpTypedOutput::PlanCheck(plan_check) => structured_tool_result(
            PlanCheckToolOutput {
                outcome: ToolOutcome::Ok,
                pgs: Some(plan_check.clone()),
                pgs_error: None,
            },
            plan_check_summary_text(&plan_check),
            false,
        ),
        McpTypedOutput::PlanDiff(plan_diff) => structured_tool_result(
            PlanDiffToolOutput {
                outcome: ToolOutcome::Ok,
                pgs: Some(plan_diff.clone()),
                pgs_error: None,
            },
            plan_diff_summary_text(&plan_diff),
            false,
        ),
    }
}

fn no_effect_result(error: &McpAdapterError) -> Result<CallToolResult, PgsError> {
    let pgs_error = build_pgs_error(error);
    let text = no_effect_text(&error.source);

    match error.command {
        OutputCommand::Scan => structured_tool_result(
            ScanToolOutput {
                outcome: ToolOutcome::NoEffect,
                pgs: None,
                pgs_error: Some(pgs_error),
            },
            text,
            false,
        ),
        OutputCommand::Stage | OutputCommand::Unstage => structured_tool_result(
            OperationToolOutput {
                outcome: ToolOutcome::NoEffect,
                pgs: None,
                pgs_error: Some(pgs_error),
            },
            text,
            false,
        ),
        OutputCommand::Commit => structured_tool_result(
            CommitToolOutput {
                outcome: ToolOutcome::NoEffect,
                pgs: None,
                pgs_error: Some(pgs_error),
            },
            text,
            false,
        ),
        OutputCommand::Status => structured_tool_result(
            StatusToolOutput {
                outcome: ToolOutcome::NoEffect,
                pgs: None,
                pgs_error: Some(pgs_error),
            },
            text,
            false,
        ),
        OutputCommand::Log => structured_tool_result(
            LogToolOutput {
                outcome: ToolOutcome::NoEffect,
                pgs: None,
                pgs_error: Some(pgs_error),
            },
            text,
            false,
        ),
        OutputCommand::Overview => structured_tool_result(
            OverviewToolOutput {
                outcome: ToolOutcome::NoEffect,
                pgs: None,
                pgs_error: Some(pgs_error),
            },
            text,
            false,
        ),
        OutputCommand::SplitHunk => structured_tool_result(
            SplitHunkToolOutput {
                outcome: ToolOutcome::NoEffect,
                pgs: None,
                pgs_error: Some(pgs_error),
            },
            text,
            false,
        ),
        OutputCommand::PlanCheck => structured_tool_result(
            PlanCheckToolOutput {
                outcome: ToolOutcome::NoEffect,
                pgs: None,
                pgs_error: Some(pgs_error),
            },
            text,
            false,
        ),
        OutputCommand::PlanDiff => structured_tool_result(
            PlanDiffToolOutput {
                outcome: ToolOutcome::NoEffect,
                pgs: None,
                pgs_error: Some(pgs_error),
            },
            text,
            false,
        ),
    }
}

fn error_result(error: &McpAdapterError) -> Result<CallToolResult, PgsError> {
    let pgs_error = build_pgs_error(error);
    let text = format!("{} Guidance: {}", error.source, pgs_error.guidance);

    match error.command {
        OutputCommand::Scan => structured_tool_result(
            ScanToolOutput {
                outcome: ToolOutcome::Error,
                pgs: None,
                pgs_error: Some(pgs_error),
            },
            text,
            true,
        ),
        OutputCommand::Stage | OutputCommand::Unstage => structured_tool_result(
            OperationToolOutput {
                outcome: ToolOutcome::Error,
                pgs: None,
                pgs_error: Some(pgs_error),
            },
            text,
            true,
        ),
        OutputCommand::Commit => structured_tool_result(
            CommitToolOutput {
                outcome: ToolOutcome::Error,
                pgs: None,
                pgs_error: Some(pgs_error),
            },
            text,
            true,
        ),
        OutputCommand::Status => structured_tool_result(
            StatusToolOutput {
                outcome: ToolOutcome::Error,
                pgs: None,
                pgs_error: Some(pgs_error),
            },
            text,
            true,
        ),
        OutputCommand::Log => structured_tool_result(
            LogToolOutput {
                outcome: ToolOutcome::Error,
                pgs: None,
                pgs_error: Some(pgs_error),
            },
            text,
            true,
        ),
        OutputCommand::Overview => structured_tool_result(
            OverviewToolOutput {
                outcome: ToolOutcome::Error,
                pgs: None,
                pgs_error: Some(pgs_error),
            },
            text,
            true,
        ),
        OutputCommand::SplitHunk => structured_tool_result(
            SplitHunkToolOutput {
                outcome: ToolOutcome::Error,
                pgs: None,
                pgs_error: Some(pgs_error),
            },
            text,
            true,
        ),
        OutputCommand::PlanCheck => structured_tool_result(
            PlanCheckToolOutput {
                outcome: ToolOutcome::Error,
                pgs: None,
                pgs_error: Some(pgs_error),
            },
            text,
            true,
        ),
        OutputCommand::PlanDiff => structured_tool_result(
            PlanDiffToolOutput {
                outcome: ToolOutcome::Error,
                pgs: None,
                pgs_error: Some(pgs_error),
            },
            text,
            true,
        ),
    }
}

fn structured_tool_result<T: Serialize>(
    structured: T,
    text: String,
    is_error: bool,
) -> Result<CallToolResult, PgsError> {
    let structured_content = serde_json::to_value(structured)?;
    let mut result = if is_error {
        CallToolResult::structured_error(structured_content)
    } else {
        CallToolResult::structured(structured_content)
    };

    result.content = vec![Content::text(text)];

    Ok(result)
}

fn build_pgs_error(error: &McpAdapterError) -> PgsToolError {
    let kind = match error.source {
        PgsError::NoChanges | PgsError::SelectionEmpty => PgsToolErrorKind::NoEffect,
        PgsError::InvalidSelection { .. }
        | PgsError::InvalidLineRange { .. }
        | PgsError::UnknownHunkId { .. }
        | PgsError::FileNotInDiff { .. }
        | PgsError::BinaryFileGranular { .. }
        | PgsError::GranularOnWholeFile { .. }
        | PgsError::ExplainWithoutDryRun => PgsToolErrorKind::User,
        PgsError::StaleScan { .. } | PgsError::IndexLocked | PgsError::StagingFailed { .. } => {
            PgsToolErrorKind::Retryable
        }
        PgsError::WorkdirMismatch { .. }
        | PgsError::Git(_)
        | PgsError::Io { .. }
        | PgsError::Json(_)
        | PgsError::Internal(_) => PgsToolErrorKind::Internal,
    };

    PgsToolError {
        kind,
        code: error.code.to_owned(),
        message: error.source.to_string(),
        exit_code: error.exit_code,
        retryable: matches!(kind, PgsToolErrorKind::Retryable),
        guidance: error_guidance(&error.source),
    }
}

const fn is_no_effect(error: &PgsError) -> bool {
    matches!(error, PgsError::NoChanges | PgsError::SelectionEmpty)
}

fn scan_summary_text(scan: &ScanOutput) -> String {
    format!(
        "Found {} unstaged file(s) across {} hunk(s).",
        scan.summary.total_files, scan.summary.total_hunks
    )
}

fn operation_summary_text(operation: &OperationOutput) -> String {
    let verb = match operation.command {
        OutputCommand::Stage => "Staged",
        OutputCommand::Unstage => "Unstaged",
        OutputCommand::Scan
        | OutputCommand::Status
        | OutputCommand::Commit
        | OutputCommand::Log
        | OutputCommand::Overview
        | OutputCommand::SplitHunk
        | OutputCommand::PlanCheck
        | OutputCommand::PlanDiff => "Applied",
    };
    format!("{verb} {} selection(s).", operation.items.len())
}

fn plan_check_summary_text(output: &PlanCheckOutput) -> String {
    format!(
        "Plan check: {} overlap(s), {} uncovered, {} unsafe selector(s), {} unknown path(s).",
        output.overlaps.len(),
        output.uncovered.len(),
        output.unsafe_selectors.len(),
        output.unknown_paths.len()
    )
}

fn plan_diff_summary_text(output: &PlanDiffOutput) -> String {
    format!(
        "Plan diff: {} still valid, {} shifted, {} gone.",
        output.still_valid.len(),
        output.shifted.len(),
        output.gone.len()
    )
}

fn split_hunk_summary_text(split: &SplitHunkOutput) -> String {
    format!(
        "Classified hunk {} into {} run(s).",
        split.hunk_id,
        split.ranges.len()
    )
}

fn status_summary_text(status: &StatusOutput) -> String {
    format!(
        "Found {} staged file(s), {} addition(s), and {} deletion(s).",
        status.summary.total_files, status.summary.total_additions, status.summary.total_deletions
    )
}

fn overview_summary_text(overview: &OverviewOutput) -> String {
    format!(
        "Overview: {} unstaged file(s), {} staged file(s).",
        overview.unstaged.summary.total_files, overview.staged.summary.total_files
    )
}

fn commit_summary_text(commit: &CommitOutput) -> String {
    let short_hash: String = commit.commit_hash.chars().take(12).collect();
    format!(
        "Created commit {short_hash} affecting {} file(s).",
        commit.files_changed
    )
}

fn no_effect_text(error: &PgsError) -> String {
    match error {
        PgsError::NoChanges => {
            "The request completed successfully, but there were no changes to apply.".to_owned()
        }
        PgsError::SelectionEmpty => {
            "The request completed successfully, but the provided selections matched nothing."
                .to_owned()
        }
        _ => error.to_string(),
    }
}

fn error_guidance(error: &PgsError) -> String {
    match error {
        PgsError::NoChanges => {
            "Check the repository state or narrow the request before retrying.".to_owned()
        }
        PgsError::SelectionEmpty => {
            "Run pgs_scan again and refresh the file, hunk, or line-range selections.".to_owned()
        }
        PgsError::InvalidSelection { .. } => {
            "Use a file path, 12-hex hunk ID, or path:line-range selection.".to_owned()
        }
        PgsError::InvalidLineRange { .. } => {
            "Use 1-indexed inclusive line ranges that exist in the current file diff.".to_owned()
        }
        PgsError::UnknownHunkId { .. } | PgsError::FileNotInDiff { .. } => {
            "Run pgs_scan again and retry with a current hunk ID or file path.".to_owned()
        }
        PgsError::BinaryFileGranular { .. } | PgsError::GranularOnWholeFile { .. } => {
            "Retry with a file-level selection instead of hunk or line granularity.".to_owned()
        }
        PgsError::ExplainWithoutDryRun => {
            "Pass --dry-run alongside --explain, or drop --explain.".to_owned()
        }
        PgsError::StaleScan { .. } => {
            "Re-run pgs_scan to refresh checksums and hunk IDs, then retry.".to_owned()
        }
        PgsError::IndexLocked => {
            "Wait for the git index lock to clear, then retry the request.".to_owned()
        }
        PgsError::StagingFailed { .. } => {
            "Retry the request once the repository index is stable.".to_owned()
        }
        PgsError::WorkdirMismatch { .. }
        | PgsError::Git(_)
        | PgsError::Io { .. }
        | PgsError::Json(_)
        | PgsError::Internal(_) => {
            "Retry once; if the failure persists, inspect repository state and server logs."
                .to_owned()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::cmd::mcp_adapter::McpAdapterError;

    fn required_fields(tool: &Tool) -> Vec<String> {
        tool.input_schema
            .get("required")
            .and_then(serde_json::Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn mcp_tool_schemas_require_repo_path() {
        let tools = tool_definitions();
        let tool_names: Vec<&str> = tools.iter().map(|tool| tool.name.as_ref()).collect();

        assert_eq!(
            tool_names,
            vec![
                PGS_SCAN_TOOL,
                PGS_STATUS_TOOL,
                PGS_STAGE_TOOL,
                PGS_UNSTAGE_TOOL,
                PGS_COMMIT_TOOL,
                PGS_LOG_TOOL,
                PGS_OVERVIEW_TOOL,
                PGS_SPLIT_HUNK_TOOL,
                PGS_PLAN_CHECK_TOOL,
                PGS_PLAN_DIFF_TOOL,
            ]
        );

        for tool in tools {
            let required = required_fields(&tool);
            assert!(
                required.iter().any(|field| field == "repo_path"),
                "tool {} should require repo_path, schema: {}",
                tool.name,
                serde_json::Value::Object(tool.input_schema.as_ref().clone())
            );
        }
    }

    #[test]
    fn mcp_no_effect_maps_to_successful_tool_result() {
        let no_changes = map_execution_result(Err(McpAdapterError::new(
            OutputCommand::Scan,
            PgsError::NoChanges,
        )))
        .expect("no-effect result should serialize");
        assert_eq!(no_changes.is_error, Some(false));
        assert_eq!(
            no_changes.structured_content,
            Some(serde_json::json!({
                "outcome": "no_effect",
                "pgs_error": {
                    "kind": "no_effect",
                    "code": "no_changes",
                    "message": "no changes detected in working tree",
                    "exit_code": 1,
                    "retryable": false,
                    "guidance": "Check the repository state or narrow the request before retrying."
                }
            }))
        );

        let selection_empty = map_execution_result(Err(McpAdapterError::new(
            OutputCommand::Stage,
            PgsError::SelectionEmpty,
        )))
        .expect("no-effect result should serialize");
        assert_eq!(selection_empty.is_error, Some(false));
        assert_eq!(
            selection_empty.structured_content,
            Some(serde_json::json!({
                "outcome": "no_effect",
                "pgs_error": {
                    "kind": "no_effect",
                    "code": "selection_empty",
                    "message": "selection matched no hunks",
                    "exit_code": 1,
                    "retryable": false,
                    "guidance": "Run pgs_scan again and refresh the file, hunk, or line-range selections."
                }
            }))
        );
    }

    #[test]
    fn mcp_mutating_tools_forbid_task_support() {
        let scan = tool_definition(PGS_SCAN_TOOL).expect("scan tool should exist");
        let status = tool_definition(PGS_STATUS_TOOL).expect("status tool should exist");
        let stage = tool_definition(PGS_STAGE_TOOL).expect("stage tool should exist");
        let unstage = tool_definition(PGS_UNSTAGE_TOOL).expect("unstage tool should exist");
        let commit = tool_definition(PGS_COMMIT_TOOL).expect("commit tool should exist");
        let log = tool_definition(PGS_LOG_TOOL).expect("log tool should exist");
        let overview = tool_definition(PGS_OVERVIEW_TOOL).expect("overview tool should exist");

        let split_hunk =
            tool_definition(PGS_SPLIT_HUNK_TOOL).expect("split-hunk tool should exist");

        assert_eq!(scan.task_support(), TaskSupport::Optional);
        assert_eq!(status.task_support(), TaskSupport::Optional);
        assert_eq!(log.task_support(), TaskSupport::Optional);
        assert_eq!(overview.task_support(), TaskSupport::Optional);
        assert_eq!(split_hunk.task_support(), TaskSupport::Optional);
        assert_eq!(stage.task_support(), TaskSupport::Forbidden);
        assert_eq!(unstage.task_support(), TaskSupport::Forbidden);
        assert_eq!(commit.task_support(), TaskSupport::Forbidden);
    }

    #[test]
    fn mcp_overview_tool_is_read_only_and_requires_repo_path() {
        let overview = tool_definition(PGS_OVERVIEW_TOOL).expect("overview tool should exist");
        assert_eq!(overview.task_support(), TaskSupport::Optional);

        let annotations = overview
            .annotations
            .as_ref()
            .expect("overview tool should have annotations");
        assert_eq!(annotations.read_only_hint, Some(true));
        assert_eq!(annotations.destructive_hint, Some(false));
        assert_eq!(annotations.idempotent_hint, Some(true));

        assert!(required_fields(&overview).iter().any(|f| f == "repo_path"));
    }

    #[test]
    fn mcp_overview_no_effect_maps_to_overview_envelope() {
        let no_changes = map_execution_result(Err(McpAdapterError::new(
            OutputCommand::Overview,
            PgsError::NoChanges,
        )))
        .expect("no-effect result should serialize");
        assert_eq!(no_changes.is_error, Some(false));
        let structured = no_changes
            .structured_content
            .expect("overview no-effect must carry structured content");
        assert_eq!(structured["outcome"], "no_effect");
        assert_eq!(structured["pgs_error"]["code"], "no_changes");
    }

    #[test]
    fn mcp_log_tool_is_read_only() {
        let log = tool_definition(PGS_LOG_TOOL).expect("log tool should exist");
        assert_eq!(
            log.task_support(),
            TaskSupport::Optional,
            "log is read-only so task support should be Optional"
        );

        let annotations = log
            .annotations
            .as_ref()
            .expect("log tool should have annotations");
        assert_eq!(
            annotations.read_only_hint,
            Some(true),
            "log tool should be annotated as read-only"
        );
        assert_eq!(
            annotations.destructive_hint,
            Some(false),
            "log tool should not be annotated as destructive"
        );
    }
}
