/// Index-direct staging operations for agstage v2.
///
/// Stages changes from the working directory into the git index by constructing
/// blobs directly, without building unified diff patches. Supports file-level,
/// line-level, and hunk-level staging.
use std::collections::HashSet;
use std::fs;
use std::path::Path;

use git2::Repository;
use similar::TextDiff;

use crate::error::AgstageError;
use crate::git::repo;
use crate::git::{build_index_entry, read_head_blob};
use crate::models::{HunkInfo, LineOrigin};

/// Stage an entire file from the working directory into the index.
///
/// Reads the working-tree file content, creates a blob in the ODB, and updates
/// the index entry. Works for new files, modified files, and binary files.
///
/// # Errors
///
/// - `AgstageError::Internal` if the repository has no working directory
/// - `AgstageError::Io` if the file cannot be read from disk
/// - `AgstageError::Git` if blob creation or index update fails
pub fn stage_file(repo: &Repository, file_path: &str) -> Result<u32, AgstageError> {
    let workdir = repo::workdir(repo)?;
    let full_path = workdir.join(file_path);
    let content = fs::read(&full_path).map_err(|e| AgstageError::io(&full_path, e))?;

    let oid = repo.blob(&content)?;
    let mut index = repo.index()?;

    #[allow(clippy::cast_possible_truncation)]
    let entry = build_index_entry(&index, file_path, oid, content.len() as u32);
    index.add_frombuffer(&entry, &content)?;
    index.write()?;

    #[allow(clippy::cast_possible_truncation)]
    Ok(content.len() as u32)
}

/// Stage specific lines from the working directory into the index.
///
/// Diffs the HEAD blob against the working-tree file using `similar::TextDiff`,
/// then selectively applies only the lines whose NEW file line numbers (1-indexed)
/// appear in `selected_lines`. Unselected changes are preserved as they were in HEAD.
///
/// # Algorithm
///
/// 1. Read HEAD blob (base) and workdir file (target)
/// 2. Diff HEAD vs workdir line-by-line
/// 3. Walk all changes:
///    - **Equal**: always keep
///    - **Delete**: keep (preserve HEAD line) unless old line is selected for removal
///    - **Insert**: include only if new line number is in `selected_lines`
/// 4. Reconstruct result blob preserving trailing-newline semantics
///
/// # Errors
///
/// - `AgstageError::Git` if HEAD blob or index operations fail
/// - `AgstageError::Io` if the workdir file cannot be read
/// - `AgstageError::Internal` if the repository is bare
#[allow(clippy::implicit_hasher)]
pub fn stage_lines(
    repo: &Repository,
    file_path: &str,
    selected_lines: &HashSet<u32>,
) -> Result<u32, AgstageError> {
    let head_bytes = read_head_blob(repo, file_path)?;
    let workdir = repo::workdir(repo)?;
    let full_path = workdir.join(file_path);
    let work_bytes = fs::read(&full_path).map_err(|e| AgstageError::io(&full_path, e))?;

    let head_text = String::from_utf8_lossy(&head_bytes);
    let work_text = String::from_utf8_lossy(&work_bytes);

    let head_has_trailing_newline = head_text.ends_with('\n');
    let work_has_trailing_newline = work_text.ends_with('\n');

    let diff = TextDiff::from_lines(head_text.as_ref(), work_text.as_ref());

    let mut result_lines: Vec<&str> = Vec::new();
    let mut lines_staged: u32 = 0;

    for change in diff.iter_all_changes() {
        match change.tag() {
            similar::ChangeTag::Equal => {
                result_lines.push(change.value());
            }
            similar::ChangeTag::Delete => {
                // old_index() is 0-based; convert to 1-based
                #[allow(clippy::cast_possible_truncation)]
                let old_line = change.old_index().map_or(0, |i| i + 1) as u32;
                if !selected_lines.contains(&old_line) {
                    // Not selected: keep the HEAD line
                    result_lines.push(change.value());
                }
                // If selected: drop the HEAD line (it will be replaced by the Insert)
            }
            similar::ChangeTag::Insert => {
                // new_index() is 0-based; convert to 1-based
                #[allow(clippy::cast_possible_truncation)]
                let new_line = change.new_index().map_or(0, |i| i + 1) as u32;
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
        head_has_trailing_newline
    };

    if should_have_trailing_newline && !result.ends_with('\n') {
        result.push('\n');
    } else if !should_have_trailing_newline && result.ends_with('\n') {
        result.pop();
    }

    let content = result.into_bytes();
    let oid = repo.blob(&content)?;
    let mut index = repo.index()?;

    #[allow(clippy::cast_possible_truncation)]
    let entry = build_index_entry(&index, file_path, oid, content.len() as u32);
    index.add_frombuffer(&entry, &content)?;
    index.write()?;

    Ok(lines_staged)
}

/// Stage a single hunk by converting it to selected line numbers.
///
/// Extracts the new-file line numbers for all Addition and Context lines in the
/// hunk, then delegates to [`stage_lines`].
///
/// # Errors
///
/// Propagates all errors from [`stage_lines`].
pub fn stage_hunk(
    repo: &Repository,
    file_path: &str,
    hunk: &HunkInfo,
) -> Result<u32, AgstageError> {
    let mut selected = HashSet::new();
    for line in &hunk.lines {
        match line.origin {
            LineOrigin::Addition | LineOrigin::Context => {
                selected.insert(line.line_number);
            }
            LineOrigin::Deletion => {}
        }
    }
    stage_lines(repo, file_path, &selected)
}

/// Stage a file deletion (remove a file from the index).
///
/// The file is removed from the index but the working tree is not modified.
///
/// # Errors
///
/// - `AgstageError::Git` if the index cannot be updated
pub fn stage_deletion(repo: &Repository, file_path: &str) -> Result<(), AgstageError> {
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
/// - `AgstageError::Git` if index operations fail
/// - `AgstageError::Io` if the new file cannot be read from disk
/// - `AgstageError::Internal` if the repository is bare
pub fn stage_rename(repo: &Repository, old_path: &str, new_path: &str) -> Result<(), AgstageError> {
    let mut index = repo.index()?;
    index.remove_path(Path::new(old_path))?;

    let workdir = repo::workdir(repo)?;
    let full_path = workdir.join(new_path);
    let content = fs::read(&full_path).map_err(|e| AgstageError::io(&full_path, e))?;

    let oid = repo.blob(&content)?;

    #[allow(clippy::cast_possible_truncation)]
    let entry = build_index_entry(&index, new_path, oid, content.len() as u32);
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

        let bytes = stage_file(&repo, "file.txt").expect("stage_file");

        assert_eq!(bytes, modified.len() as u32);
        let staged = read_index_content(&repo, "file.txt");
        assert_eq!(staged, modified.as_bytes());
    }

    #[test]
    fn stage_file_new_file_adds_to_index() {
        let (dir, repo) = setup_repo_with_commit(&[("existing.txt", "hi\n")]);

        // Create a brand new file
        let new_content = "brand new file\n";
        fs::write(dir.path().join("new_file.txt"), new_content).expect("write");

        let bytes = stage_file(&repo, "new_file.txt").expect("stage_file");

        assert_eq!(bytes, new_content.len() as u32);
        let staged = read_index_content(&repo, "new_file.txt");
        assert_eq!(staged, new_content.as_bytes());
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

        stage_rename(&repo, "old_name.rs", "new_name.rs").expect("stage_rename");

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
