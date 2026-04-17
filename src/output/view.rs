use rmcp::schemars::{self, JsonSchema};
use serde::Serialize;

use crate::models::{
    CommitResult, CompactFileInfo, CompactHunkInfo, CompactScanResult, DiffLineInfo, FileInfo,
    FileStatus, HunkInfo, LineOrigin, OperationStatus, ScanResult, ScanSummary, StagedFileInfo,
    StatusReport, StatusSummary,
};

pub const OUTPUT_VERSION: &str = "v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandOutput {
    Scan(ScanOutput),
    Operation(OperationOutput),
    Status(StatusOutput),
    Commit(CommitOutput),
    Log(LogOutput),
    Overview(OverviewOutput),
}

impl From<ScanOutput> for CommandOutput {
    fn from(output: ScanOutput) -> Self {
        Self::Scan(output)
    }
}

impl From<OperationOutput> for CommandOutput {
    fn from(output: OperationOutput) -> Self {
        Self::Operation(output)
    }
}

impl From<StatusOutput> for CommandOutput {
    fn from(output: StatusOutput) -> Self {
        Self::Status(output)
    }
}

impl From<CommitOutput> for CommandOutput {
    fn from(output: CommitOutput) -> Self {
        Self::Commit(output)
    }
}

impl From<LogOutput> for CommandOutput {
    fn from(output: LogOutput) -> Self {
        Self::Log(output)
    }
}

impl From<OverviewOutput> for CommandOutput {
    fn from(output: OverviewOutput) -> Self {
        Self::Overview(output)
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
        }
    }

    pub fn stage(
        status: OperationStatus,
        items: Vec<OperationItemView>,
        warnings: Vec<String>,
        backup_id: Option<String>,
    ) -> Self {
        Self::new(OutputCommand::Stage, status, items, warnings, backup_id)
    }

    pub fn unstage(
        status: OperationStatus,
        items: Vec<OperationItemView>,
        warnings: Vec<String>,
        backup_id: Option<String>,
    ) -> Self {
        Self::new(OutputCommand::Unstage, status, items, warnings, backup_id)
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

#[derive(Debug, Clone, Copy, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScanDetail {
    Compact,
    Full,
}

#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
pub struct ScanOutput {
    pub version: &'static str,
    pub command: OutputCommand,
    pub detail: ScanDetail,
    pub files: Vec<ScanFileView>,
    pub summary: ScanSummaryView,
}

impl ScanOutput {
    pub fn compact(result: &ScanResult) -> Self {
        let CompactScanResult { files, summary } = CompactScanResult::from(result);

        Self {
            version: OUTPUT_VERSION,
            command: OutputCommand::Scan,
            detail: ScanDetail::Compact,
            files: files.into_iter().map(ScanFileView::from_compact).collect(),
            summary: summary.into(),
        }
    }

    pub fn full(result: ScanResult) -> Self {
        let ScanResult { files, summary } = result;

        Self {
            version: OUTPUT_VERSION,
            command: OutputCommand::Scan,
            detail: ScanDetail::Full,
            files: files.into_iter().map(ScanFileView::from_full).collect(),
            summary: summary.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
pub struct ScanFileView {
    pub path: String,
    pub status: FileStatusView,
    pub binary: bool,
    pub hunks_count: usize,
    pub lines_added: u32,
    pub lines_deleted: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_mode: Option<String>,
    pub hunks: Vec<ScanHunkView>,
}

impl ScanFileView {
    fn from_compact(file: CompactFileInfo) -> Self {
        let CompactFileInfo {
            path,
            status,
            is_binary,
            old_mode: old_mode_raw,
            new_mode: new_mode_raw,
            hunks,
            hunks_count,
            lines_added,
            lines_deleted,
        } = file;

        let (old_mode, new_mode) = if old_mode_raw == new_mode_raw {
            (None, None)
        } else {
            (
                Some(format!("{old_mode_raw:o}")),
                Some(format!("{new_mode_raw:o}")),
            )
        };

        Self {
            path,
            status: status.into(),
            binary: is_binary,
            hunks_count,
            lines_added,
            lines_deleted,
            checksum: None,
            old_mode,
            new_mode,
            hunks: hunks.into_iter().map(ScanHunkView::from_compact).collect(),
        }
    }

    fn from_full(file: FileInfo) -> Self {
        let FileInfo {
            path,
            status,
            file_checksum,
            is_binary,
            old_mode: old_mode_raw,
            new_mode: new_mode_raw,
            hunks,
        } = file;

        let hunks: Vec<ScanHunkView> = hunks.into_iter().map(ScanHunkView::from_full).collect();
        let (lines_added, lines_deleted) = count_hunk_totals(&hunks);
        let hunks_count = hunks.len();

        let (old_mode, new_mode) = if old_mode_raw == new_mode_raw {
            (None, None)
        } else {
            (
                Some(format!("{old_mode_raw:o}")),
                Some(format!("{new_mode_raw:o}")),
            )
        };

        Self {
            path,
            status: status.into(),
            binary: is_binary,
            hunks_count,
            lines_added,
            lines_deleted,
            checksum: Some(file_checksum),
            old_mode,
            new_mode,
            hunks,
        }
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
pub struct ScanHunkView {
    pub id: String,
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub header: String,
    pub additions: u32,
    pub deletions: u32,
    pub whitespace_only: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines: Option<Vec<ScanLineView>>,
}

impl ScanHunkView {
    fn from_compact(hunk: CompactHunkInfo) -> Self {
        let CompactHunkInfo {
            hunk_id,
            header,
            old_start,
            old_lines,
            new_start,
            new_lines,
            additions,
            deletions,
            whitespace_only,
        } = hunk;

        Self {
            id: hunk_id,
            old_start,
            old_lines,
            new_start,
            new_lines,
            header,
            additions,
            deletions,
            whitespace_only,
            checksum: None,
            lines: None,
        }
    }

    fn from_full(hunk: HunkInfo) -> Self {
        let HunkInfo {
            hunk_id,
            old_start,
            old_lines,
            new_start,
            new_lines,
            header,
            lines,
            checksum,
            whitespace_only,
        } = hunk;

        let (additions, deletions) = count_lines(&lines);

        Self {
            id: hunk_id,
            old_start,
            old_lines,
            new_start,
            new_lines,
            header,
            additions,
            deletions,
            whitespace_only,
            checksum: Some(checksum),
            lines: Some(lines.into_iter().map(Into::into).collect()),
        }
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
pub struct ScanLineView {
    pub line_number: u32,
    pub origin: LineOriginView,
    pub content: String,
}

impl From<DiffLineInfo> for ScanLineView {
    fn from(line: DiffLineInfo) -> Self {
        Self {
            line_number: line.line_number,
            origin: line.origin.into(),
            content: line.content,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, JsonSchema, PartialEq, Eq)]
pub enum LineOriginView {
    Context,
    Addition,
    Deletion,
}

impl From<LineOrigin> for LineOriginView {
    fn from(origin: LineOrigin) -> Self {
        match origin {
            LineOrigin::Context => Self::Context,
            LineOrigin::Addition => Self::Addition,
            LineOrigin::Deletion => Self::Deletion,
        }
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
pub struct ScanSummaryView {
    pub total_files: usize,
    pub total_hunks: usize,
    pub added: usize,
    pub modified: usize,
    pub deleted: usize,
    pub renamed: usize,
    pub binary: usize,
    pub mode_changed: usize,
}

impl From<ScanSummary> for ScanSummaryView {
    fn from(summary: ScanSummary) -> Self {
        Self {
            total_files: summary.total_files,
            total_hunks: summary.total_hunks,
            added: summary.added,
            modified: summary.modified,
            deleted: summary.deleted,
            renamed: summary.renamed,
            binary: summary.binary,
            mode_changed: summary.mode_changed,
        }
    }
}

fn count_hunk_totals(hunks: &[ScanHunkView]) -> (u32, u32) {
    let lines_added = hunks.iter().map(|hunk| hunk.additions).sum();
    let lines_deleted = hunks.iter().map(|hunk| hunk.deletions).sum();
    (lines_added, lines_deleted)
}

fn count_lines(lines: &[DiffLineInfo]) -> (u32, u32) {
    let additions = crate::saturating_u32(
        lines
            .iter()
            .filter(|line| line.origin == LineOrigin::Addition)
            .count(),
    );
    let deletions = crate::saturating_u32(
        lines
            .iter()
            .filter(|line| line.origin == LineOrigin::Deletion)
            .count(),
    );

    (additions, deletions)
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
#[serde(tag = "type")]
pub enum FileStatusView {
    Added,
    Modified,
    Deleted,
    Renamed { old_path: String },
}

impl From<FileStatus> for FileStatusView {
    fn from(status: FileStatus) -> Self {
        match status {
            FileStatus::Added => Self::Added,
            FileStatus::Modified => Self::Modified,
            FileStatus::Deleted => Self::Deleted,
            FileStatus::Renamed { old_path } => Self::Renamed { old_path },
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
    pub unstaged: ScanOutput,
    pub staged: StatusOutput,
}

impl OverviewOutput {
    pub const fn new(unstaged: ScanOutput, staged: StatusOutput) -> Self {
        Self {
            version: OUTPUT_VERSION,
            command: OutputCommand::Overview,
            unstaged,
            staged,
        }
    }
}
