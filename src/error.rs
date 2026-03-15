/// All error types for pgs, mapped to CLI exit codes.
///
/// Exit codes:
/// - 0: Success
/// - 1: No effect (nothing to stage/unstage)
/// - 2: User error (invalid selection, bad arguments)
/// - 3: Conflict/failure (stale scan, index locked)
/// - 4: Internal error (git2, IO, bug)
use std::path::PathBuf;

use thiserror::Error;

/// Top-level error type for all pgs operations.
#[derive(Debug, Error)]
pub enum PgsError {
    // --- Exit code 1: No effect ---
    /// No unstaged changes were detected in the working tree.
    #[error("no changes detected in working tree")]
    NoChanges,

    /// The provided selection did not match any changes.
    #[error("selection matched no hunks")]
    SelectionEmpty,

    // --- Exit code 2: User error ---
    /// The selection syntax provided is invalid.
    #[error("invalid selection syntax: {detail}")]
    InvalidSelection {
        /// Detailed description of the syntax error.
        detail: String,
    },

    /// The specified line range is invalid (e.g., start > end or start < 1).
    #[error("invalid line range {start}-{end} in {path}")]
    InvalidLineRange {
        /// Path to the file.
        path: String,
        /// Starting line number.
        start: u32,
        /// Ending line number.
        end: u32,
    },

    /// The provided hunk ID does not exist in the scan result.
    #[error("unknown hunk ID: {hunk_id}")]
    UnknownHunkId {
        /// The missing hunk identifier.
        hunk_id: String,
    },

    /// The specified file path was not found in the current diff.
    #[error("file not found in diff: {path}")]
    FileNotInDiff {
        /// Path to the file.
        path: String,
    },

    /// Granular staging (hunk/lines) was attempted on a binary file.
    #[error("binary file {path}: only file-level staging is supported for binary files")]
    BinaryFileGranular {
        /// Path to the binary file.
        path: String,
    },

    /// Granular staging (hunk/lines) was attempted on an added or deleted file.
    #[error("{path}: only file-level staging is supported for added/deleted/renamed files")]
    GranularOnWholeFile {
        /// Path to the file.
        path: String,
    },

    // --- Exit code 3: Conflict (retryable — agent should re-scan) ---
    /// The file has changed on disk since the last scan.
    #[error("stale scan detected for {path}: file has changed since last scan")]
    StaleScan {
        /// Path to the stale file.
        path: String,
    },

    /// The git index is locked by another process.
    #[error("git index is locked by another process")]
    IndexLocked,

    /// Index-direct staging write failed.
    #[error("staging failed for {path}: {reason}")]
    StagingFailed {
        /// Path to the file.
        path: String,
        /// Reason for the failure.
        reason: String,
    },

    // --- Exit code 4: Internal error ---
    /// An error from the underlying libgit2 library.
    #[error("git error: {0}")]
    Git(#[from] git2::Error),

    /// A filesystem IO error.
    #[error("IO error on {path}: {source}")]
    Io {
        /// Path where the IO error occurred.
        path: PathBuf,
        /// The underlying IO error.
        source: std::io::Error,
    },

    /// Failed to serialize or deserialize JSON.
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    /// An unexpected internal error or bug.
    #[error("internal error: {0}")]
    Internal(String),
}

impl PgsError {
    /// Stable machine-readable error code for output rendering.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::NoChanges => "no_changes",
            Self::SelectionEmpty => "selection_empty",
            Self::InvalidSelection { .. } => "invalid_selection",
            Self::InvalidLineRange { .. } => "invalid_line_range",
            Self::UnknownHunkId { .. } => "unknown_hunk_id",
            Self::FileNotInDiff { .. } => "file_not_in_diff",
            Self::BinaryFileGranular { .. } => "binary_file_granular",
            Self::GranularOnWholeFile { .. } => "granular_on_whole_file",
            Self::StaleScan { .. } => "stale_scan",
            Self::IndexLocked => "index_locked",
            Self::StagingFailed { .. } => "staging_failed",
            Self::Git(_) => "git_error",
            Self::Io { .. } => "io_error",
            Self::Json(_) => "json_error",
            Self::Internal(_) => "internal_error",
        }
    }

    /// Map this error to the appropriate CLI exit code.
    pub const fn exit_code(&self) -> i32 {
        match self {
            Self::NoChanges | Self::SelectionEmpty => 1,

            Self::InvalidSelection { .. }
            | Self::InvalidLineRange { .. }
            | Self::UnknownHunkId { .. }
            | Self::FileNotInDiff { .. }
            | Self::BinaryFileGranular { .. }
            | Self::GranularOnWholeFile { .. } => 2,

            Self::StaleScan { .. } | Self::IndexLocked | Self::StagingFailed { .. } => 3,

            Self::Git(_) | Self::Io { .. } | Self::Json(_) | Self::Internal(_) => 4,
        }
    }

    /// Create an IO error with path context.
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_changes_maps_to_exit_code_1() {
        assert_eq!(PgsError::NoChanges.exit_code(), 1);
    }

    #[test]
    fn selection_empty_maps_to_exit_code_1() {
        assert_eq!(PgsError::SelectionEmpty.exit_code(), 1);
    }

    #[test]
    fn all_exit_code_2_variants_consistent() {
        let variants: Vec<PgsError> = vec![
            PgsError::InvalidSelection { detail: "x".into() },
            PgsError::InvalidLineRange {
                path: "f".into(),
                start: 5,
                end: 3,
            },
            PgsError::UnknownHunkId {
                hunk_id: "abc".into(),
            },
            PgsError::FileNotInDiff { path: "f".into() },
            PgsError::BinaryFileGranular {
                path: "binary.bin".into(),
            },
            PgsError::GranularOnWholeFile {
                path: "new.rs".into(),
            },
        ];
        for v in variants {
            assert_eq!(v.exit_code(), 2, "wrong exit code for: {v}");
        }
    }

    #[test]
    fn stale_scan_maps_to_exit_code_3() {
        let err = PgsError::StaleScan {
            path: "src/main.rs".into(),
        };
        assert_eq!(err.exit_code(), 3);
    }

    #[test]
    fn index_locked_maps_to_exit_code_3() {
        assert_eq!(PgsError::IndexLocked.exit_code(), 3);
    }

    #[test]
    fn staging_failed_maps_to_exit_code_3() {
        let err = PgsError::StagingFailed {
            path: "src/main.rs".into(),
            reason: "blob write failed".into(),
        };
        assert_eq!(err.exit_code(), 3);
    }

    #[test]
    fn internal_error_maps_to_exit_code_4() {
        let err = PgsError::Internal("bug".into());
        assert_eq!(err.exit_code(), 4);
    }

    #[test]
    fn io_error_includes_path_in_display() {
        let err = PgsError::io(
            "/some/path",
            std::io::Error::new(std::io::ErrorKind::NotFound, "not found"),
        );
        let msg = err.to_string();
        assert!(msg.contains("/some/path"), "message was: {msg}");
    }

    #[test]
    fn granular_on_whole_file_maps_to_exit_code_2() {
        let err = PgsError::GranularOnWholeFile {
            path: "new_file.rs".into(),
        };
        assert_eq!(err.exit_code(), 2);
        let msg = err.to_string();
        assert!(msg.contains("new_file.rs"), "message was: {msg}");
    }

    #[test]
    fn stale_scan_maps_to_stable_code() {
        let err = PgsError::StaleScan {
            path: "src/main.rs".into(),
        };
        assert_eq!(err.code(), "stale_scan");
    }
}
