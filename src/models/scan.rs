use rmcp::schemars::{self, JsonSchema};
use serde::{Deserialize, Serialize};

use crate::saturating_u32;

// ─── Scan Output ───────────────────────────────────────────────────

/// Result of `pgs scan --full` — all unstaged changes with line content.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScanResult {
    /// List of files with unstaged changes.
    pub files: Vec<FileInfo>,
    /// Summary statistics for the scan.
    pub summary: ScanSummary,
}

/// Per-file information in a scan result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileInfo {
    /// Relative path from the repository root.
    pub path: String,
    /// File-level change status.
    pub status: FileStatus,
    /// SHA-256 hex digest of working-tree file content.
    pub file_checksum: String,
    /// Whether this file contains binary content.
    pub is_binary: bool,
    /// File mode in the old (index) state (e.g. `0o100644`).
    pub old_mode: u32,
    /// File mode in the new (workdir) state (e.g. `0o100755`).
    pub new_mode: u32,
    /// Diff hunks for this file. Empty for binary files.
    pub hunks: Vec<HunkInfo>,
}

/// Summary statistics for a scan.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScanSummary {
    /// Total number of files with changes.
    pub total_files: usize,
    /// Total number of hunks across all files.
    pub total_hunks: usize,
    /// Count of Added files.
    pub added: usize,
    /// Count of Modified files.
    pub modified: usize,
    /// Count of Deleted files.
    pub deleted: usize,
    /// Count of Renamed files.
    pub renamed: usize,
    /// Count of binary files.
    pub binary: usize,
    /// Count of files with mode (permission) changes.
    pub mode_changed: usize,
}

/// File-level change status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum FileStatus {
    /// File is new and not yet in the index.
    Added,
    /// File exists in the index but has unstaged modifications.
    Modified,
    /// File has been deleted from the working tree.
    Deleted,
    /// File has been renamed.
    Renamed {
        /// The original path before renaming.
        old_path: String,
    },
}

/// A single diff hunk with content-based ID.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HunkInfo {
    /// Position-DEPENDENT addressing key: `sha256(path:old_start:new_start:content)[..12]`.
    /// Shifts when an earlier hunk changes the line count. Use for fresh-scan
    /// selection; use `checksum` to re-locate after cross-hunk edits.
    pub hunk_id: String,
    /// Starting line in the original file (1-indexed).
    pub old_start: u32,
    /// Number of lines in the original file hunk.
    pub old_lines: u32,
    /// Starting line in the new file (1-indexed).
    pub new_start: u32,
    /// Number of lines in the new file hunk.
    pub new_lines: u32,
    /// Raw @@ header line.
    pub header: String,
    /// Individual lines within the hunk.
    pub lines: Vec<DiffLineInfo>,
    /// Position-STABLE content fingerprint: SHA-256 of all hunk lines (context,
    /// additions, and deletions) concatenated, no positional data included.
    /// Invariant under position shifts — use to re-locate a hunk after cross-hunk
    /// edits. `plan-diff` keys `High`-confidence relocation off this field.
    /// Present in `scan --full` output only; compact scan omits it.
    pub checksum: String,
    /// True when every Addition/Deletion line has empty or whitespace-only content. Metadata only — not part of the hunk-ID input.
    #[serde(default)]
    pub whitespace_only: bool,
}

/// A single line within a diff hunk.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiffLineInfo {
    /// Line number (1-indexed): new file for additions/context, old file for deletions.
    pub line_number: u32,
    /// Classification of the line.
    pub origin: LineOrigin,
    /// Text content (without +/- prefix).
    pub content: String,
}

/// Classification of a single diff line; values produced by git2 are `Context`, `Addition`, and `Deletion`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum LineOrigin {
    /// Unchanged context line.
    Context,
    /// Line added in the new version.
    Addition,
    /// Line removed from the old version.
    Deletion,
}

/// Classification of a contiguous run of diff lines in [`suggest_splits`](crate::git::diff::suggest_splits) output.
///
/// Unlike [`LineOrigin`], this covers runs, so `Mixed` is valid. `DiffLineInfo` never carries `Mixed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OriginMix {
    /// The run contains only additions.
    Addition,
    /// The run contains only deletions.
    Deletion,
    /// The run interleaves additions and deletions.
    Mixed,
}

// ─── Compact Scan Output ──────────────────────────────────────────

/// Compact scan result — default output for `pgs scan`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompactScanResult {
    /// Files with unstaged changes (metadata only).
    pub files: Vec<CompactFileInfo>,
    /// Summary statistics.
    pub summary: ScanSummary,
}

/// Compact per-file info with aggregate line counts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompactFileInfo {
    /// Relative path from repo root.
    pub path: String,
    /// File-level change status.
    pub status: FileStatus,
    /// SHA-256 hex digest of working-tree file content (same value as `FileInfo::file_checksum`).
    pub file_checksum: String,
    /// Whether this file contains binary content.
    pub is_binary: bool,
    /// File mode in the old (index) state (e.g. `0o100644`).
    pub old_mode: u32,
    /// File mode in the new (workdir) state (e.g. `0o100755`).
    pub new_mode: u32,
    /// Hunk metadata (no line content).
    pub hunks: Vec<CompactHunkInfo>,
    /// Number of hunks in this file.
    pub hunks_count: usize,
    /// Total lines added across all hunks.
    pub lines_added: u32,
    /// Total lines deleted across all hunks.
    pub lines_deleted: u32,
}

/// Hunk metadata only — no diff line content.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompactHunkInfo {
    /// Content-based ID (same as `HunkInfo.hunk_id`).
    pub hunk_id: String,
    /// Raw @@ header line.
    pub header: String,
    /// Starting line in original file (1-indexed).
    pub old_start: u32,
    /// Lines in original file hunk.
    pub old_lines: u32,
    /// Starting line in new file (1-indexed).
    pub new_start: u32,
    /// Lines in new file hunk.
    pub new_lines: u32,
    /// Count of Addition lines.
    pub additions: u32,
    /// Count of Deletion lines.
    pub deletions: u32,
    /// True when every Addition/Deletion line has empty or whitespace-only content.
    #[serde(default)]
    pub whitespace_only: bool,
}

impl From<&ScanResult> for CompactScanResult {
    fn from(result: &ScanResult) -> Self {
        let files = result
            .files
            .iter()
            .map(|file| {
                let hunks: Vec<CompactHunkInfo> = file
                    .hunks
                    .iter()
                    .map(|hunk| {
                        let additions = saturating_u32(
                            hunk.lines
                                .iter()
                                .filter(|l| l.origin == LineOrigin::Addition)
                                .count(),
                        );
                        let deletions = saturating_u32(
                            hunk.lines
                                .iter()
                                .filter(|l| l.origin == LineOrigin::Deletion)
                                .count(),
                        );
                        CompactHunkInfo {
                            hunk_id: hunk.hunk_id.clone(),
                            header: hunk.header.clone(),
                            old_start: hunk.old_start,
                            old_lines: hunk.old_lines,
                            new_start: hunk.new_start,
                            new_lines: hunk.new_lines,
                            additions,
                            deletions,
                            whitespace_only: hunk.whitespace_only,
                        }
                    })
                    .collect();
                let lines_added = hunks.iter().map(|h| h.additions).sum();
                let lines_deleted = hunks.iter().map(|h| h.deletions).sum();
                let hunks_count = hunks.len();
                CompactFileInfo {
                    path: file.path.clone(),
                    status: file.status.clone(),
                    file_checksum: file.file_checksum.clone(),
                    is_binary: file.is_binary,
                    old_mode: file.old_mode,
                    new_mode: file.new_mode,
                    hunks,
                    hunks_count,
                    lines_added,
                    lines_deleted,
                }
            })
            .collect();
        Self {
            files,
            summary: result.summary.clone(),
        }
    }
}
