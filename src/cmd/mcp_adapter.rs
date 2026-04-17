use rmcp::schemars::{self, JsonSchema};
use serde::{Deserialize, Serialize};

use crate::error::PgsError;
use crate::output::view::{CommandOutput, OutputCommand};

use super::{commit, log, overview, scan, stage, status, unstage};

/// Typed MCP payload for `pgs_scan` requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct McpStatusRequest {
    /// Explicit repository path supplied by the MCP caller.
    pub repo_path: String,
    /// Unified diff context lines to request from the status command.
    pub context: u32,
}

/// Typed MCP payload for `pgs_stage` requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct McpStageRequest {
    /// Explicit repository path supplied by the MCP caller.
    pub repo_path: String,
    /// File, hunk, or line-range selections to stage.
    pub selections: Vec<String>,
    /// Selections to exclude from the staging request.
    pub exclude: Vec<String>,
    /// Whether to validate the request without mutating the index.
    pub dry_run: bool,
    /// Unified diff context lines used while resolving selections.
    pub context: u32,
}

/// Typed MCP payload for `pgs_unstage` requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct McpLogRequest {
    /// Explicit repository path supplied by the MCP caller.
    pub repo_path: String,
    /// Maximum number of commits to return.
    pub max_count: u32,
    /// Optional file path filters forwarded to the log command.
    pub paths: Vec<String>,
}

/// Typed MCP payload for `pgs_commit` requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct McpCommitRequest {
    /// Explicit repository path supplied by the MCP caller.
    pub repo_path: String,
    /// Commit message to pass through to the commit command.
    pub message: String,
}

/// Typed MCP payload for `pgs_overview` requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct McpOverviewRequest {
    /// Explicit repository path supplied by the MCP caller.
    pub repo_path: String,
    /// Unified diff context lines forwarded to the scan and status stages.
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
}

/// Typed command output returned to MCP callers without re-parsing CLI markers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpTypedOutput {
    /// Structured scan output.
    Scan(crate::output::view::ScanOutput),
    /// Structured stage or unstage output.
    Operation(crate::output::view::OperationOutput),
    /// Structured status output.
    Status(crate::output::view::StatusOutput),
    /// Structured commit output.
    Commit(crate::output::view::CommitOutput),
    /// Structured log output.
    Log(crate::output::view::LogOutput),
    /// Structured overview output (scan + status fusion).
    Overview(crate::output::view::OverviewOutput),
    /// Structured split-hunk output (descriptive run classification).
    SplitHunk(crate::output::view::SplitHunkOutput),
}

impl From<CommandOutput> for McpTypedOutput {
    fn from(output: CommandOutput) -> Self {
        match output {
            CommandOutput::Scan(output) => Self::Scan(output),
            CommandOutput::Operation(output) => Self::Operation(output),
            CommandOutput::Status(output) => Self::Status(output),
            CommandOutput::Commit(output) => Self::Commit(output),
            CommandOutput::Log(output) => Self::Log(output),
            CommandOutput::Overview(output) => Self::Overview(output),
            CommandOutput::SplitHunk(output) => Self::SplitHunk(output),
        }
    }
}

/// Stable MCP-facing error metadata derived from a CLI command failure.
#[derive(Debug)]
pub struct McpAdapterError {
    /// Command that produced the failure.
    pub command: OutputCommand,
    /// Stable machine-readable error code from [`PgsError`].
    pub code: &'static str,
    /// Exit code associated with the underlying error.
    pub exit_code: i32,
    /// Original command error preserved for downstream mapping.
    pub source: PgsError,
}

impl McpAdapterError {
    pub(crate) const fn new(command: OutputCommand, source: PgsError) -> Self {
        Self {
            command,
            code: source.code(),
            exit_code: source.exit_code(),
            source,
        }
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
pub fn execute(request: McpCommandRequest) -> Result<McpTypedOutput, McpAdapterError> {
    match request {
        McpCommandRequest::Scan(request) => scan::execute(
            Some(request.repo_path.as_str()),
            request.context.max(1),
            scan::ScanArgs {
                files: request.files,
                full: request.full,
            },
        )
        .map(Into::into)
        .map_err(|source| McpAdapterError::new(OutputCommand::Scan, source)),
        McpCommandRequest::Status(request) => {
            status::execute(Some(request.repo_path.as_str()), request.context.max(1))
                .map(Into::into)
                .map_err(|source| McpAdapterError::new(OutputCommand::Status, source))
        }
        McpCommandRequest::Stage(request) => stage::execute(
            Some(request.repo_path.as_str()),
            request.context.max(1),
            stage::StageArgs {
                selections: request.selections,
                exclude: request.exclude,
                dry_run: request.dry_run,
                explain: false,
                limit: 200,
            },
        )
        .map(Into::into)
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
        .map(Into::into)
        .map_err(|source| McpAdapterError::new(OutputCommand::Unstage, source)),
        McpCommandRequest::Commit(request) => commit::execute(
            Some(request.repo_path.as_str()),
            commit::CommitArgs {
                message: request.message,
            },
        )
        .map(Into::into)
        .map_err(|source| McpAdapterError::new(OutputCommand::Commit, source)),
        McpCommandRequest::Log(request) => log::execute(
            Some(request.repo_path.as_str()),
            log::LogArgs {
                max_count: request.max_count,
                paths: request.paths,
            },
        )
        .map(Into::into)
        .map_err(|source| McpAdapterError::new(OutputCommand::Log, source)),
        McpCommandRequest::Overview(request) => {
            overview::execute(Some(request.repo_path.as_str()), request.context.max(1))
                .map(Into::into)
                .map_err(|source| McpAdapterError::new(OutputCommand::Overview, source))
        }
    }
}
