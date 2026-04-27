/// Index-direct staging operations for pgs.
///
/// Stages changes from the working directory into the git index by constructing
/// blobs directly, without building unified diff patches. Supports file-level,
/// line-level, and hunk-level staging.
use std::collections::HashSet;
use std::fs;
use std::path::Path;

use git2::Repository;
use similar::TextDiff;

use crate::error::PgsError;
use crate::git::repo;
use crate::git::{
    WorkdirFileType, build_index_entry, read_head_blob, read_index_blob, read_workdir_for_blob,
};
use crate::models::{
    HunkInfo, LineOrigin, LineRange, OperationPreview, PreviewLine, ResolvedSelection, ScanResult,
};
use crate::saturating_u32;

/// Stage an entire file from the working directory into the index.
///
/// Reads the working-tree file content, creates a blob in the ODB, and updates
/// the index entry. Works for new files, modified files, and binary files.
///
/// # Errors
///
/// - `PgsError::Internal` if the repository has no working directory
/// - `PgsError::Io` if the file cannot be read from disk
/// - `PgsError::Git` if blob creation or index update fails
pub fn stage_file(
    repo: &Repository,
    file_path: &str,
    mode_override: Option<u32>,
) -> Result<u32, PgsError> {
    let workdir = repo::workdir(repo)?;
    let blob = read_workdir_for_blob(workdir, file_path)?;
    let content = blob.bytes;
    let effective_mode = match mode_override {
        Some(m) => Some(m),
        None if blob.file_type == WorkdirFileType::Symlink => Some(0o120_000),
        None => None,
    };

    let oid = repo.blob(&content)?;
    let mut index = repo.index()?;

    let entry = build_index_entry(
        &index,
        file_path,
        oid,
        saturating_u32(content.len()),
        effective_mode,
    );
    index.add_frombuffer(&entry, &content)?;
    index.write()?;

    Ok(saturating_u32(content.len()))
}

/// Stage specific lines from the working directory into the index.
///
/// Diffs the current index blob (falling back to HEAD) against the working-tree
/// file, then selectively applies only the lines whose line numbers (1-indexed)
/// appear in `selected_lines`. Unselected changes are preserved as-is.
///
/// # Errors
///
/// - `PgsError::Git` if index/HEAD blob or index operations fail
/// - `PgsError::Io` if the workdir file cannot be read
/// - `PgsError::Internal` if the repository is bare
#[allow(clippy::implicit_hasher)]
pub fn stage_lines(
    repo: &Repository,
    file_path: &str,
    selected_lines: &HashSet<u32>,
) -> Result<u32, PgsError> {
    let base_bytes =
        read_index_blob(repo, file_path).or_else(|_| read_head_blob(repo, file_path))?;
    let workdir = repo::workdir(repo)?;
    let full_path = workdir.join(file_path);
    let work_bytes = fs::read(&full_path).map_err(|e| PgsError::io(&full_path, e))?;

    let base_text = String::from_utf8_lossy(&base_bytes);
    let work_text = String::from_utf8_lossy(&work_bytes);

    let base_has_trailing_newline = base_text.ends_with('\n');
    let work_has_trailing_newline = work_text.ends_with('\n');

    let diff = TextDiff::from_lines(base_text.as_ref(), work_text.as_ref());

    let mut result_lines: Vec<&str> = Vec::new();
    let mut lines_staged: u32 = 0;

    for change in diff.iter_all_changes() {
        match change.tag() {
            similar::ChangeTag::Equal => {
                result_lines.push(change.value());
            }
            similar::ChangeTag::Delete => {
                // old_index() is 0-based; convert to 1-based
                let old_line = change.old_index().map_or(0, |i| saturating_u32(i + 1));
                if !selected_lines.contains(&old_line) {
                    // Not selected: keep the HEAD line
                    result_lines.push(change.value());
                }
                // If selected: drop the HEAD line (it will be replaced by the Insert)
            }
            similar::ChangeTag::Insert => {
                // new_index() is 0-based; convert to 1-based
                let new_line = change.new_index().map_or(0, |i| saturating_u32(i + 1));
                if selected_lines.contains(&new_line) {
                    result_lines.push(change.value());
                    lines_staged += 1;
                }
                // If not selected: don't stage this addition
            }
        }
    }

    // Reconstruct content: join lines (each already has its own newline from the diff)
    let mut result = result_lines.concat();

    // Trailing newline preservation:
    // - If workdir has trailing newline and we staged something, result should end with newline
    // - If neither had trailing newline, result should not end with newline
    // - Preserve the "expected" trailing newline state:
    //   If no lines were staged, result should match HEAD's trailing newline.
    //   If lines were staged, match the mix: if any selected lines came from workdir
    //   (which has trailing newline), preserve that.
    let should_have_trailing_newline = if lines_staged > 0 {
        work_has_trailing_newline
    } else {
        base_has_trailing_newline
    };

    if should_have_trailing_newline && !result.ends_with('\n') {
        result.push('\n');
    } else if !should_have_trailing_newline && result.ends_with('\n') {
        result.pop();
    }

    let content = result.into_bytes();
    let oid = repo.blob(&content)?;
    let mut index = repo.index()?;

    let entry = build_index_entry(&index, file_path, oid, saturating_u32(content.len()), None);
    index.add_frombuffer(&entry, &content)?;
    index.write()?;

    Ok(lines_staged)
}

/// Stage a single hunk by converting it to selected line numbers.
///
/// Extracts line numbers for all lines in the hunk:
/// - Addition/Context: new-file line numbers (for Insert gating in `stage_lines`)
/// - Deletion: old-file line numbers (for Delete suppression in `stage_lines`)
///
/// Then delegates to [`stage_lines`].
///
/// # Errors
///
/// Propagates all errors from [`stage_lines`].
pub fn stage_hunk(repo: &Repository, file_path: &str, hunk: &HunkInfo) -> Result<u32, PgsError> {
    let mut selected = HashSet::new();
    for line in &hunk.lines {
        match line.origin {
            LineOrigin::Addition | LineOrigin::Context | LineOrigin::Deletion => {
                selected.insert(line.line_number);
            }
            // `Mixed` tags split-hunk classification ranges; it never appears in
            // a real `DiffLineInfo`. Skip defensively.
            LineOrigin::Mixed => {}
        }
    }
    stage_lines(repo, file_path, &selected)
}

/// Inputs for [`preview_stage`] — bundled to keep the function under four params.
pub struct PreviewRequest<'a> {
    /// Scan result the selection was resolved against.
    pub scan: &'a ScanResult,
    /// Resolved selection for the file being previewed.
    pub resolved: &'a ResolvedSelection,
    /// Original selection string (e.g. `src/main.rs:10-20`) for display.
    pub selection: &'a str,
    /// Per-file cap on preview lines. `0` means unlimited.
    pub limit: u32,
}

/// Build an [`OperationPreview`] for one resolved file without mutating anything.
///
/// Reuses the same `TextDiff` resolution path as [`stage_lines`] so the preview
/// matches what would land in the index — then stops before the blob write.
/// Binary files short-circuit with an empty preview and `reason: "binary"`.
///
/// # Errors
///
/// - `PgsError::Git` if the index/HEAD blob cannot be read
/// - `PgsError::Io` if the workdir file cannot be read
/// - `PgsError::Internal` if the repository is bare
pub fn preview_stage(
    repo: &Repository,
    request: &PreviewRequest<'_>,
) -> Result<OperationPreview, PgsError> {
    let PreviewRequest {
        scan,
        resolved,
        selection,
        limit,
    } = *request;

    let file_path = resolved.file_path.clone();
    let resolved_ranges = resolved_ranges_for(scan, resolved);

    let file_info = scan.files.iter().find(|f| f.path == file_path);
    if file_info.is_some_and(|f| f.is_binary) {
        return Ok(OperationPreview {
            selection: selection.to_owned(),
            file_path,
            resolved_ranges,
            preview_lines: Vec::new(),
            truncated: false,
            limit_applied: limit,
            reason: Some("binary".to_owned()),
        });
    }

    let selected = selected_line_numbers(scan, resolved);
    let additions = collect_preview_additions(repo, &file_path, &selected)?;

    let (capped, truncated) = apply_limit(additions, limit);

    Ok(OperationPreview {
        selection: selection.to_owned(),
        file_path,
        resolved_ranges,
        preview_lines: capped,
        truncated,
        limit_applied: limit,
        reason: None,
    })
}

fn resolved_ranges_for(scan: &ScanResult, resolved: &ResolvedSelection) -> Vec<LineRange> {
    if let Some(ranges) = &resolved.line_ranges {
        return ranges.clone();
    }

    let Some(file) = scan.files.iter().find(|f| f.path == resolved.file_path) else {
        return Vec::new();
    };

    resolved
        .hunk_indices
        .iter()
        .filter_map(|&idx| file.hunks.get(idx))
        .filter(|hunk| hunk.new_lines > 0)
        .map(|hunk| LineRange {
            start: hunk.new_start,
            end: hunk.new_start + hunk.new_lines.saturating_sub(1),
        })
        .collect()
}

fn selected_line_numbers(scan: &ScanResult, resolved: &ResolvedSelection) -> HashSet<u32> {
    let mut selected: HashSet<u32> = HashSet::new();

    if let Some(ranges) = &resolved.line_ranges {
        for range in ranges {
            for line in range.start..=range.end {
                selected.insert(line);
            }
        }
        return selected;
    }

    let Some(file) = scan.files.iter().find(|f| f.path == resolved.file_path) else {
        return selected;
    };

    if resolved.hunk_indices.is_empty() {
        // Whole-file selection — pull every addition line from every hunk.
        for hunk in &file.hunks {
            for line in &hunk.lines {
                if matches!(line.origin, LineOrigin::Addition) {
                    selected.insert(line.line_number);
                }
            }
        }
        return selected;
    }

    for &hunk_idx in &resolved.hunk_indices {
        let Some(hunk) = file.hunks.get(hunk_idx) else {
            continue;
        };
        for line in &hunk.lines {
            if matches!(line.origin, LineOrigin::Addition) {
                selected.insert(line.line_number);
            }
        }
    }

    selected
}

fn collect_preview_additions(
    repo: &Repository,
    file_path: &str,
    selected: &HashSet<u32>,
) -> Result<Vec<PreviewLine>, PgsError> {
    let base_bytes =
        read_index_blob(repo, file_path).or_else(|_| read_head_blob(repo, file_path))?;
    let workdir = repo::workdir(repo)?;
    let full_path = workdir.join(file_path);
    let work_bytes = fs::read(&full_path).map_err(|e| PgsError::io(&full_path, e))?;

    let base_text = String::from_utf8_lossy(&base_bytes);
    let work_text = String::from_utf8_lossy(&work_bytes);
    let diff = TextDiff::from_lines(base_text.as_ref(), work_text.as_ref());

    let mut out: Vec<PreviewLine> = Vec::new();
    for change in diff.iter_all_changes() {
        if change.tag() != similar::ChangeTag::Insert {
            continue;
        }
        let new_line = change.new_index().map_or(0, |i| saturating_u32(i + 1));
        if !selected.contains(&new_line) {
            continue;
        }
        let raw = change.value();
        let content = raw.strip_suffix('\n').unwrap_or(raw).to_owned();
        out.push(PreviewLine {
            line_number: new_line,
            origin: LineOrigin::Addition,
            content,
        });
    }
    Ok(out)
}

fn apply_limit(mut lines: Vec<PreviewLine>, limit: u32) -> (Vec<PreviewLine>, bool) {
    if limit == 0 {
        return (lines, false);
    }
    let cap = limit as usize;
    if lines.len() > cap {
        lines.truncate(cap);
        (lines, true)
    } else {
        (lines, false)
    }
}

/// Stage a file deletion (remove a file from the index).
///
/// The file is removed from the index but the working tree is not modified.
///
/// # Errors
///
/// - `PgsError::Git` if the index cannot be updated
pub fn stage_deletion(repo: &Repository, file_path: &str) -> Result<(), PgsError> {
    let mut index = repo.index()?;
    index.remove_path(Path::new(file_path))?;
    index.write()?;
    Ok(())
}

/// Stage a renamed file: remove the old path and add the new path.
///
/// Removes the old path from the index, reads the new file from the working
/// directory, creates a blob, and adds the new entry to the index.
///
/// # Errors
///
/// - `PgsError::Git` if index operations fail
/// - `PgsError::Io` if the new file cannot be read from disk
/// - `PgsError::Internal` if the repository is bare
pub fn stage_rename(
    repo: &Repository,
    old_path: &str,
    new_path: &str,
    mode_override: Option<u32>,
) -> Result<(), PgsError> {
    let mut index = repo.index()?;
    index.remove_path(Path::new(old_path))?;

    let workdir = repo::workdir(repo)?;
    let blob = read_workdir_for_blob(workdir, new_path)?;
    let content = blob.bytes;
    let effective_mode = match mode_override {
        Some(m) => Some(m),
        None if blob.file_type == WorkdirFileType::Symlink => Some(0o120_000),
        None => None,
    };

    let oid = repo.blob(&content)?;

    let entry = build_index_entry(
        &index,
        new_path,
        oid,
        saturating_u32(content.len()),
        effective_mode,
    );
    index.add_frombuffer(&entry, &content)?;
    index.write()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use git2::Repository;
    use tempfile::TempDir;

    use super::*;

    /// Create a repo with an initial commit containing the specified files.
    fn setup_repo_with_commit(files: &[(&str, &str)]) -> (TempDir, Repository) {
        let dir = TempDir::new().expect("tempdir");
        let repo = Repository::init(dir.path()).expect("init");
        {
            let mut config = repo.config().expect("config");
            config.set_str("user.name", "Test").expect("set name");
            config
                .set_str("user.email", "test@test.com")
                .expect("set email");
        }

        {
            let mut index = repo.index().expect("index");

            for &(path, content) in files {
                let full = dir.path().join(path);
                if let Some(parent) = full.parent() {
                    fs::create_dir_all(parent).expect("create parent dirs");
                }
                fs::write(&full, content).expect("write file");
                index.add_path(Path::new(path)).expect("add path");
            }

            index.write().expect("write index");
            let tree_oid = index.write_tree().expect("write tree");
            {
                let tree = repo.find_tree(tree_oid).expect("find tree");
                let sig = repo.signature().expect("sig");
                repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
                    .expect("commit");
            }
        }

        (dir, repo)
    }

    /// Read the blob content for a file from the current index.
    fn read_index_content(repo: &Repository, path: &str) -> Vec<u8> {
        let index = repo.index().expect("index");
        let entry = index.get_path(Path::new(path), 0).expect("entry in index");
        let blob = repo.find_blob(entry.id).expect("find blob");
        blob.content().to_vec()
    }

    #[test]
    fn stage_file_stages_entire_workdir_content() {
        let (dir, repo) = setup_repo_with_commit(&[("file.txt", "original\n")]);

        // Modify in workdir
        let modified = "original\nappended line\n";
        fs::write(dir.path().join("file.txt"), modified).expect("write");

        let bytes = stage_file(&repo, "file.txt", None).expect("stage_file");

        assert_eq!(
            bytes,
            u32::try_from(modified.len()).expect("content fits u32")
        );
        let staged = read_index_content(&repo, "file.txt");
        assert_eq!(staged, modified.as_bytes());
    }

    #[test]
    fn stage_file_new_file_adds_to_index() {
        let (dir, repo) = setup_repo_with_commit(&[("existing.txt", "hi\n")]);

        // Create a brand new file
        let new_content = "brand new file\n";
        fs::write(dir.path().join("new_file.txt"), new_content).expect("write");

        let bytes = stage_file(&repo, "new_file.txt", None).expect("stage_file");

        assert_eq!(
            bytes,
            u32::try_from(new_content.len()).expect("content fits u32")
        );
        let staged = read_index_content(&repo, "new_file.txt");
        assert_eq!(staged, new_content.as_bytes());
    }

    /// Symlink blobs must store the link-target string, not the referent's bytes.
    #[cfg(unix)]
    #[test]
    fn stage_file_symlink_writes_link_target_string_not_target_bytes() {
        use std::os::unix::fs::symlink;

        let (dir, repo) = setup_repo_with_commit(&[]);

        // Create a target file with 2048 distinguishable bytes.
        let target_content = vec![0xAB_u8; 2048];
        fs::write(dir.path().join("target.bin"), &target_content).expect("write target");

        // Create a symlink whose stored string is "target.bin" (10 bytes).
        symlink("target.bin", dir.path().join("link_to_target")).expect("symlink");

        // Stage the symlink with the symlink mode.
        let _lines = stage_file(&repo, "link_to_target", Some(0o120_000)).expect("stage_file");

        // The blob must equal the link-target string, not the target file's bytes.
        let staged = read_index_content(&repo, "link_to_target");
        assert_eq!(
            staged,
            b"target.bin",
            "symlink blob must equal link target string, got {} bytes",
            staged.len()
        );

        // The index entry mode must be the symlink mode regardless of the blob content.
        let index = repo.index().expect("index");
        let entry = index
            .get_path(Path::new("link_to_target"), 0)
            .expect("entry in index");
        assert_eq!(
            entry.mode, 0o120_000,
            "index entry mode must be 0o120000 (symlink)"
        );
    }

    #[test]
    fn stage_lines_selects_subset() {
        let original = "line1\nline2\nline3\n";
        let modified = "line1\nMODIFIED\nline3\nnew line4\n";
        let (dir, repo) = setup_repo_with_commit(&[("file.txt", original)]);

        fs::write(dir.path().join("file.txt"), modified).expect("write");

        // Select only line 2 (the MODIFIED replacement). Don't select line 4 (new line4).
        let mut selected = HashSet::new();
        selected.insert(2); // line 2 in the new file = "MODIFIED"

        let count = stage_lines(&repo, "file.txt", &selected).expect("stage_lines");

        assert_eq!(count, 1); // only 1 line staged (the "MODIFIED" insertion)
        let staged = read_index_content(&repo, "file.txt");
        let staged_text = String::from_utf8(staged).expect("utf8");
        // Should have: line1, MODIFIED, line3 (no "new line4" since line 4 not selected)
        assert_eq!(staged_text, "line1\nMODIFIED\nline3\n");
    }

    #[test]
    fn stage_lines_preserves_trailing_newline() {
        let original = "line1\nline2\n";
        let modified = "line1\nchanged\n";
        let (dir, repo) = setup_repo_with_commit(&[("file.txt", original)]);

        fs::write(dir.path().join("file.txt"), modified).expect("write");

        let mut selected = HashSet::new();
        selected.insert(2); // select "changed"

        stage_lines(&repo, "file.txt", &selected).expect("stage_lines");

        let staged = read_index_content(&repo, "file.txt");
        let staged_text = String::from_utf8(staged).expect("utf8");
        assert!(
            staged_text.ends_with('\n'),
            "staged content should end with newline, got: {staged_text:?}"
        );
        assert_eq!(staged_text, "line1\nchanged\n");
    }

    #[test]
    fn stage_lines_preserves_no_trailing_newline() {
        let original = "line1\nline2";
        let modified = "line1\nchanged";
        let (dir, repo) = setup_repo_with_commit(&[("file.txt", original)]);

        fs::write(dir.path().join("file.txt"), modified).expect("write");

        let mut selected = HashSet::new();
        selected.insert(2); // select "changed"

        stage_lines(&repo, "file.txt", &selected).expect("stage_lines");

        let staged = read_index_content(&repo, "file.txt");
        let staged_text = String::from_utf8(staged).expect("utf8");
        assert!(
            !staged_text.ends_with('\n'),
            "staged content should NOT end with newline, got: {staged_text:?}"
        );
        assert_eq!(staged_text, "line1\nchanged");
    }

    #[test]
    fn stage_deletion_removes_from_index() {
        let (dir, repo) = setup_repo_with_commit(&[("doomed.txt", "goodbye\n")]);

        // File still exists on disk (unstaged deletion means workdir is unchanged,
        // but we just want to remove from index).
        // Verify it's in the index first.
        let index = repo.index().expect("index");
        assert!(
            index.get_path(Path::new("doomed.txt"), 0).is_some(),
            "file should be in index before deletion"
        );
        drop(index);

        stage_deletion(&repo, "doomed.txt").expect("stage_deletion");

        // Verify removed from index
        let index = repo.index().expect("index");
        assert!(
            index.get_path(Path::new("doomed.txt"), 0).is_none(),
            "file should not be in index after deletion"
        );

        // File should still exist on disk
        assert!(
            dir.path().join("doomed.txt").exists(),
            "workdir file should still exist"
        );
    }

    #[test]
    fn stage_rename_removes_old_adds_new() {
        let (dir, repo) = setup_repo_with_commit(&[("old_name.rs", "fn old() {}\n")]);

        // Simulate rename in working directory
        let new_content = "fn renamed() {}\n";
        fs::write(dir.path().join("new_name.rs"), new_content).expect("write new");

        stage_rename(&repo, "old_name.rs", "new_name.rs", None).expect("stage_rename");

        let index = repo.index().expect("index");
        assert!(
            index.get_path(Path::new("old_name.rs"), 0).is_none(),
            "old path should be removed from index"
        );
        assert!(
            index.get_path(Path::new("new_name.rs"), 0).is_some(),
            "new path should be in index"
        );
        drop(index);

        let staged = read_index_content(&repo, "new_name.rs");
        assert_eq!(staged, new_content.as_bytes());
    }

    #[test]
    fn stage_hunk_with_pure_deletion_removes_line() {
        // stage_hunk ignores Deletion-origin lines when building selected_lines.
        // Deleting the last line exposes this: old_lineno=5 is never in the context
        // new_linenos {2,3,4}, so stage_lines keeps the HEAD line unchanged.
        let head = "line1\nline2\nline3\nline4\nline5\n";
        let workdir = "line1\nline2\nline3\nline4\n"; // line5 (last) deleted
        let (dir, repo) = setup_repo_with_commit(&[("file.txt", head)]);

        fs::write(dir.path().join("file.txt"), workdir).expect("write");

        // Scan the actual diff to get real hunk metadata from the diff engine
        let diff = crate::git::diff::diff_index_to_workdir(&repo, 3).expect("diff");
        let scan = crate::git::diff::build_scan_result(&repo, &diff, None).expect("scan");

        assert_eq!(scan.files.len(), 1, "expected 1 changed file");
        assert!(!scan.files[0].hunks.is_empty(), "expected at least 1 hunk");

        let hunk = &scan.files[0].hunks[0];
        stage_hunk(&repo, "file.txt", hunk).expect("stage_hunk");

        let staged = read_index_content(&repo, "file.txt");
        let staged_text = String::from_utf8(staged).expect("utf8");
        // line5 should be gone from the index
        assert_eq!(
            staged_text, workdir,
            "index should match workdir after staging deletion hunk, got: {staged_text:?}"
        );
    }

    #[test]
    fn stage_hunk_with_deletion_in_substitution_applies_both() {
        // bbb replaced with BBB — this is a delete+insert in one hunk.
        // Checks that stage_hunk correctly handles the substitution.
        let head = "aaa\nbbb\nccc\n";
        let workdir = "aaa\nBBB\nccc\n";
        let (dir, repo) = setup_repo_with_commit(&[("file.txt", head)]);

        fs::write(dir.path().join("file.txt"), workdir).expect("write");

        let diff = crate::git::diff::diff_index_to_workdir(&repo, 3).expect("diff");
        let scan = crate::git::diff::build_scan_result(&repo, &diff, None).expect("scan");

        assert_eq!(scan.files.len(), 1, "expected 1 changed file");
        assert!(!scan.files[0].hunks.is_empty(), "expected at least 1 hunk");

        let hunk = &scan.files[0].hunks[0];
        stage_hunk(&repo, "file.txt", hunk).expect("stage_hunk");

        let staged = read_index_content(&repo, "file.txt");
        let staged_text = String::from_utf8(staged).expect("utf8");
        assert_eq!(
            staged_text, workdir,
            "index should match workdir after staging substitution hunk, got: {staged_text:?}"
        );
    }

    #[test]
    fn stage_lines_sequential_without_commit_preserves_both() {
        // Reproduce bug: second stage_lines reads HEAD (not updated index),
        // so it overwrites the first call's result.
        let head: String = (1..=30).fold(String::new(), |mut s, i| {
            use std::fmt::Write;
            let _ = writeln!(s, "line {i}");
            s
        });
        // Modify line 1 and line 30 — far apart enough to produce 2 separate hunks
        let workdir = {
            let mut s = head.clone();
            s = s.replacen("line 1\n", "CHANGED 1\n", 1);
            s = s.replacen("line 30\n", "CHANGED 30\n", 1);
            s
        };
        let (dir, repo) = setup_repo_with_commit(&[("multi.txt", &head)]);
        fs::write(dir.path().join("multi.txt"), &workdir).expect("write");

        let diff = crate::git::diff::diff_index_to_workdir(&repo, 3).expect("diff");
        let scan = crate::git::diff::build_scan_result(&repo, &diff, None).expect("scan");

        let file = &scan.files[0];
        assert!(
            file.hunks.len() >= 2,
            "expected at least 2 hunks, got {}",
            file.hunks.len()
        );

        // Stage hunk 0 lines (Addition + Context + Deletion)
        let mut selected0 = HashSet::new();
        for line in &file.hunks[0].lines {
            selected0.insert(line.line_number);
        }
        stage_lines(&repo, "multi.txt", &selected0).expect("stage_lines hunk0");

        // Stage hunk 1 lines (Addition + Context + Deletion)
        let mut selected1 = HashSet::new();
        for line in &file.hunks[1].lines {
            selected1.insert(line.line_number);
        }
        stage_lines(&repo, "multi.txt", &selected1).expect("stage_lines hunk1");

        let staged = read_index_content(&repo, "multi.txt");
        let staged_text = String::from_utf8(staged).expect("utf8");

        assert!(
            staged_text.contains("CHANGED 1"),
            "first hunk change should survive second stage_lines call; got: {staged_text:?}"
        );
        assert!(
            staged_text.contains("CHANGED 30"),
            "second hunk change should be staged; got: {staged_text:?}"
        );
    }

    #[test]
    fn stage_hunk_multi_hunk_sequential_with_commits_no_phantom() {
        // Bug: stage_hunk ignores Deletion-origin lines. When one of the two hunks is
        // a pure end-of-file deletion (old_lineno not in context new_linenos), staging
        // it leaves the index unchanged. The commit captures nothing for that hunk,
        // and the final scan still reports a file — a phantom hunk.
        let head: String = (1..=30).fold(String::new(), |mut s, i| {
            use std::fmt::Write;
            let _ = writeln!(s, "line {i}");
            s
        });
        let workdir = {
            let mut s = head.clone();
            // Hunk 0: substitution at line 1 (works fine — Addition covers it)
            s = s.replacen("line 1\n", "CHANGED 1\n", 1);
            // Hunk 1: pure deletion of line 30 (last) — exposes the bug
            s = s.replacen("line 30\n", "", 1);
            s
        };
        let (dir, repo) = setup_repo_with_commit(&[("multi.txt", &head)]);
        fs::write(dir.path().join("multi.txt"), &workdir).expect("write");

        // First scan → 2 hunks
        let diff = crate::git::diff::diff_index_to_workdir(&repo, 3).expect("diff");
        let scan = crate::git::diff::build_scan_result(&repo, &diff, None).expect("scan");
        assert!(
            scan.files[0].hunks.len() >= 2,
            "expected 2 hunks initially, got {}",
            scan.files[0].hunks.len()
        );

        // Stage hunk[0] (substitution) and commit
        stage_hunk(&repo, "multi.txt", &scan.files[0].hunks[0]).expect("stage hunk0");
        {
            let tree_oid = repo.index().unwrap().write_tree().unwrap();
            let tree = repo.find_tree(tree_oid).unwrap();
            let sig = repo.signature().unwrap();
            let parent = repo.head().unwrap().peel_to_commit().unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "stage hunk0", &tree, &[&parent])
                .unwrap();
        }

        // Re-scan — should have exactly 1 hunk remaining (the deletion)
        let diff2 = crate::git::diff::diff_index_to_workdir(&repo, 3).expect("diff2");
        let scan2 = crate::git::diff::build_scan_result(&repo, &diff2, None).expect("scan2");
        assert_eq!(
            scan2.files.len(),
            1,
            "expected 1 file with remaining deletion hunk after first commit"
        );

        // Stage the remaining pure-deletion hunk and commit
        stage_hunk(&repo, "multi.txt", &scan2.files[0].hunks[0]).expect("stage deletion hunk");
        {
            let tree_oid = repo.index().unwrap().write_tree().unwrap();
            let tree = repo.find_tree(tree_oid).unwrap();
            let sig = repo.signature().unwrap();
            let parent = repo.head().unwrap().peel_to_commit().unwrap();
            repo.commit(
                Some("HEAD"),
                &sig,
                &sig,
                "stage deletion hunk",
                &tree,
                &[&parent],
            )
            .unwrap();
        }

        // Final scan — should be empty; bug causes phantom hunk to remain
        let diff3 = crate::git::diff::diff_index_to_workdir(&repo, 3).expect("diff3");
        let scan3 = crate::git::diff::build_scan_result(&repo, &diff3, None).expect("scan3");
        assert_eq!(
            scan3.files.len(),
            0,
            "expected 0 files after staging all hunks and committing, got {:?}",
            scan3.files.iter().map(|f| &f.path).collect::<Vec<_>>()
        );
    }

    #[test]
    fn stage_hunk_delegates_to_stage_lines() {
        let original = "aaa\nbbb\nccc\nddd\n";
        let modified = "aaa\nBBB\nccc\nDDD\n";
        let (dir, repo) = setup_repo_with_commit(&[("file.txt", original)]);

        fs::write(dir.path().join("file.txt"), modified).expect("write");

        // Create a hunk that covers only lines 1-3 (aaa, BBB, ccc) in the new file.
        // Line 2 is the only Addition; lines 1 and 3 are Context.
        let hunk = HunkInfo {
            hunk_id: "test_hunk".into(),
            old_start: 1,
            old_lines: 3,
            new_start: 1,
            new_lines: 3,
            header: "@@ -1,3 +1,3 @@".into(),
            lines: vec![
                crate::models::DiffLineInfo {
                    line_number: 1,
                    origin: LineOrigin::Context,
                    content: "aaa".into(),
                },
                crate::models::DiffLineInfo {
                    line_number: 2,
                    origin: LineOrigin::Deletion,
                    content: "bbb".into(),
                },
                crate::models::DiffLineInfo {
                    line_number: 2,
                    origin: LineOrigin::Addition,
                    content: "BBB".into(),
                },
                crate::models::DiffLineInfo {
                    line_number: 3,
                    origin: LineOrigin::Context,
                    content: "ccc".into(),
                },
            ],
            checksum: "test".into(),
            whitespace_only: false,
        };

        let count = stage_hunk(&repo, "file.txt", &hunk).expect("stage_hunk");

        // Should stage line 2 (BBB) but NOT line 4 (DDD) since it's outside the hunk
        assert!(count > 0, "should have staged at least one line");
        let staged = read_index_content(&repo, "file.txt");
        let staged_text = String::from_utf8(staged).expect("utf8");
        // BBB should be staged, DDD should not (stays as ddd from HEAD)
        assert!(
            staged_text.contains("BBB"),
            "BBB should be staged, got: {staged_text:?}"
        );
        assert!(
            staged_text.contains("ddd"),
            "ddd should remain (DDD not selected), got: {staged_text:?}"
        );
        assert_eq!(staged_text, "aaa\nBBB\nccc\nddd\n");
    }
}
