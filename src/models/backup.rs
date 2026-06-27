use serde::{Deserialize, Serialize};

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
