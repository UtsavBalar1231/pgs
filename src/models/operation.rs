use serde::{Deserialize, Serialize};

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
