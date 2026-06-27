use serde::{Deserialize, Serialize};

use super::scan::FileStatus;

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
