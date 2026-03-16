/// Shared data models for pgs.
///
/// All serializable types used across commands. Every struct derives
/// Serialize + Deserialize for JSON round-tripping.
use serde::{Deserialize, Serialize};

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
    /// Content-based ID: `sha256(path:old_start:new_start:content)[0..12]`.
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
    /// SHA-256 hex digest of hunk line content.
    pub checksum: String,
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

/// Classification of a diff line.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum LineOrigin {
    /// Unchanged context line.
    Context,
    /// Line added in the new version.
    Addition,
    /// Line removed from the old version.
    Deletion,
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
}

#[allow(clippy::cast_possible_truncation)]
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
                        let additions = hunk
                            .lines
                            .iter()
                            .filter(|l| l.origin == LineOrigin::Addition)
                            .count() as u32;
                        let deletions = hunk
                            .lines
                            .iter()
                            .filter(|l| l.origin == LineOrigin::Deletion)
                            .count() as u32;
                        CompactHunkInfo {
                            hunk_id: hunk.hunk_id.clone(),
                            header: hunk.header.clone(),
                            old_start: hunk.old_start,
                            old_lines: hunk.old_lines,
                            new_start: hunk.new_start,
                            new_lines: hunk.new_lines,
                            additions,
                            deletions,
                        }
                    })
                    .collect();
                let lines_added = hunks.iter().map(|h| h.additions).sum();
                let lines_deleted = hunks.iter().map(|h| h.deletions).sum();
                let hunks_count = hunks.len();
                CompactFileInfo {
                    path: file.path.clone(),
                    status: file.status.clone(),
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

// ─── Stage/Unstage Output ─────────────────────────────────────────

/// Result of `pgs stage` or `pgs unstage`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StageResult {
    /// Overall outcome.
    pub status: OperationStatus,
    /// Selections successfully applied.
    pub succeeded: Vec<StagedItem>,
    /// Selections that failed (always empty on success — failures roll back).
    pub failed: Vec<FailedItem>,
    /// Non-fatal warnings.
    pub warnings: Vec<String>,
    /// Backup ID (always present — backup is mandatory).
    pub backup_id: String,
}

/// Overall operation outcome.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum OperationStatus {
    /// All selections were successfully applied.
    Ok,
    /// Operation was a dry-run; no changes made.
    DryRun,
}

/// A successfully staged selection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StagedItem {
    /// The selection string that was applied.
    pub selection: String,
    /// Number of lines staged/unstaged.
    pub lines_staged: u32,
}

/// A selection that failed to stage.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FailedItem {
    /// The selection string that failed.
    pub selection: String,
    /// Machine-readable failure reason.
    pub reason: String,
    /// Human/agent-readable recovery suggestion.
    pub suggestion: String,
}

// ─── Status Output ─────────────────────────────────────────────────

/// Result of `pgs status`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StatusReport {
    /// Files currently staged.
    pub staged_files: Vec<StagedFileInfo>,
    /// Summary statistics.
    pub summary: StatusSummary,
}

/// Per-file info for staged changes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StagedFileInfo {
    /// Relative path from repo root.
    pub path: String,
    /// File-level change status in the index.
    pub status: FileStatus,
    /// Lines added in this file.
    pub lines_added: u32,
    /// Lines deleted in this file.
    pub lines_deleted: u32,
    /// File mode in the old (HEAD) state.
    pub old_mode: u32,
    /// File mode in the new (index) state.
    pub new_mode: u32,
}

/// Summary of staged changes.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct StatusSummary {
    /// Total files with staged changes.
    pub total_files: usize,
    /// Total lines added.
    pub total_additions: u32,
    /// Total lines deleted.
    pub total_deletions: u32,
}

// ─── Commit Output ────────────────────────────────────────────────

/// Result of `pgs commit`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommitResult {
    /// Full 40-character SHA-1 hash.
    pub commit_hash: String,
    /// The commit message.
    pub message: String,
    /// Author in "Name <email>" format.
    pub author: String,
    /// Number of files changed.
    pub files_changed: usize,
    /// Total line insertions.
    pub insertions: u32,
    /// Total line deletions.
    pub deletions: u32,
}

// ─── Selection (internal, not serialized to JSON output) ──────────

/// Parsed selection from CLI positional args.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionSpec {
    /// Select an entire file.
    File { path: String },
    /// Select a specific hunk by its content-based ID.
    Hunk { hunk_id: String },
    /// Select specific line ranges within a file.
    Lines {
        path: String,
        ranges: Vec<LineRange>,
    },
}

/// An inclusive line range [start, end] (1-indexed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineRange {
    /// Starting line number (inclusive).
    pub start: u32,
    /// Ending line number (inclusive).
    pub end: u32,
}

/// A resolved selection ready for staging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSelection {
    /// Path to the file.
    pub file_path: String,
    /// Indices into the file's hunks vec.
    pub hunk_indices: Vec<usize>,
    /// Optional line ranges (for line-level staging).
    pub line_ranges: Option<Vec<LineRange>>,
}

// ─── Backup (internal) ───────────────────────────────────────────

/// Metadata for an index backup.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupInfo {
    /// Unique identifier for the backup.
    pub backup_id: String,
    /// ISO 8601 timestamp.
    pub timestamp: String,
    /// SHA-256 checksum of the backed-up index.
    pub index_checksum: String,
}

/// Format a `SelectionSpec` as a display string.
pub fn format_selection(spec: &SelectionSpec) -> String {
    match spec {
        SelectionSpec::File { path } => path.clone(),
        SelectionSpec::Hunk { hunk_id } => hunk_id.clone(),
        SelectionSpec::Lines { path, ranges } => {
            let ranges_str: Vec<String> = ranges
                .iter()
                .map(|r| format!("{}-{}", r.start, r.end))
                .collect();
            format!("{path}:{}", ranges_str.join(","))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_result_json_round_trip() {
        let result = ScanResult {
            files: vec![FileInfo {
                path: "src/main.rs".into(),
                status: FileStatus::Modified,
                file_checksum: "abc123".into(),
                is_binary: false,
                old_mode: 0o100_644,
                new_mode: 0o100_644,
                hunks: vec![HunkInfo {
                    hunk_id: "h1".into(),
                    old_start: 1,
                    old_lines: 3,
                    new_start: 1,
                    new_lines: 5,
                    header: "@@ -1,3 +1,5 @@".into(),
                    lines: vec![DiffLineInfo {
                        line_number: 1,
                        origin: LineOrigin::Context,
                        content: "fn main() {".into(),
                    }],
                    checksum: "def456".into(),
                }],
            }],
            summary: ScanSummary {
                total_files: 1,
                total_hunks: 1,
                modified: 1,
                ..ScanSummary::default()
            },
        };
        let json = serde_json::to_string(&result).expect("serialize");
        let back: ScanResult = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(result, back);
    }

    #[test]
    fn stage_result_json_round_trip() {
        let result = StageResult {
            status: OperationStatus::Ok,
            succeeded: vec![StagedItem {
                selection: "src/main.rs".into(),
                lines_staged: 5,
            }],
            failed: vec![],
            warnings: vec![],
            backup_id: "backup-001".into(),
        };
        let json = serde_json::to_string(&result).expect("serialize");
        let back: StageResult = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(result, back);
    }

    #[test]
    fn status_report_json_round_trip() {
        let report = StatusReport {
            staged_files: vec![StagedFileInfo {
                path: "src/lib.rs".into(),
                status: FileStatus::Added,
                lines_added: 10,
                lines_deleted: 0,
                old_mode: 0,
                new_mode: 0o100_644,
            }],
            summary: StatusSummary {
                total_files: 1,
                total_additions: 10,
                total_deletions: 0,
            },
        };
        let json = serde_json::to_string(&report).expect("serialize");
        let back: StatusReport = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(report, back);
    }

    #[test]
    fn commit_result_json_round_trip() {
        let result = CommitResult {
            commit_hash: "a1b2c3d4".into(),
            message: "feat: add feature".into(),
            author: "Test <test@test.com>".into(),
            files_changed: 3,
            insertions: 15,
            deletions: 5,
        };
        let json = serde_json::to_string(&result).expect("serialize");
        let back: CommitResult = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(result, back);
    }

    #[test]
    fn file_status_renamed_serializes_with_old_path() {
        let status = FileStatus::Renamed {
            old_path: "old/name.rs".into(),
        };
        let json = serde_json::to_string(&status).expect("serialize");
        assert!(json.contains("old/name.rs"));
        assert!(json.contains("Renamed"));
    }

    #[test]
    fn operation_status_variants_serialize() {
        for status in [OperationStatus::Ok, OperationStatus::DryRun] {
            let json = serde_json::to_string(&status).expect("serialize");
            let back: OperationStatus = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(status, back);
        }
    }

    #[test]
    fn compact_scan_from_full_preserves_hunk_ids() {
        let full = make_test_scan();
        let compact = CompactScanResult::from(&full);
        assert_eq!(
            compact.files[0].hunks[0].hunk_id,
            full.files[0].hunks[0].hunk_id
        );
    }

    #[test]
    fn compact_scan_counts_additions_and_deletions() {
        let compact = CompactScanResult::from(&make_test_scan());
        let file = &compact.files[0];
        assert_eq!(file.lines_added, 2);
        assert_eq!(file.lines_deleted, 1);
        assert_eq!(file.hunks[0].additions, 2);
        assert_eq!(file.hunks[0].deletions, 1);
    }

    #[test]
    fn compact_scan_binary_file_has_empty_hunks() {
        let compact = CompactScanResult::from(&make_test_scan());
        let binary = &compact.files[1];
        assert!(binary.is_binary);
        assert!(binary.hunks.is_empty());
        assert_eq!(binary.hunks_count, 0);
    }

    #[test]
    fn format_selection_file() {
        let spec = SelectionSpec::File {
            path: "src/main.rs".into(),
        };
        assert_eq!(format_selection(&spec), "src/main.rs");
    }

    #[test]
    fn format_selection_hunk() {
        let spec = SelectionSpec::Hunk {
            hunk_id: "abc123def456".into(),
        };
        assert_eq!(format_selection(&spec), "abc123def456");
    }

    #[test]
    fn format_selection_lines() {
        let spec = SelectionSpec::Lines {
            path: "src/lib.rs".into(),
            ranges: vec![
                LineRange { start: 1, end: 5 },
                LineRange { start: 10, end: 15 },
            ],
        };
        assert_eq!(format_selection(&spec), "src/lib.rs:1-5,10-15");
    }

    fn make_test_scan() -> ScanResult {
        ScanResult {
            files: vec![
                FileInfo {
                    path: "src/main.rs".into(),
                    status: FileStatus::Modified,
                    file_checksum: "abc123".into(),
                    is_binary: false,
                    old_mode: 0o100_644,
                    new_mode: 0o100_644,
                    hunks: vec![HunkInfo {
                        hunk_id: "h1a2b3c4d5e6".into(),
                        old_start: 10,
                        old_lines: 3,
                        new_start: 10,
                        new_lines: 5,
                        header: "@@ -10,3 +10,5 @@".into(),
                        lines: vec![
                            DiffLineInfo {
                                line_number: 10,
                                origin: LineOrigin::Context,
                                content: "fn main() {".into(),
                            },
                            DiffLineInfo {
                                line_number: 11,
                                origin: LineOrigin::Addition,
                                content: "    println!(\"hello\");".into(),
                            },
                            DiffLineInfo {
                                line_number: 12,
                                origin: LineOrigin::Addition,
                                content: "    println!(\"world\");".into(),
                            },
                            DiffLineInfo {
                                line_number: 11,
                                origin: LineOrigin::Deletion,
                                content: "    old_line();".into(),
                            },
                            DiffLineInfo {
                                line_number: 13,
                                origin: LineOrigin::Context,
                                content: "}".into(),
                            },
                        ],
                        checksum: "def456".into(),
                    }],
                },
                FileInfo {
                    path: "data.bin".into(),
                    status: FileStatus::Modified,
                    file_checksum: "bin999".into(),
                    is_binary: true,
                    old_mode: 0o100_644,
                    new_mode: 0o100_644,
                    hunks: vec![],
                },
            ],
            summary: ScanSummary {
                total_files: 2,
                total_hunks: 1,
                modified: 2,
                binary: 1,
                ..ScanSummary::default()
            },
        }
    }
}
