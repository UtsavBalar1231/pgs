/// Shared data models for pgs.
///
/// Most types derive `Serialize + Deserialize`. Exception: `OriginMix`
/// derives neither — it is an internal classifier exposed only through view-model
/// conversions in `src/output/view/`.
pub mod backup;
pub mod commit;
pub mod operation;
pub mod plan;
pub mod preview;
pub mod scan;
pub mod selection;
pub mod status;

pub use backup::BackupInfo;
pub use commit::CommitResult;
pub use operation::{FailedItem, OperationStatus, StageResult, StagedItem};
pub use plan::{CommitPlan, PlannedCommit};
pub use preview::{OperationPreview, PreviewLine};
pub use scan::{
    CompactFileInfo, CompactHunkInfo, CompactScanResult, DiffLineInfo, FileInfo, FileStatus,
    HunkInfo, LineOrigin, OriginMix, ScanResult, ScanSummary,
};
pub use selection::{LineRange, ResolvedSelection, SelectionSpec, format_selection};
pub use status::{StagedFileInfo, StatusReport, StatusSummary};

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
                    whitespace_only: false,
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
                        whitespace_only: false,
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
