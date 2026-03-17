/// Diff engine — index-to-workdir and HEAD-to-index diffs for pgs.
use sha2::{Digest, Sha256};

use git2::{Delta, Diff, DiffOptions, Patch, Repository};

use crate::error::PgsError;
use crate::models::{
    DiffLineInfo, FileInfo, FileStatus, HunkInfo, LineOrigin, ScanResult, ScanSummary,
    StagedFileInfo, StatusReport, StatusSummary,
};

/// Compute a diff between the index and the working directory.
///
/// This is the diff base for `scan`: it shows only unstaged changes,
/// correctly excluding content that has already been staged.
pub fn diff_index_to_workdir(repo: &Repository, context_lines: u32) -> Result<Diff<'_>, PgsError> {
    let mut opts = DiffOptions::new();
    opts.context_lines(context_lines);
    opts.include_untracked(true);
    opts.recurse_untracked_dirs(true);
    opts.show_untracked_content(true);
    let diff = repo.diff_index_to_workdir(None, Some(&mut opts))?;
    Ok(diff)
}

/// Compute a diff between HEAD and the index.
///
/// This is the diff base for `status`: it shows what is currently staged
/// for the next commit.
pub fn diff_head_to_index(repo: &Repository, context_lines: u32) -> Result<Diff<'_>, PgsError> {
    let head_commit = repo.head()?.peel_to_commit()?;
    let head_tree = head_commit.tree()?;
    let mut opts = DiffOptions::new();
    opts.context_lines(context_lines);
    let diff = repo.diff_tree_to_index(Some(&head_tree), None, Some(&mut opts))?;
    Ok(diff)
}

/// Build a `ScanResult` from a diff (index-to-workdir).
///
/// If `file_filter` is `Some`, only files whose paths appear in the list
/// are included in the result.
pub fn build_scan_result(
    repo: &Repository,
    diff: &Diff<'_>,
    file_filter: Option<&[String]>,
) -> Result<ScanResult, PgsError> {
    let mut files: Vec<FileInfo> = Vec::new();
    let mut summary = ScanSummary::default();

    let count = diff.deltas().count();
    for i in 0..count {
        let delta = diff.get_delta(i).expect("delta index in bounds");
        let path = delta_path(&delta)?;

        // Apply file filter — support exact paths and directory prefixes
        if let Some(filter) = file_filter {
            if !filter.iter().any(|f| {
                let f_normalized = f.strip_suffix('/').unwrap_or(f);
                path == *f_normalized || path.starts_with(&format!("{f_normalized}/"))
            }) {
                continue;
            }
        }

        let status = delta_to_file_status(&delta);
        let old_mode: u32 = delta.old_file().mode().into();
        let new_mode: u32 = delta.new_file().mode().into();

        // Build patch to inspect the diff
        let patch_result = Patch::from_diff(diff, i)?;

        // Detect binary: first from delta flags, then by reading workdir file
        let is_binary = if delta.flags().is_binary() {
            true
        } else if delta.flags().is_not_binary() {
            false
        } else {
            // Fallback: read workdir file and scan for null bytes
            read_workdir_bytes(repo, &path).is_some_and(|bytes| super::content_is_binary(&bytes))
        };

        // Compute checksum from workdir file content
        let file_checksum = compute_file_checksum(repo, &path);

        let hunks = if is_binary {
            Vec::new()
        } else if let Some(patch) = patch_result {
            extract_hunks(&patch, &path)?
        } else {
            Vec::new()
        };

        // Update summary
        summary.total_hunks += hunks.len();
        match &status {
            FileStatus::Added => summary.added += 1,
            FileStatus::Modified => summary.modified += 1,
            FileStatus::Deleted => summary.deleted += 1,
            FileStatus::Renamed { .. } => summary.renamed += 1,
        }
        if is_binary {
            summary.binary += 1;
        }
        if old_mode != new_mode {
            summary.mode_changed += 1;
        }

        files.push(FileInfo {
            path,
            status,
            file_checksum,
            is_binary,
            old_mode,
            new_mode,
            hunks,
        });
    }

    summary.total_files = files.len();
    Ok(ScanResult { files, summary })
}

/// Build a `StatusReport` from a HEAD-to-index diff.
pub fn build_status_report(diff: &Diff<'_>) -> Result<StatusReport, PgsError> {
    let mut staged_files: Vec<StagedFileInfo> = Vec::new();
    let mut summary = StatusSummary::default();

    let count = diff.deltas().count();
    for i in 0..count {
        let delta = diff.get_delta(i).expect("delta index in bounds");
        let path = delta_path(&delta)?;
        let status = delta_to_file_status(&delta);
        let old_mode: u32 = delta.old_file().mode().into();
        let new_mode: u32 = delta.new_file().mode().into();

        let patch_result = Patch::from_diff(diff, i)?;
        let (lines_added, lines_deleted) = if let Some(patch) = patch_result {
            patch_line_counts(&patch)
        } else {
            (0, 0)
        };

        summary.total_additions += lines_added;
        summary.total_deletions += lines_deleted;

        staged_files.push(StagedFileInfo {
            path,
            status,
            lines_added,
            lines_deleted,
            old_mode,
            new_mode,
        });
    }

    summary.total_files = staged_files.len();
    Ok(StatusReport {
        staged_files,
        summary,
    })
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Extract the file path from a diff delta (new path for non-deletions, old path for deletions).
fn delta_path(delta: &git2::DiffDelta<'_>) -> Result<String, PgsError> {
    // Use new_file for everything except pure deletions
    let file = if delta.status() == Delta::Deleted {
        delta.old_file()
    } else {
        delta.new_file()
    };
    file.path()
        .and_then(|p| p.to_str())
        .map(ToString::to_string)
        .ok_or_else(|| PgsError::Internal("diff delta has non-UTF-8 path".into()))
}

/// Convert a git2 delta status to pgs `FileStatus`.
fn delta_to_file_status(delta: &git2::DiffDelta<'_>) -> FileStatus {
    match delta.status() {
        Delta::Added | Delta::Untracked => FileStatus::Added,
        Delta::Deleted => FileStatus::Deleted,
        Delta::Renamed => {
            let old_path = delta
                .old_file()
                .path()
                .and_then(|p| p.to_str())
                .unwrap_or("")
                .to_string();
            FileStatus::Renamed { old_path }
        }
        _ => FileStatus::Modified,
    }
}

/// Extract `HunkInfo` entries from a `Patch`.
fn extract_hunks(patch: &Patch<'_>, file_path: &str) -> Result<Vec<HunkInfo>, PgsError> {
    let hunk_count = patch.num_hunks();
    let mut hunks = Vec::with_capacity(hunk_count);

    for h in 0..hunk_count {
        let (hunk, _line_count) = patch.hunk(h)?;
        let old_start = hunk.old_start();
        let old_lines = hunk.old_lines();
        let new_start = hunk.new_start();
        let new_lines = hunk.new_lines();
        let header = String::from_utf8_lossy(hunk.header())
            .trim_end()
            .to_string();

        let line_count = patch.num_lines_in_hunk(h)?;
        let mut lines: Vec<DiffLineInfo> = Vec::with_capacity(line_count);
        let mut hunk_content = String::new();

        for l in 0..line_count {
            let line = patch.line_in_hunk(h, l)?;
            let origin = match line.origin_value() {
                git2::DiffLineType::Context => LineOrigin::Context,
                git2::DiffLineType::Addition => LineOrigin::Addition,
                git2::DiffLineType::Deletion => LineOrigin::Deletion,
                // Skip file headers and other non-content lines
                _ => continue,
            };

            let content = std::str::from_utf8(line.content())
                .unwrap_or("")
                .trim_end_matches('\n')
                .trim_end_matches('\r')
                .to_string();

            // Line number: new file for context/additions, old file for deletions
            let line_number = match origin {
                LineOrigin::Deletion => line.old_lineno().unwrap_or(0),
                LineOrigin::Context | LineOrigin::Addition => line.new_lineno().unwrap_or(0),
            };

            hunk_content.push_str(&content);
            hunk_content.push('\n');

            lines.push(DiffLineInfo {
                line_number,
                origin,
                content,
            });
        }

        let hunk_id = compute_hunk_id(file_path, old_start, new_start, &hunk_content);
        let checksum = hex_sha256(hunk_content.as_bytes());

        hunks.push(HunkInfo {
            hunk_id,
            old_start,
            old_lines,
            new_start,
            new_lines,
            header,
            lines,
            checksum,
        });
    }

    Ok(hunks)
}

/// Count added and deleted lines in a patch.
fn patch_line_counts(patch: &Patch<'_>) -> (u32, u32) {
    let mut added = 0u32;
    let mut deleted = 0u32;
    for h in 0..patch.num_hunks() {
        let line_count = patch.num_lines_in_hunk(h).unwrap_or(0);
        for l in 0..line_count {
            if let Ok(line) = patch.line_in_hunk(h, l) {
                match line.origin_value() {
                    git2::DiffLineType::Addition => added += 1,
                    git2::DiffLineType::Deletion => deleted += 1,
                    _ => {}
                }
            }
        }
    }
    (added, deleted)
}

/// Compute a content-based hunk ID: first 12 hex chars of SHA-256 of
/// `"path:old_start:new_start:content"`.
fn compute_hunk_id(path: &str, old_start: u32, new_start: u32, content: &str) -> String {
    let input = format!("{path}:{old_start}:{new_start}:{content}");
    hex_sha256(input.as_bytes())[..12].to_string()
}

/// Read working-directory file bytes for binary detection fallback.
fn read_workdir_bytes(repo: &Repository, path: &str) -> Option<Vec<u8>> {
    let workdir = repo.workdir()?;
    let full_path = workdir.join(path);
    std::fs::read(full_path).ok()
}

/// Compute SHA-256 hex checksum of a workdir file, or empty string on failure.
fn compute_file_checksum(repo: &Repository, path: &str) -> String {
    read_workdir_bytes(repo, path)
        .map(|bytes| hex_sha256(&bytes))
        .unwrap_or_default()
}

/// Return the full 64-char lowercase hex SHA-256 of `data`.
fn hex_sha256(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    format!("{digest:x}")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use git2::Repository;
    use tempfile::TempDir;

    use super::*;

    // ── Test helpers ──────────────────────────────────────────────────────────

    fn setup_repo() -> (TempDir, Repository) {
        let dir = TempDir::new().expect("tempdir");
        let repo = Repository::init(dir.path()).expect("init");
        {
            let mut config = repo.config().expect("config");
            config.set_str("user.name", "Test").expect("set name");
            config
                .set_str("user.email", "test@test.com")
                .expect("set email");
        }
        (dir, repo)
    }

    fn commit_file(repo: &Repository, dir: &Path, rel_path: &str, content: &str, msg: &str) {
        let full = dir.join(rel_path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).expect("create dirs");
        }
        fs::write(&full, content).expect("write");
        let mut index = repo.index().expect("index");
        index.add_path(Path::new(rel_path)).expect("add");
        index.write().expect("write index");
        let tree_oid = index.write_tree().expect("write tree");
        let tree = repo.find_tree(tree_oid).expect("find tree");
        let sig = repo.signature().expect("sig");
        let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        let parents: Vec<&git2::Commit<'_>> = parent.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, msg, &tree, &parents)
            .expect("commit");
    }

    fn write_file(dir: &Path, rel_path: &str, content: &str) {
        let full = dir.join(rel_path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).expect("create dirs");
        }
        fs::write(full, content).expect("write");
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    #[test]
    fn empty_repo_scan_returns_empty_files() {
        let (dir, repo) = setup_repo();
        // Make an initial commit so HEAD exists
        commit_file(&repo, dir.path(), "README.md", "hello\n", "initial");
        let diff = diff_index_to_workdir(&repo, 3).expect("diff");
        let result = build_scan_result(&repo, &diff, None).expect("scan");
        assert!(result.files.is_empty());
        assert_eq!(result.summary.total_files, 0);
    }

    #[test]
    fn modified_file_detected() {
        let (dir, repo) = setup_repo();
        commit_file(
            &repo,
            dir.path(),
            "src/main.rs",
            "fn main() {}\n",
            "initial",
        );
        write_file(
            dir.path(),
            "src/main.rs",
            "fn main() {\n    println!(\"hi\");\n}\n",
        );

        let diff = diff_index_to_workdir(&repo, 3).expect("diff");
        let result = build_scan_result(&repo, &diff, None).expect("scan");

        assert_eq!(result.summary.modified, 1);
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].path, "src/main.rs");
        assert!(matches!(result.files[0].status, FileStatus::Modified));
        assert!(!result.files[0].hunks.is_empty());
    }

    #[test]
    fn new_untracked_file_detected_as_added() {
        let (dir, repo) = setup_repo();
        commit_file(&repo, dir.path(), "existing.rs", "fn a() {}\n", "initial");
        write_file(dir.path(), "new_file.rs", "fn new() {}\n");

        // Stage the new file (untracked files don't show in index-to-workdir
        // unless added to index first — we add it)
        let mut index = repo.index().expect("index");
        index.add_path(Path::new("new_file.rs")).expect("add");
        index.write().expect("write");

        let diff = diff_index_to_workdir(&repo, 3).expect("diff");
        let result = build_scan_result(&repo, &diff, None).expect("scan");

        // After adding to index, the file is no longer in index-to-workdir diff
        // unless workdir content differs from index
        // Let's instead test the HEAD-to-index diff path
        drop(result);

        let head_diff = diff_head_to_index(&repo, 3).expect("head diff");
        let status = build_status_report(&head_diff).expect("status");
        let paths: Vec<&str> = status
            .staged_files
            .iter()
            .map(|f| f.path.as_str())
            .collect();
        assert!(paths.contains(&"new_file.rs"), "paths: {paths:?}");
        let new_file = status
            .staged_files
            .iter()
            .find(|f| f.path == "new_file.rs")
            .expect("find");
        assert!(matches!(new_file.status, FileStatus::Added));
    }

    #[test]
    fn deleted_file_detected() {
        let (dir, repo) = setup_repo();
        commit_file(
            &repo,
            dir.path(),
            "to_delete.rs",
            "fn bye() {}\n",
            "initial",
        );
        fs::remove_file(dir.path().join("to_delete.rs")).expect("remove");

        let diff = diff_index_to_workdir(&repo, 3).expect("diff");
        let result = build_scan_result(&repo, &diff, None).expect("scan");

        assert_eq!(result.summary.deleted, 1);
        let file = result
            .files
            .iter()
            .find(|f| f.path == "to_delete.rs")
            .expect("find");
        assert!(matches!(file.status, FileStatus::Deleted));
    }

    #[test]
    fn binary_file_detection_with_null_bytes() {
        let (dir, repo) = setup_repo();
        commit_file(&repo, dir.path(), "data.bin", "placeholder\n", "initial");
        // Write binary content with null bytes
        let binary: Vec<u8> = vec![0x89, 0x50, 0x4E, 0x47, 0x00, 0x0D, 0x0A, 0x1A];
        fs::write(dir.path().join("data.bin"), &binary).expect("write binary");

        let diff = diff_index_to_workdir(&repo, 3).expect("diff");
        let result = build_scan_result(&repo, &diff, None).expect("scan");

        let file = result
            .files
            .iter()
            .find(|f| f.path == "data.bin")
            .expect("find");
        assert!(file.is_binary, "should be binary");
        assert_eq!(result.summary.binary, 1);
    }

    #[test]
    fn binary_file_has_empty_hunks() {
        let (dir, repo) = setup_repo();
        commit_file(&repo, dir.path(), "img.png", "placeholder\n", "initial");
        let binary: Vec<u8> = b"GIF89a\x00\x01\x00\x01".to_vec();
        fs::write(dir.path().join("img.png"), &binary).expect("write");

        let diff = diff_index_to_workdir(&repo, 3).expect("diff");
        let result = build_scan_result(&repo, &diff, None).expect("scan");

        let file = result
            .files
            .iter()
            .find(|f| f.path == "img.png")
            .expect("find");
        assert!(file.hunks.is_empty(), "binary file should have no hunks");
    }

    #[test]
    fn hunk_ids_are_12_hex_chars() {
        let (dir, repo) = setup_repo();
        commit_file(&repo, dir.path(), "lib.rs", "fn a() {}\n", "initial");
        write_file(dir.path(), "lib.rs", "fn a() {}\nfn b() {}\n");

        let diff = diff_index_to_workdir(&repo, 3).expect("diff");
        let result = build_scan_result(&repo, &diff, None).expect("scan");

        for file in &result.files {
            for hunk in &file.hunks {
                assert_eq!(
                    hunk.hunk_id.len(),
                    12,
                    "hunk_id '{}' should be 12 chars",
                    hunk.hunk_id
                );
                assert!(
                    hunk.hunk_id.chars().all(|c| c.is_ascii_hexdigit()),
                    "hunk_id '{}' should be hex",
                    hunk.hunk_id
                );
            }
        }
    }

    #[test]
    fn hunk_ids_stable_across_rescans() {
        let (dir, repo) = setup_repo();
        commit_file(&repo, dir.path(), "stable.rs", "fn a() {}\n", "initial");
        write_file(dir.path(), "stable.rs", "fn a() {}\nfn b() {}\n");

        let diff1 = diff_index_to_workdir(&repo, 3).expect("diff1");
        let result1 = build_scan_result(&repo, &diff1, None).expect("scan1");

        let diff2 = diff_index_to_workdir(&repo, 3).expect("diff2");
        let result2 = build_scan_result(&repo, &diff2, None).expect("scan2");

        let ids1: Vec<&str> = result1.files[0]
            .hunks
            .iter()
            .map(|h| h.hunk_id.as_str())
            .collect();
        let ids2: Vec<&str> = result2.files[0]
            .hunks
            .iter()
            .map(|h| h.hunk_id.as_str())
            .collect();
        assert_eq!(ids1, ids2, "hunk IDs should be stable across rescans");
    }

    #[test]
    fn file_checksum_computed_64_hex_chars() {
        let (dir, repo) = setup_repo();
        commit_file(&repo, dir.path(), "checked.rs", "fn a() {}\n", "initial");
        write_file(dir.path(), "checked.rs", "fn a() {}\nfn b() {}\n");

        let diff = diff_index_to_workdir(&repo, 3).expect("diff");
        let result = build_scan_result(&repo, &diff, None).expect("scan");

        let file = result
            .files
            .iter()
            .find(|f| f.path == "checked.rs")
            .expect("find");
        assert_eq!(
            file.file_checksum.len(),
            64,
            "checksum should be 64 hex chars"
        );
        assert!(
            file.file_checksum.chars().all(|c| c.is_ascii_hexdigit()),
            "checksum should be hex"
        );
    }

    #[test]
    fn file_filter_restricts_output() {
        let (dir, repo) = setup_repo();
        commit_file(&repo, dir.path(), "a.rs", "fn a() {}\n", "initial");
        commit_file(&repo, dir.path(), "b.rs", "fn b() {}\n", "add b");
        write_file(dir.path(), "a.rs", "fn a() { /* changed */ }\n");
        write_file(dir.path(), "b.rs", "fn b() { /* changed */ }\n");

        let diff = diff_index_to_workdir(&repo, 3).expect("diff");
        let filter = vec!["a.rs".to_string()];
        let result = build_scan_result(&repo, &diff, Some(&filter)).expect("scan");

        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].path, "a.rs");
    }

    #[test]
    fn multiple_hunks_in_single_file() {
        let (dir, repo) = setup_repo();
        // Create file with content separated by many unchanged lines
        let original: String = (1..=30)
            .map(|i| ["line ", &i.to_string(), "\n"].concat())
            .collect();
        commit_file(&repo, dir.path(), "multi.rs", &original, "initial");

        // Modify line 1 and line 30 — far enough apart to create two hunks
        let mut modified = original;
        modified = modified.replacen("line 1\n", "CHANGED line 1\n", 1);
        modified = modified.replacen("line 30\n", "CHANGED line 30\n", 1);
        write_file(dir.path(), "multi.rs", &modified);

        let diff = diff_index_to_workdir(&repo, 3).expect("diff");
        let result = build_scan_result(&repo, &diff, None).expect("scan");

        let file = result
            .files
            .iter()
            .find(|f| f.path == "multi.rs")
            .expect("find");
        assert!(
            file.hunks.len() >= 2,
            "expected at least 2 hunks, got {}",
            file.hunks.len()
        );
    }

    #[test]
    fn status_report_empty_when_nothing_staged() {
        let (dir, repo) = setup_repo();
        commit_file(&repo, dir.path(), "foo.rs", "fn foo() {}\n", "initial");
        // Modify but don't stage
        write_file(dir.path(), "foo.rs", "fn foo() { /* changed */ }\n");

        let diff = diff_head_to_index(&repo, 3).expect("diff");
        let report = build_status_report(&diff).expect("report");

        assert_eq!(report.summary.total_files, 0);
        assert!(report.staged_files.is_empty());
    }

    #[test]
    fn status_report_shows_staged_file() {
        let (dir, repo) = setup_repo();
        commit_file(&repo, dir.path(), "staged.rs", "fn old() {}\n", "initial");
        write_file(dir.path(), "staged.rs", "fn new() {}\n");

        // Stage the change
        let mut index = repo.index().expect("index");
        index.add_path(Path::new("staged.rs")).expect("add");
        index.write().expect("write");

        let diff = diff_head_to_index(&repo, 3).expect("diff");
        let report = build_status_report(&diff).expect("report");

        assert_eq!(report.summary.total_files, 1);
        let file = report
            .staged_files
            .iter()
            .find(|f| f.path == "staged.rs")
            .expect("find");
        assert!(matches!(file.status, FileStatus::Modified));
        assert!(file.lines_added > 0 || file.lines_deleted > 0);
    }

    #[test]
    fn context_lines_affect_hunk_boundaries() {
        let (dir, repo) = setup_repo();
        // Two changes close together but not adjacent
        let lines: String = (1..=20)
            .map(|i| ["line ", &i.to_string(), "\n"].concat())
            .collect();
        commit_file(&repo, dir.path(), "ctx.rs", &lines, "initial");

        let mut modified = lines;
        modified = modified.replacen("line 3\n", "CHANGED 3\n", 1);
        modified = modified.replacen("line 8\n", "CHANGED 8\n", 1);
        write_file(dir.path(), "ctx.rs", &modified);

        // With context=3, the two changes (at lines 3 and 8, distance 5) may merge
        let diff_wide = diff_index_to_workdir(&repo, 3).expect("diff wide");
        let result_wide = build_scan_result(&repo, &diff_wide, None).expect("scan wide");

        // With context=1, fewer lines around each change
        let diff_narrow = diff_index_to_workdir(&repo, 1).expect("diff narrow");
        let result_narrow = build_scan_result(&repo, &diff_narrow, None).expect("scan narrow");

        let file_wide = result_wide
            .files
            .iter()
            .find(|f| f.path == "ctx.rs")
            .expect("find");
        let file_narrow = result_narrow
            .files
            .iter()
            .find(|f| f.path == "ctx.rs")
            .expect("find narrow");

        // Context lines affect the total line count per hunk; hunks may merge or split
        // The key assertion is that the results differ based on context setting
        let wide_total_lines: usize = file_wide.hunks.iter().map(|h| h.lines.len()).sum();
        let narrow_total_lines: usize = file_narrow.hunks.iter().map(|h| h.lines.len()).sum();
        assert!(
            wide_total_lines >= narrow_total_lines,
            "wider context ({wide_total_lines}) should have >= lines than narrow ({narrow_total_lines})"
        );
    }

    #[test]
    fn untracked_file_appears_in_index_to_workdir_diff() {
        let (dir, repo) = setup_repo();
        commit_file(&repo, dir.path(), "existing.rs", "fn a() {}\n", "initial");

        // Write a new file but do NOT add it to the index
        write_file(dir.path(), "brand_new.rs", "fn brand_new() {}\n");

        let diff = diff_index_to_workdir(&repo, 3).expect("diff");
        let result = build_scan_result(&repo, &diff, None).expect("scan");

        let file = result
            .files
            .iter()
            .find(|f| f.path == "brand_new.rs")
            .expect("untracked file should appear in scan");
        assert!(
            matches!(file.status, FileStatus::Added),
            "untracked file should be Added, got {:?}",
            file.status
        );
        assert!(!file.is_binary);
        assert_eq!(
            file.file_checksum.len(),
            64,
            "checksum should be 64 hex chars"
        );
        assert!(
            !file.hunks.is_empty(),
            "untracked file should have hunks with all lines as additions"
        );
    }

    #[test]
    fn gitignored_untracked_file_not_in_scan() {
        let (dir, repo) = setup_repo();
        commit_file(&repo, dir.path(), ".gitignore", "*.log\n", "add gitignore");

        // Write an ignored file and a non-ignored file
        write_file(dir.path(), "debug.log", "some log data\n");
        write_file(dir.path(), "new_code.rs", "fn new() {}\n");

        let diff = diff_index_to_workdir(&repo, 3).expect("diff");
        let result = build_scan_result(&repo, &diff, None).expect("scan");

        let paths: Vec<&str> = result.files.iter().map(|f| f.path.as_str()).collect();
        assert!(
            paths.contains(&"new_code.rs"),
            "non-ignored file should appear: {paths:?}"
        );
        assert!(
            !paths.contains(&"debug.log"),
            "gitignored file should NOT appear: {paths:?}"
        );
    }

    #[test]
    fn untracked_files_in_subdirectory_detected() {
        let (dir, repo) = setup_repo();
        commit_file(&repo, dir.path(), "root.rs", "fn root() {}\n", "initial");

        // Write untracked files in a subdirectory
        write_file(dir.path(), "subdir/file_a.rs", "fn a() {}\n");
        write_file(dir.path(), "subdir/file_b.rs", "fn b() {}\n");

        let diff = diff_index_to_workdir(&repo, 3).expect("diff");
        let result = build_scan_result(&repo, &diff, None).expect("scan");

        let paths: Vec<&str> = result.files.iter().map(|f| f.path.as_str()).collect();
        assert!(
            paths.contains(&"subdir/file_a.rs"),
            "subdir/file_a.rs should appear: {paths:?}"
        );
        assert!(
            paths.contains(&"subdir/file_b.rs"),
            "subdir/file_b.rs should appear: {paths:?}"
        );
    }
}
