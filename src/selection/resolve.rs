/// Selection resolver: maps [`SelectionSpec`] against a [`ScanResult`] to
/// produce concrete [`ResolvedSelection`] values ready for staging.
use sha2::{Digest, Sha256};

use crate::error::PgsError;
use crate::models::{FileStatus, LineRange, ResolvedSelection, ScanResult, SelectionSpec};

/// Resolve a [`SelectionSpec`] against the provided scan result.
///
/// Returns a [`ResolvedSelection`] that identifies which file and which hunk
/// indices (and optional line ranges) are targeted.
///
/// # Errors
///
/// - [`PgsError::FileNotInDiff`] when the referenced path is absent.
/// - [`PgsError::UnknownHunkId`] when the referenced hunk ID is absent.
/// - [`PgsError::SelectionEmpty`] when a line-range selection matches no hunks.
pub fn resolve_selection(
    scan: &ScanResult,
    spec: &SelectionSpec,
) -> Result<ResolvedSelection, PgsError> {
    match spec {
        SelectionSpec::File { path } => resolve_file(scan, path),
        SelectionSpec::Hunk { hunk_id } => resolve_hunk(scan, hunk_id),
        SelectionSpec::Lines { path, ranges } => resolve_lines(scan, path, ranges),
        SelectionSpec::Directory { .. } => {
            unreachable!(
                "Directory specs must be resolved via resolve_directory(), not resolve_selection()"
            )
        }
    }
}

/// Resolve a directory-prefix selection: all files whose path starts with `prefix/`.
///
/// Returns every file in `scan` whose path equals `prefix` or starts with `prefix/`.
///
/// # Errors
///
/// Returns [`PgsError::FileNotInDiff`] when no files match the prefix.
pub fn resolve_directory(
    scan: &ScanResult,
    prefix: &str,
) -> Result<Vec<ResolvedSelection>, PgsError> {
    let normalized = prefix.strip_suffix('/').unwrap_or(prefix);
    let matches: Vec<ResolvedSelection> = scan
        .files
        .iter()
        .filter(|f| f.path == normalized || f.path.starts_with(&format!("{normalized}/")))
        .map(|f| ResolvedSelection {
            file_path: f.path.clone(),
            hunk_indices: (0..f.hunks.len()).collect(),
            line_ranges: None,
        })
        .collect();
    if matches.is_empty() {
        return Err(PgsError::FileNotInDiff {
            path: format!("{normalized}/"),
        });
    }
    Ok(matches)
}

/// Resolve a file-level selection: all hunks in the file.
fn resolve_file(scan: &ScanResult, path: &str) -> Result<ResolvedSelection, PgsError> {
    let file =
        scan.files
            .iter()
            .find(|f| f.path == path)
            .ok_or_else(|| PgsError::FileNotInDiff {
                path: path.to_owned(),
            })?;

    let hunk_indices: Vec<usize> = (0..file.hunks.len()).collect();

    Ok(ResolvedSelection {
        file_path: path.to_owned(),
        hunk_indices,
        line_ranges: None,
    })
}

/// Resolve a hunk-level selection by content-based ID.
fn resolve_hunk(scan: &ScanResult, hunk_id: &str) -> Result<ResolvedSelection, PgsError> {
    for file in &scan.files {
        for (idx, hunk) in file.hunks.iter().enumerate() {
            if hunk.hunk_id == hunk_id {
                return Ok(ResolvedSelection {
                    file_path: file.path.clone(),
                    hunk_indices: vec![idx],
                    line_ranges: None,
                });
            }
        }
    }

    Err(PgsError::UnknownHunkId {
        hunk_id: hunk_id.to_owned(),
    })
}

/// Resolve a line-range selection: hunks that overlap any of the given ranges.
fn resolve_lines(
    scan: &ScanResult,
    path: &str,
    ranges: &[LineRange],
) -> Result<ResolvedSelection, PgsError> {
    let file =
        scan.files
            .iter()
            .find(|f| f.path == path)
            .ok_or_else(|| PgsError::FileNotInDiff {
                path: path.to_owned(),
            })?;

    let hunk_indices: Vec<usize> = file
        .hunks
        .iter()
        .enumerate()
        .filter(|(_, hunk)| {
            // A hunk overlaps if any range intersects [new_start, new_start + new_lines - 1].
            // Use new_start/new_lines for addition/context hunks; for deletion-only hunks
            // (new_lines == 0) fall back to old_start range.
            let (hunk_start, hunk_end) = if hunk.new_lines > 0 {
                (
                    hunk.new_start,
                    hunk.new_start + hunk.new_lines.saturating_sub(1),
                )
            } else {
                (
                    hunk.old_start,
                    hunk.old_start + hunk.old_lines.saturating_sub(1),
                )
            };

            ranges
                .iter()
                .any(|r| r.start <= hunk_end && r.end >= hunk_start)
        })
        .map(|(idx, _)| idx)
        .collect();

    if hunk_indices.is_empty() {
        return Err(PgsError::SelectionEmpty);
    }

    Ok(ResolvedSelection {
        file_path: path.to_owned(),
        hunk_indices,
        line_ranges: Some(ranges.to_vec()),
    })
}

/// Reject granular (hunk/lines) selections on binary files.
///
/// File-level selections on binary files are always permitted (they are
/// handled by the index-direct strategy which works on raw bytes).
///
/// # Errors
///
/// Returns [`PgsError::BinaryFileGranular`] when a hunk or lines
/// selection targets a binary file.
pub fn validate_binary_constraints(
    scan: &ScanResult,
    spec: &SelectionSpec,
) -> Result<(), PgsError> {
    let path = match spec {
        SelectionSpec::File { .. } | SelectionSpec::Directory { .. } => return Ok(()),
        SelectionSpec::Hunk { hunk_id } => {
            // Find the file that owns this hunk.
            scan.files
                .iter()
                .find(|f| f.hunks.iter().any(|h| h.hunk_id == *hunk_id))
                .map(|f| f.path.as_str())
        }
        SelectionSpec::Lines { path, .. } => scan
            .files
            .iter()
            .find(|f| f.path == *path)
            .map(|f| f.path.as_str()),
    };

    if let Some(p) = path {
        if let Some(file) = scan.files.iter().find(|f| f.path == p) {
            if file.is_binary {
                return Err(PgsError::BinaryFileGranular { path: p.to_owned() });
            }
        }
    }

    Ok(())
}

/// Reject granular (hunk/lines) selections on added, deleted, or renamed files.
///
/// Such files can only be staged at file level because the entire file must be
/// moved in or out of the index atomically.
///
/// # Errors
///
/// Returns [`PgsError::GranularOnWholeFile`] when a hunk or lines
/// selection targets an added, deleted, or renamed file.
pub fn validate_whole_file_constraints(
    scan: &ScanResult,
    spec: &SelectionSpec,
) -> Result<(), PgsError> {
    let target_path: Option<&str> = match spec {
        SelectionSpec::File { .. } | SelectionSpec::Directory { .. } => return Ok(()),
        SelectionSpec::Hunk { hunk_id } => scan
            .files
            .iter()
            .find(|f| f.hunks.iter().any(|h| h.hunk_id == *hunk_id))
            .map(|f| f.path.as_str()),
        SelectionSpec::Lines { path, .. } => scan
            .files
            .iter()
            .find(|f| f.path == *path)
            .map(|f| f.path.as_str()),
    };

    if let Some(p) = target_path {
        if let Some(file) = scan.files.iter().find(|f| f.path == p) {
            let is_whole_file_only = matches!(
                file.status,
                FileStatus::Added | FileStatus::Deleted | FileStatus::Renamed { .. }
            );
            if is_whole_file_only {
                return Err(PgsError::GranularOnWholeFile { path: p.to_owned() });
            }
        }
    }

    Ok(())
}

/// Validate that the file on disk matches the checksum recorded in the scan.
///
/// Skips validation when:
/// - The file status is `Deleted` (no working-tree file to read).
/// - The recorded `file_checksum` is empty (checksum was skipped via
///   `--skip-checksum` or `--max-filesize`).
///
/// # Errors
///
/// - [`PgsError::StaleScan`] when the working-tree content differs from
///   the scan-time checksum.
/// - [`PgsError::Io`] when the file cannot be read from disk.
pub fn validate_freshness(
    repo: &git2::Repository,
    scan: &ScanResult,
    file_path: &str,
) -> Result<(), PgsError> {
    let Some(file_info) = scan.files.iter().find(|f| f.path == file_path) else {
        return Ok(()); // Not in scan — nothing to validate.
    };

    // Skip for deleted files: they have no workdir content to checksum.
    if matches!(file_info.status, FileStatus::Deleted) {
        return Ok(());
    }

    // Skip when checksum was intentionally omitted.
    if file_info.file_checksum.is_empty() {
        return Ok(());
    }

    let workdir = repo.workdir().ok_or_else(|| {
        PgsError::Internal("repository has no working directory (bare repo)".into())
    })?;

    let abs_path = workdir.join(file_path);
    let content = std::fs::read(&abs_path).map_err(|e| PgsError::Io {
        path: abs_path.clone(),
        source: e,
    })?;

    let mut hasher = Sha256::new();
    hasher.update(&content);
    let digest = format!("{:x}", hasher.finalize());

    if digest != file_info.file_checksum {
        return Err(PgsError::StaleScan {
            path: file_path.to_owned(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        DiffLineInfo, FileInfo, FileStatus, HunkInfo, LineOrigin, ScanResult, ScanSummary,
    };

    // ── Helpers ───────────────────────────────────────────────────

    fn make_scan() -> ScanResult {
        ScanResult {
            files: vec![
                FileInfo {
                    path: "src/main.rs".into(),
                    status: FileStatus::Modified,
                    file_checksum: "aabbcc".into(),
                    is_binary: false,
                    old_mode: 0o100_644,
                    new_mode: 0o100_644,
                    hunks: vec![
                        HunkInfo {
                            hunk_id: "aaa111bbb222".into(),
                            old_start: 10,
                            old_lines: 3,
                            new_start: 10,
                            new_lines: 3,
                            header: "@@ -10,3 +10,3 @@".into(),
                            lines: vec![DiffLineInfo {
                                line_number: 10,
                                origin: LineOrigin::Addition,
                                content: "x".into(),
                            }],
                            checksum: "cs1".into(),
                            whitespace_only: false,
                        },
                        HunkInfo {
                            hunk_id: "ccc333ddd444".into(),
                            old_start: 30,
                            old_lines: 2,
                            new_start: 30,
                            new_lines: 2,
                            header: "@@ -30,2 +30,2 @@".into(),
                            lines: vec![DiffLineInfo {
                                line_number: 30,
                                origin: LineOrigin::Addition,
                                content: "y".into(),
                            }],
                            checksum: "cs2".into(),
                            whitespace_only: false,
                        },
                    ],
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
                FileInfo {
                    path: "new_file.rs".into(),
                    status: FileStatus::Added,
                    file_checksum: "new111".into(),
                    is_binary: false,
                    old_mode: 0o100_644,
                    new_mode: 0o100_644,
                    hunks: vec![HunkInfo {
                        hunk_id: "eee555fff666".into(),
                        old_start: 0,
                        old_lines: 0,
                        new_start: 1,
                        new_lines: 5,
                        header: "@@ -0,0 +1,5 @@".into(),
                        lines: vec![],
                        checksum: "cs3".into(),
                        whitespace_only: false,
                    }],
                },
                FileInfo {
                    path: "gone.rs".into(),
                    status: FileStatus::Deleted,
                    file_checksum: String::new(),
                    is_binary: false,
                    old_mode: 0o100_644,
                    new_mode: 0o100_644,
                    hunks: vec![HunkInfo {
                        hunk_id: "del111222333".into(),
                        old_start: 1,
                        old_lines: 2,
                        new_start: 0,
                        new_lines: 0,
                        header: "@@ -1,2 +0,0 @@".into(),
                        lines: vec![],
                        checksum: "cs4".into(),
                        whitespace_only: false,
                    }],
                },
            ],
            summary: ScanSummary {
                total_files: 4,
                total_hunks: 4,
                modified: 2,
                added: 1,
                deleted: 1,
                ..ScanSummary::default()
            },
        }
    }

    // ── resolve_file ──────────────────────────────────────────────

    #[test]
    fn resolve_file_returns_all_hunk_indices() {
        let scan = make_scan();
        let spec = SelectionSpec::File {
            path: "src/main.rs".into(),
        };
        let resolved = resolve_selection(&scan, &spec).unwrap();
        assert_eq!(resolved.file_path, "src/main.rs");
        assert_eq!(resolved.hunk_indices, vec![0, 1]);
        assert!(resolved.line_ranges.is_none());
    }

    #[test]
    fn resolve_file_unknown_path_returns_file_not_in_diff() {
        let scan = make_scan();
        let spec = SelectionSpec::File {
            path: "nonexistent.rs".into(),
        };
        let err = resolve_selection(&scan, &spec).unwrap_err();
        assert!(
            matches!(err, PgsError::FileNotInDiff { .. }),
            "unexpected: {err}"
        );
    }

    // ── resolve_hunk ──────────────────────────────────────────────

    #[test]
    fn resolve_hunk_finds_by_id() {
        let scan = make_scan();
        let spec = SelectionSpec::Hunk {
            hunk_id: "ccc333ddd444".into(),
        };
        let resolved = resolve_selection(&scan, &spec).unwrap();
        assert_eq!(resolved.file_path, "src/main.rs");
        assert_eq!(resolved.hunk_indices, vec![1]);
    }

    #[test]
    fn resolve_hunk_unknown_id_returns_unknown_hunk_id() {
        let scan = make_scan();
        let spec = SelectionSpec::Hunk {
            hunk_id: "000000000000".into(),
        };
        let err = resolve_selection(&scan, &spec).unwrap_err();
        assert!(
            matches!(err, PgsError::UnknownHunkId { .. }),
            "unexpected: {err}"
        );
    }

    // ── resolve_lines ─────────────────────────────────────────────

    #[test]
    fn resolve_lines_finds_overlapping_hunks() {
        let scan = make_scan();
        // Range 10-12 overlaps hunk at new_start=10, new_lines=3 (lines 10-12).
        let spec = SelectionSpec::Lines {
            path: "src/main.rs".into(),
            ranges: vec![LineRange { start: 10, end: 12 }],
        };
        let resolved = resolve_selection(&scan, &spec).unwrap();
        assert_eq!(resolved.hunk_indices, vec![0]);
        assert!(resolved.line_ranges.is_some());
    }

    // ── validate_binary_constraints ───────────────────────────────

    #[test]
    fn validate_binary_allows_file_on_binary() {
        let scan = make_scan();
        let spec = SelectionSpec::File {
            path: "data.bin".into(),
        };
        assert!(validate_binary_constraints(&scan, &spec).is_ok());
    }

    #[test]
    fn validate_binary_rejects_lines_on_binary() {
        let scan = make_scan();
        let spec = SelectionSpec::Lines {
            path: "data.bin".into(),
            ranges: vec![LineRange { start: 1, end: 5 }],
        };
        let err = validate_binary_constraints(&scan, &spec).unwrap_err();
        assert!(
            matches!(err, PgsError::BinaryFileGranular { .. }),
            "unexpected: {err}"
        );
    }

    #[test]
    fn validate_binary_allows_lines_on_text_file() {
        let scan = make_scan();
        let spec = SelectionSpec::Lines {
            path: "src/main.rs".into(),
            ranges: vec![LineRange { start: 1, end: 5 }],
        };
        assert!(validate_binary_constraints(&scan, &spec).is_ok());
    }

    // ── validate_whole_file_constraints ──────────────────────────

    #[test]
    fn validate_whole_file_allows_file_on_added() {
        let scan = make_scan();
        let spec = SelectionSpec::File {
            path: "new_file.rs".into(),
        };
        assert!(validate_whole_file_constraints(&scan, &spec).is_ok());
    }

    #[test]
    fn validate_whole_file_rejects_hunk_on_added_file() {
        let scan = make_scan();
        let spec = SelectionSpec::Hunk {
            hunk_id: "eee555fff666".into(),
        };
        let err = validate_whole_file_constraints(&scan, &spec).unwrap_err();
        assert!(
            matches!(err, PgsError::GranularOnWholeFile { .. }),
            "unexpected: {err}"
        );
    }

    #[test]
    fn validate_whole_file_rejects_lines_on_deleted_file() {
        let scan = make_scan();
        let spec = SelectionSpec::Lines {
            path: "gone.rs".into(),
            ranges: vec![LineRange { start: 1, end: 2 }],
        };
        let err = validate_whole_file_constraints(&scan, &spec).unwrap_err();
        assert!(
            matches!(err, PgsError::GranularOnWholeFile { .. }),
            "unexpected: {err}"
        );
    }

    #[test]
    fn validate_whole_file_allows_hunk_on_modified_file() {
        let scan = make_scan();
        let spec = SelectionSpec::Hunk {
            hunk_id: "aaa111bbb222".into(),
        };
        assert!(validate_whole_file_constraints(&scan, &spec).is_ok());
    }

    // ── validate_freshness ────────────────────────────────────────

    #[test]
    fn validate_freshness_passes_for_matching_checksum() {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();

        // Write the file and compute its real checksum.
        let content = b"hello world\n";
        std::fs::write(dir.path().join("src/main.rs"), content).unwrap_or_else(|_| {
            std::fs::create_dir_all(dir.path().join("src")).unwrap();
            std::fs::write(dir.path().join("src/main.rs"), content).unwrap();
        });

        let mut hasher = sha2::Sha256::new();
        hasher.update(content);
        let checksum = format!("{:x}", hasher.finalize());

        let scan = ScanResult {
            files: vec![FileInfo {
                path: "src/main.rs".into(),
                status: FileStatus::Modified,
                file_checksum: checksum,
                is_binary: false,
                old_mode: 0o100_644,
                new_mode: 0o100_644,
                hunks: vec![],
            }],
            summary: ScanSummary::default(),
        };

        // Ensure the file exists (handle nested dir creation).
        std::fs::create_dir_all(dir.path().join("src")).ok();
        std::fs::write(dir.path().join("src/main.rs"), content).unwrap();

        let result = validate_freshness(&repo, &scan, "src/main.rs");
        assert!(result.is_ok(), "expected Ok, got: {result:?}");
    }

    #[test]
    fn validate_freshness_fails_for_mismatched_checksum() {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();

        std::fs::write(dir.path().join("file.txt"), b"current content\n").unwrap();

        let scan = ScanResult {
            files: vec![FileInfo {
                path: "file.txt".into(),
                status: FileStatus::Modified,
                file_checksum: "000000000000000000000000000000000000000000000000000000000000dead"
                    .into(),
                is_binary: false,
                old_mode: 0o100_644,
                new_mode: 0o100_644,
                hunks: vec![],
            }],
            summary: ScanSummary::default(),
        };

        let err = validate_freshness(&repo, &scan, "file.txt").unwrap_err();
        assert!(
            matches!(err, PgsError::StaleScan { .. }),
            "unexpected: {err}"
        );
    }

    #[test]
    fn validate_freshness_skips_deleted_files() {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();

        // No file on disk — but Deleted status should skip the check.
        let scan = ScanResult {
            files: vec![FileInfo {
                path: "gone.rs".into(),
                status: FileStatus::Deleted,
                file_checksum: "some_checksum".into(),
                is_binary: false,
                old_mode: 0o100_644,
                new_mode: 0o100_644,
                hunks: vec![],
            }],
            summary: ScanSummary::default(),
        };

        let result = validate_freshness(&repo, &scan, "gone.rs");
        assert!(
            result.is_ok(),
            "expected Ok for deleted file, got: {result:?}"
        );
    }

    #[test]
    fn validate_freshness_skips_empty_checksum() {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();

        // File exists on disk but checksum was skipped in scan.
        std::fs::write(dir.path().join("file.txt"), b"data\n").unwrap();

        let scan = ScanResult {
            files: vec![FileInfo {
                path: "file.txt".into(),
                status: FileStatus::Modified,
                file_checksum: String::new(),
                is_binary: false,
                old_mode: 0o100_644,
                new_mode: 0o100_644,
                hunks: vec![],
            }],
            summary: ScanSummary::default(),
        };

        let result = validate_freshness(&repo, &scan, "file.txt");
        assert!(
            result.is_ok(),
            "expected Ok for empty checksum, got: {result:?}"
        );
    }

    // ── resolve_directory ─────────────────────────────────────────

    #[test]
    fn resolve_directory_returns_matching_files() {
        let scan = make_scan();
        let results = resolve_directory(&scan, "src").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_path, "src/main.rs");
        assert_eq!(results[0].hunk_indices, vec![0, 1]);
        assert!(results[0].line_ranges.is_none());
    }

    #[test]
    fn resolve_directory_no_match_returns_error() {
        let scan = make_scan();
        let err = resolve_directory(&scan, "nonexistent").unwrap_err();
        assert!(
            matches!(err, PgsError::FileNotInDiff { .. }),
            "unexpected: {err}"
        );
    }

    #[test]
    fn resolve_directory_does_not_match_partial_names() {
        let scan = make_scan();
        let err = resolve_directory(&scan, "sr").unwrap_err();
        assert!(
            matches!(err, PgsError::FileNotInDiff { .. }),
            "sr should not match src/main.rs: {err}"
        );
    }

    #[test]
    fn validate_binary_allows_directory() {
        let scan = make_scan();
        let spec = SelectionSpec::Directory {
            prefix: "src".into(),
        };
        assert!(validate_binary_constraints(&scan, &spec).is_ok());
    }

    #[test]
    fn validate_whole_file_allows_directory() {
        let scan = make_scan();
        let spec = SelectionSpec::Directory {
            prefix: "src".into(),
        };
        assert!(validate_whole_file_constraints(&scan, &spec).is_ok());
    }
}
