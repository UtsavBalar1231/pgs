use rmcp::schemars::{self, JsonSchema};
use serde::Serialize;

pub const OUTPUT_VERSION: &str = "v1";

/// Top-level enum carrying any command's typed output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandOutput {
    Scan(super::scan::ScanOutput),
    Operation(super::operation::OperationOutput),
    Status(super::operation::StatusOutput),
    Commit(super::operation::CommitOutput),
    Log(super::operation::LogOutput),
    Overview(super::operation::OverviewOutput),
    SplitHunk(super::plan::SplitHunkOutput),
    PlanCheck(super::plan::PlanCheckOutput),
    PlanDiff(super::plan::PlanDiffOutput),
}

impl From<super::scan::ScanOutput> for CommandOutput {
    fn from(output: super::scan::ScanOutput) -> Self {
        Self::Scan(output)
    }
}

impl From<super::operation::OperationOutput> for CommandOutput {
    fn from(output: super::operation::OperationOutput) -> Self {
        Self::Operation(output)
    }
}

impl From<super::operation::StatusOutput> for CommandOutput {
    fn from(output: super::operation::StatusOutput) -> Self {
        Self::Status(output)
    }
}

impl From<super::operation::CommitOutput> for CommandOutput {
    fn from(output: super::operation::CommitOutput) -> Self {
        Self::Commit(output)
    }
}

impl From<super::operation::LogOutput> for CommandOutput {
    fn from(output: super::operation::LogOutput) -> Self {
        Self::Log(output)
    }
}

impl From<super::operation::OverviewOutput> for CommandOutput {
    fn from(output: super::operation::OverviewOutput) -> Self {
        Self::Overview(output)
    }
}

impl From<super::plan::SplitHunkOutput> for CommandOutput {
    fn from(output: super::plan::SplitHunkOutput) -> Self {
        Self::SplitHunk(output)
    }
}

impl From<super::plan::PlanCheckOutput> for CommandOutput {
    fn from(output: super::plan::PlanCheckOutput) -> Self {
        Self::PlanCheck(output)
    }
}

impl From<super::plan::PlanDiffOutput> for CommandOutput {
    fn from(output: super::plan::PlanDiffOutput) -> Self {
        Self::PlanDiff(output)
    }
}

#[derive(Debug, Clone, Copy, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutputCommand {
    Scan,
    Stage,
    Unstage,
    Status,
    Commit,
    Log,
    Overview,
    #[serde(rename = "split")]
    SplitHunk,
    #[serde(rename = "plan-check")]
    PlanCheck,
    #[serde(rename = "plan-diff")]
    PlanDiff,
}

impl OutputCommand {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scan => "scan",
            Self::Stage => "stage",
            Self::Unstage => "unstage",
            Self::Status => "status",
            Self::Commit => "commit",
            Self::Log => "log",
            Self::Overview => "overview",
            Self::SplitHunk => "split",
            Self::PlanCheck => "plan-check",
            Self::PlanDiff => "plan-diff",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorPhase {
    Parse,
    Runtime,
}

#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
pub struct CliErrorOutput {
    pub version: &'static str,
    pub command: &'static str,
    pub phase: ErrorPhase,
    pub code: &'static str,
    pub message: String,
    pub exit_code: i32,
}

impl CliErrorOutput {
    pub const fn parse(code: &'static str, message: String, exit_code: i32) -> Self {
        Self {
            version: OUTPUT_VERSION,
            command: "cli",
            phase: ErrorPhase::Parse,
            code,
            message,
            exit_code,
        }
    }

    pub const fn runtime(
        command: OutputCommand,
        code: &'static str,
        message: String,
        exit_code: i32,
    ) -> Self {
        Self {
            version: OUTPUT_VERSION,
            command: command.as_str(),
            phase: ErrorPhase::Runtime,
            code,
            message,
            exit_code,
        }
    }
}

/// View-side classification of a diff line origin.
#[derive(Debug, Clone, Copy, Serialize, JsonSchema, PartialEq, Eq)]
pub enum LineOriginView {
    Context,
    Addition,
    Deletion,
}

impl From<crate::models::LineOrigin> for LineOriginView {
    fn from(origin: crate::models::LineOrigin) -> Self {
        match origin {
            crate::models::LineOrigin::Context => Self::Context,
            crate::models::LineOrigin::Addition => Self::Addition,
            crate::models::LineOrigin::Deletion => Self::Deletion,
        }
    }
}
