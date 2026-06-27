use serde::{Deserialize, Serialize};

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
