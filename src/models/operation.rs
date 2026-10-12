use serde::{Deserialize, Serialize};

// ─── Stage/Unstage Output ─────────────────────────────────────────

/// Overall operation outcome.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum OperationStatus {
    /// All selections were successfully applied.
    Ok,
    /// Operation was a dry-run; no changes made.
    DryRun,
}
