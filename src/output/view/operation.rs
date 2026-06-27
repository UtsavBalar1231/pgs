use rmcp::schemars::{self, JsonSchema};
use serde::Serialize;

use crate::models::{
    CommitResult, OperationPreview, OperationStatus, StagedFileInfo, StatusReport, StatusSummary,
};

use super::common::{OUTPUT_VERSION, OutputCommand};
use super::scan::FileStatusView;

#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
pub struct CommitOutput {
    pub version: &'static str,
    pub command: OutputCommand,
    pub commit_hash: String,
    pub message: String,
    pub author: String,
    pub files_changed: usize,
    pub insertions: u32,
    pub deletions: u32,
}

impl From<CommitResult> for CommitOutput {
    fn from(result: CommitResult) -> Self {
        Self {
            version: OUTPUT_VERSION,
            command: OutputCommand::Commit,
            commit_hash: result.commit_hash,
            message: result.message,
            author: result.author,
            files_changed: result.files_changed,
            insertions: result.insertions,
            deletions: result.deletions,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OperationStatusView {
    Ok,
    DryRun,
}

impl From<OperationStatus> for OperationStatusView {
    fn from(status: OperationStatus) -> Self {
        match status {
            OperationStatus::Ok => Self::Ok,
            OperationStatus::DryRun => Self::DryRun,
        }
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
pub struct OperationOutput {
    pub version: &'static str,
    pub command: OutputCommand,
    pub status: OperationStatusView,
    pub items: Vec<OperationItemView>,
    pub warnings: Vec<String>,
    pub backup_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previews: Option<Vec<OperationPreview>>,
}

impl OperationOutput {
    pub fn new(
        command: OutputCommand,
        status: OperationStatus,
        items: Vec<OperationItemView>,
        warnings: Vec<String>,
        backup_id: Option<String>,
    ) -> Self {
        Self {
            version: OUTPUT_VERSION,
            command,
            status: status.into(),
            items,
            warnings,
            backup_id,
            previews: None,
        }
    }

    /// Attach per-file previews for `--dry-run --explain`.
    #[must_use]
    pub fn with_previews(mut self, previews: Vec<OperationPreview>) -> Self {
        self.previews = Some(previews);
        self
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
pub struct OperationItemView {
    pub selection: String,
    pub lines_affected: u32,
}

impl OperationItemView {
    pub const fn new(selection: String, lines_affected: u32) -> Self {
        Self {
            selection,
            lines_affected,
        }
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
pub struct StatusOutput {
    pub version: &'static str,
    pub command: OutputCommand,
    pub files: Vec<StatusFileView>,
    pub summary: StatusSummaryView,
}

impl From<StatusReport> for StatusOutput {
    fn from(report: StatusReport) -> Self {
        let StatusReport {
            staged_files,
            summary,
        } = report;

        Self {
            version: OUTPUT_VERSION,
            command: OutputCommand::Status,
            files: staged_files.into_iter().map(Into::into).collect(),
            summary: summary.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
pub struct StatusFileView {
    pub path: String,
    pub status: FileStatusView,
    pub lines_added: u32,
    pub lines_deleted: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_mode: Option<String>,
}

impl From<StagedFileInfo> for StatusFileView {
    fn from(file: StagedFileInfo) -> Self {
        let (old_mode, new_mode) = if file.old_mode == file.new_mode {
            (None, None)
        } else {
            (
                Some(format!("{:o}", file.old_mode)),
                Some(format!("{:o}", file.new_mode)),
            )
        };

        Self {
            path: file.path,
            status: file.status.into(),
            lines_added: file.lines_added,
            lines_deleted: file.lines_deleted,
            old_mode,
            new_mode,
        }
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
#[allow(clippy::struct_field_names)]
pub struct StatusSummaryView {
    pub total_files: usize,
    pub total_additions: u32,
    pub total_deletions: u32,
}

impl From<StatusSummary> for StatusSummaryView {
    fn from(summary: StatusSummary) -> Self {
        Self {
            total_files: summary.total_files,
            total_additions: summary.total_additions,
            total_deletions: summary.total_deletions,
        }
    }
}

/// A single commit entry for log output.
#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
pub struct CommitEntryView {
    pub hash: String,
    pub short_hash: String,
    pub author: String,
    pub date: String,
    pub message: String,
}

/// Output for the `log` command.
#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
pub struct LogOutput {
    pub version: &'static str,
    pub command: OutputCommand,
    pub commits: Vec<CommitEntryView>,
    pub total: usize,
    pub truncated: bool,
}

/// Output for the `overview` command — fuses scan (unstaged) and status (staged) envelopes.
#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
pub struct OverviewOutput {
    pub version: &'static str,
    pub command: OutputCommand,
    pub unstaged: super::scan::ScanOutput,
    pub staged: StatusOutput,
}

impl OverviewOutput {
    pub const fn new(unstaged: super::scan::ScanOutput, staged: StatusOutput) -> Self {
        Self {
            version: OUTPUT_VERSION,
            command: OutputCommand::Overview,
            unstaged,
            staged,
        }
    }
}
