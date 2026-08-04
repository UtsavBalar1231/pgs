use std::sync::{Arc, LazyLock};

use rmcp::{
    model::{CallToolResult, ContentBlock, Tool, ToolAnnotations},
    schemars::{self, JsonSchema},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    /// Whether to include exact per-file preview lines. Requires `dry_run`.
    pub explain: Option<bool>,
    /// Per-file preview cap when `explain` is enabled.
    pub limit: Option<u32>,
    /// Optional diff context override used while resolving selections.
    pub context: Option<u32>,
    /// Per-file checksums from a prior scan (path → SHA-256). When present for
    /// a file, returns `StaleScan` (exit 3) if the file changed since the scan.
    pub expected_checksums: Option<std::collections::HashMap<String, String>>,
}

impl From<StageToolInput> for McpStageRequest {
    fn from(value: StageToolInput) -> Self {
        Self {
            repo_path: value.repo_path,
            selections: value.selections,
            exclude: value.exclude.unwrap_or_default(),
            dry_run: value.dry_run.unwrap_or(false),
            explain: value.explain.unwrap_or(false),
            limit: value.limit.unwrap_or(200),
            context: value.context.unwrap_or(DEFAULT_CONTEXT),
            expected_checksums: value.expected_checksums.unwrap_or_default(),
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
    /// Inline commit message. Supply exactly one of `message` or `message_file`.
    pub message: Option<String>,
    /// Path to a file holding the commit message. Supply exactly one of `message`
    /// or `message_file`. `-` is rejected: an MCP request has no stdin to read.
    pub message_file: Option<String>,
    /// Replace the current HEAD commit instead of creating a new child commit.
    pub amend: Option<bool>,
}

impl From<CommitToolInput> for McpCommitRequest {
    fn from(value: CommitToolInput) -> Self {
        Self {
            repo_path: value.repo_path,
            message: value.message,
            message_file: value.message_file,
            amend: value.amend.unwrap_or(false),
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

/// No-effect/error envelope. All typed output structs skip `pgs` when `None`, so the
/// JSON is command-agnostic: `{"outcome": ..., "pgs_error": ...}` for every command.
#[derive(Serialize)]
struct ToolOutcomeOnly {
    outcome: ToolOutcome,
    pgs_error: PgsToolError,
}

/// The tool set is frozen and schema sanitization is pure, so it is built once
/// and shared; `list_tools` would otherwise rebuild all ten definitions per tool.
static TOOL_DEFINITIONS: LazyLock<Vec<Tool>> = LazyLock::new(|| {
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
    .into_iter()
    .map(sanitize_tool_schemas)
    .collect()
});

/// Return the frozen MCP tool definitions exposed by `pgs-mcp`.
pub fn tool_definitions() -> Vec<Tool> {
    TOOL_DEFINITIONS.clone()
}

/// Look up a frozen MCP tool definition by its MCP name.
pub fn tool_definition(name: &str) -> Option<Tool> {
    TOOL_DEFINITIONS
        .iter()
        .find(|tool| tool.name.as_ref() == name)
        .cloned()
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
}

fn sanitize_tool_schemas(mut tool: Tool) -> Tool {
    let mut input_schema = Value::Object(tool.input_schema.as_ref().clone());
    strip_nonstandard_integer_formats(&mut input_schema);
    if let Value::Object(schema) = input_schema {
        tool.input_schema = Arc::new(schema);
    }

    if let Some(output_schema) = tool.output_schema.take() {
        let mut output_schema_value = Value::Object(output_schema.as_ref().clone());
        strip_nonstandard_integer_formats(&mut output_schema_value);
        tool.output_schema = match output_schema_value {
            Value::Object(schema) => Some(Arc::new(schema)),
            _ => Some(output_schema),
        };
    }

    tool
}

fn strip_nonstandard_integer_formats(value: &mut Value) {
    match value {
        Value::Object(object) => {
            if object
                .get("format")
                .and_then(Value::as_str)
                .is_some_and(is_nonstandard_integer_format)
            {
                object.remove("format");
            }

            for child in object.values_mut() {
                strip_nonstandard_integer_formats(child);
            }
        }
        Value::Array(items) => {
            for item in items {
                strip_nonstandard_integer_formats(item);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn is_nonstandard_integer_format(format: &str) -> bool {
    matches!(
        format,
        "int"
            | "int8"
            | "int16"
            | "int32"
            | "int64"
            | "isize"
            | "uint"
            | "uint8"
            | "uint16"
            | "uint32"
            | "uint64"
            | "usize"
    )
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
}

fn stage_tool() -> Tool {
    Tool::new(
        PGS_STAGE_TOOL,
        "Stage file, hunk, or line-range selections into the git index for an explicit local repository path. With dry_run and explain, returns exact preview lines without mutating repository state. Without dry_run, this mutates repository state.",
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
}

fn commit_tool() -> Tool {
    Tool::new(
        PGS_COMMIT_TOOL,
        "Create a git commit from currently staged changes, or amend the current HEAD when amend is true, in an explicit local repository path. This mutates repository history.",
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
    macro_rules! ok {
        ($wrapper:ident, $pgs:expr, $summary_fn:expr) => {{
            let pgs = $pgs;
            let text = $summary_fn(&pgs);
            structured_tool_result(
                $wrapper {
                    outcome: ToolOutcome::Ok,
                    pgs: Some(pgs),
                    pgs_error: None,
                },
                text,
                false,
            )
        }};
    }
    match output {
        McpTypedOutput::Scan(v) => ok!(ScanToolOutput, v, scan_summary_text),
        McpTypedOutput::Operation(v) => ok!(OperationToolOutput, v, operation_summary_text),
        McpTypedOutput::Status(v) => ok!(StatusToolOutput, v, status_summary_text),
        McpTypedOutput::Commit(v) => ok!(CommitToolOutput, v, commit_summary_text),
        McpTypedOutput::Log(v) => ok!(LogToolOutput, v, log_summary_text),
        McpTypedOutput::Overview(v) => ok!(OverviewToolOutput, v, overview_summary_text),
        McpTypedOutput::SplitHunk(v) => ok!(SplitHunkToolOutput, v, split_hunk_summary_text),
        McpTypedOutput::PlanCheck(v) => ok!(PlanCheckToolOutput, v, plan_check_summary_text),
        McpTypedOutput::PlanDiff(v) => ok!(PlanDiffToolOutput, v, plan_diff_summary_text),
    }
}

fn no_effect_result(error: &McpAdapterError) -> Result<CallToolResult, PgsError> {
    structured_tool_result(
        ToolOutcomeOnly {
            outcome: ToolOutcome::NoEffect,
            pgs_error: build_pgs_error(error),
        },
        no_effect_text(&error.source),
        false,
    )
}

fn error_result(error: &McpAdapterError) -> Result<CallToolResult, PgsError> {
    let pgs_error = build_pgs_error(error);
    let text = format!("{} Guidance: {}", error.source, pgs_error.guidance);
    structured_tool_result(
        ToolOutcomeOnly {
            outcome: ToolOutcome::Error,
            pgs_error,
        },
        text,
        true,
    )
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

    result.content = vec![ContentBlock::text(text)];

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
        | PgsError::EmptyCommitMessage
        | PgsError::InvalidArguments { .. }
        | PgsError::InputFileUnreadable { .. }
        | PgsError::ExplainWithoutDryRun
        | PgsError::NonUtf8Partial { .. }
        | PgsError::CrlfMismatch { .. }
        | PgsError::UnterminatedInteriorLine { .. } => PgsToolErrorKind::User,
        PgsError::StaleScan { .. } | PgsError::IndexLocked | PgsError::StagingFailed { .. } => {
            PgsToolErrorKind::Retryable
        }
        PgsError::RestoreFailed { .. }
        | PgsError::WorkdirMismatch { .. }
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
        "Plan check: {} overlap(s), {} uncovered, {} unsafe selector(s), {} unknown path(s), {} unknown hunk id(s).",
        output.overlaps.len(),
        output.uncovered.len(),
        output.unsafe_selectors.len(),
        output.unknown_paths.len(),
        output.unknown_hunk_ids.len()
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
        PgsError::BinaryFileGranular { .. }
        | PgsError::GranularOnWholeFile { .. }
        | PgsError::NonUtf8Partial { .. }
        | PgsError::CrlfMismatch { .. } => {
            "Retry with a file-level selection instead of hunk or line granularity.".to_owned()
        }
        PgsError::UnterminatedInteriorLine { .. } => {
            "Stage the whole hunk or the whole file: the selection cannot be applied without \
             also terminating the file's last line."
                .to_owned()
        }
        PgsError::EmptyCommitMessage => {
            "Supply a commit message with at least one non-whitespace character.".to_owned()
        }
        PgsError::ExplainWithoutDryRun => {
            "Pass --dry-run alongside --explain, or drop --explain.".to_owned()
        }
        PgsError::InvalidArguments { detail } => detail.clone(),
        PgsError::InputFileUnreadable { .. } => {
            "Check that the path exists, is a readable UTF-8 file, and is not `-`.".to_owned()
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
        PgsError::RestoreFailed { backup_id, .. } => format!(
            "The index may be inconsistent. Restore manually: \
             cp .git/pgs/backups/{backup_id}.index .git/index"
        ),
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

    fn assert_no_nonstandard_integer_formats(value: &serde_json::Value) {
        match value {
            serde_json::Value::Object(object) => {
                if let Some(format) = object.get("format").and_then(serde_json::Value::as_str) {
                    assert!(
                        !is_nonstandard_integer_format(format),
                        "schema contains nonstandard integer format: {format}"
                    );
                }

                for child in object.values() {
                    assert_no_nonstandard_integer_formats(child);
                }
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    assert_no_nonstandard_integer_formats(item);
                }
            }
            serde_json::Value::Null
            | serde_json::Value::Bool(_)
            | serde_json::Value::Number(_)
            | serde_json::Value::String(_) => {}
        }
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
    fn mcp_tool_schemas_do_not_emit_nonstandard_integer_formats() {
        for tool in tool_definitions() {
            assert_no_nonstandard_integer_formats(&serde_json::Value::Object(
                tool.input_schema.as_ref().clone(),
            ));
            if let Some(output_schema) = tool.output_schema {
                assert_no_nonstandard_integer_formats(&serde_json::Value::Object(
                    output_schema.as_ref().clone(),
                ));
            }
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
    fn mcp_tool_annotations_split_read_only_from_mutating() {
        let read_only = [
            PGS_SCAN_TOOL,
            PGS_STATUS_TOOL,
            PGS_LOG_TOOL,
            PGS_OVERVIEW_TOOL,
            PGS_SPLIT_HUNK_TOOL,
            PGS_PLAN_CHECK_TOOL,
            PGS_PLAN_DIFF_TOOL,
        ];
        let mutating = [PGS_STAGE_TOOL, PGS_UNSTAGE_TOOL, PGS_COMMIT_TOOL];

        for name in read_only {
            let tool = tool_definition(name).expect("tool should exist");
            let annotations = tool.annotations.expect("tool should have annotations");
            assert_eq!(annotations.read_only_hint, Some(true), "{name}");
            assert_eq!(annotations.destructive_hint, Some(false), "{name}");
        }

        for name in mutating {
            let tool = tool_definition(name).expect("tool should exist");
            let annotations = tool.annotations.expect("tool should have annotations");
            assert_eq!(annotations.read_only_hint, Some(false), "{name}");
            assert_eq!(annotations.destructive_hint, Some(true), "{name}");
        }
    }

    #[test]
    fn mcp_overview_tool_is_read_only_and_requires_repo_path() {
        let overview = tool_definition(PGS_OVERVIEW_TOOL).expect("overview tool should exist");

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

    #[test]
    fn mcp_error_result_carries_structured_error_content() {
        let result = map_execution_result(Err(McpAdapterError::new(
            OutputCommand::Scan,
            PgsError::IndexLocked,
        )))
        .expect("error result should serialize");
        assert_eq!(result.is_error, Some(true));
        assert_eq!(
            result.structured_content,
            Some(serde_json::json!({
                "outcome": "error",
                "pgs_error": {
                    "kind": "retryable",
                    "code": "index_locked",
                    "message": "git index is locked by another process",
                    "exit_code": 3,
                    "retryable": true,
                    "guidance": "Wait for the git index lock to clear, then retry the request."
                }
            }))
        );
    }

    #[test]
    fn mcp_no_effect_and_error_outcomes_are_command_agnostic() {
        // no_effect and error outcomes carry no pgs payload; the JSON is identical for
        // every command. Assert the invariant across all 10 OutputCommand variants.
        let expected_no_effect = serde_json::json!({
            "outcome": "no_effect",
            "pgs_error": {
                "kind": "no_effect",
                "code": "no_changes",
                "message": "no changes detected in working tree",
                "exit_code": 1,
                "retryable": false,
                "guidance": "Check the repository state or narrow the request before retrying."
            }
        });
        let expected_error = serde_json::json!({
            "outcome": "error",
            "pgs_error": {
                "kind": "retryable",
                "code": "index_locked",
                "message": "git index is locked by another process",
                "exit_code": 3,
                "retryable": true,
                "guidance": "Wait for the git index lock to clear, then retry the request."
            }
        });

        macro_rules! check_command {
            ($cmd:expr) => {
                let no_effect =
                    map_execution_result(Err(McpAdapterError::new($cmd, PgsError::NoChanges)))
                        .expect("no-effect result should serialize");
                assert_eq!(no_effect.is_error, Some(false));
                assert_eq!(
                    no_effect.structured_content.as_ref(),
                    Some(&expected_no_effect)
                );

                let err =
                    map_execution_result(Err(McpAdapterError::new($cmd, PgsError::IndexLocked)))
                        .expect("error result should serialize");
                assert_eq!(err.is_error, Some(true));
                assert_eq!(err.structured_content.as_ref(), Some(&expected_error));
            };
        }

        check_command!(OutputCommand::Scan);
        check_command!(OutputCommand::Stage);
        check_command!(OutputCommand::Unstage);
        check_command!(OutputCommand::Status);
        check_command!(OutputCommand::Commit);
        check_command!(OutputCommand::Log);
        check_command!(OutputCommand::Overview);
        check_command!(OutputCommand::SplitHunk);
        check_command!(OutputCommand::PlanCheck);
        check_command!(OutputCommand::PlanDiff);
    }

    /// Guards the exact `structuredContent` shape of scan and status success envelopes.
    #[test]
    fn mcp_success_envelopes_match_snapshot() {
        use crate::cmd::mcp_adapter::McpTypedOutput;
        use crate::models::{ScanResult, ScanSummary, StatusReport, StatusSummary};

        // Build via the public factory methods to avoid depending on private view types.
        let scan_output = ScanOutput::compact(&ScanResult {
            files: vec![],
            summary: ScanSummary {
                total_files: 0,
                total_hunks: 0,
                added: 0,
                modified: 0,
                deleted: 0,
                renamed: 0,
                binary: 0,
                mode_changed: 0,
            },
        });
        let scan_result = map_execution_result(Ok(McpTypedOutput::Scan(scan_output)))
            .expect("scan success result must serialize");
        assert_eq!(scan_result.is_error, Some(false));
        assert_eq!(
            scan_result.structured_content,
            Some(serde_json::json!({
                "outcome": "ok",
                "pgs": {
                    "version": "v1",
                    "command": "scan",
                    "detail": "compact",
                    "files": [],
                    "summary": {
                        "total_files": 0,
                        "total_hunks": 0,
                        "added": 0,
                        "modified": 0,
                        "deleted": 0,
                        "renamed": 0,
                        "binary": 0,
                        "mode_changed": 0
                    }
                }
            })),
            "scan success envelope changed — update snapshot if intentional"
        );

        let status_output = StatusOutput::from(StatusReport {
            staged_files: vec![],
            summary: StatusSummary {
                total_files: 0,
                total_additions: 0,
                total_deletions: 0,
            },
        });
        let status_result = map_execution_result(Ok(McpTypedOutput::Status(status_output)))
            .expect("status success result must serialize");
        assert_eq!(status_result.is_error, Some(false));
        assert_eq!(
            status_result.structured_content,
            Some(serde_json::json!({
                "outcome": "ok",
                "pgs": {
                    "version": "v1",
                    "command": "status",
                    "files": [],
                    "summary": {
                        "total_files": 0,
                        "total_additions": 0,
                        "total_deletions": 0
                    }
                }
            })),
            "status success envelope changed — update snapshot if intentional"
        );
    }
}
