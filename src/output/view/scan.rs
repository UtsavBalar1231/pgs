use rmcp::schemars::{self, JsonSchema};
use serde::Serialize;

use crate::models::{
    CompactFileInfo, CompactHunkInfo, CompactScanResult, DiffLineInfo, FileInfo, FileStatus,
    HunkInfo, LineOrigin, ScanResult, ScanSummary,
};

use super::common::{LineOriginView, OUTPUT_VERSION, OutputCommand};

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
    pub(super) fn from_compact(file: CompactFileInfo) -> Self {
        let CompactFileInfo {
            path,
            status,
            file_checksum,
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
            checksum: Some(file_checksum),
            old_mode,
            new_mode,
            hunks: hunks.into_iter().map(ScanHunkView::from_compact).collect(),
        }
    }

    pub(super) fn from_full(file: FileInfo) -> Self {
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

/// View-side file status — mirrors [`FileStatus`] but serialized for output.
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
