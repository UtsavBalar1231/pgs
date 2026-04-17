/// Unstaging operations: restore the git index toward HEAD.
///
/// Unlike staging (which moves workdir changes into the index), unstaging
/// reverses direction — it moves the index back toward HEAD. The diff base
/// is HEAD vs index (current staged content), and selected lines are
/// reverted to their HEAD state.
use std::collections::HashSet;

use git2::Repository;
use similar::TextDiff;

use crate::error::PgsError;
use crate::git::{build_index_entry, read_head_blob, read_head_mode, read_index_blob};
use crate::models::{HunkInfo, LineOrigin};
use crate::saturating_u32;

/// Unstage an entire file — restore the index entry to match HEAD.
///
/// Handles three cases:
/// - **Modified file** (exists in HEAD and index): replaces the index entry
///   with the HEAD blob content.
/// - **Added file** (in index but not in HEAD): removes the entry from the
///   index entirely.
/// - **Deleted file** (in HEAD but staged for deletion): restores the HEAD
///   blob into the index.
///
/// Returns the number of lines affected.
///
/// # Errors
///
/// Returns `PgsError::Git` if libgit2 operations fail, or
/// `PgsError::StagingFailed` if the index write fails.
pub fn unstage_file(repo: &Repository, file_path: &str) -> Result<u32, PgsError> {
    let head_result = read_head_blob(repo, file_path);
    let mut index = repo.index()?;
    let in_index = index.get_path(std::path::Path::new(file_path), 0).is_some();

    match (head_result, in_index) {
        // Case (a): Modified file — exists in HEAD and index
        // Case (c): Deleted file — exists in HEAD, staged for deletion (not in index)
        (Ok(head_content), _) => {
            let lines_affected = count_lines(&head_content);
            let oid = repo.blob(&head_content)?;
            let head_mode = read_head_mode(repo, file_path).ok();
            let entry = build_index_entry(
                &index,
                file_path,
                oid,
                saturating_u32(head_content.len()),
                head_mode,
            );
            index.add(&entry)?;
            index.write()?;
            Ok(lines_affected)
        }
        // Case (b): Added file — not in HEAD, remove from index
        (Err(_), true) => {
            let index_content = read_index_blob(repo, file_path)?;
            let lines_affected = count_lines(&index_content);
            index.remove_path(std::path::Path::new(file_path))?;
            index.write()?;
            Ok(lines_affected)
        }
        // File not in HEAD and not in index — nothing to unstage
        (Err(e), false) => Err(e),
    }
}

/// Unstage specific lines — partially revert the index toward HEAD.
///
/// Reads HEAD blob content (the target state) and index blob content
/// (the current staged state), then uses `similar::TextDiff` to compute
/// the diff. For each change:
/// - **Equal** lines: kept as-is.
/// - **Delete** lines (in HEAD, removed from index): if the line's old
///   number is in `selected_lines`, restore it (unstage the deletion).
/// - **Insert** lines (added in index): if the line's new number is in
///   `selected_lines`, drop it (unstage the addition).
///
/// `selected_lines` contains 1-indexed line numbers from the HEAD-to-index
/// diff. For deletions, use the old (HEAD) line number. For insertions,
/// use the new (index) line number.
///
/// Returns the number of lines affected by the unstage operation.
///
/// # Errors
///
/// Returns `PgsError::Git` if blob reads or index writes fail, or
/// `PgsError::FileNotInDiff` if the file is not in the index.
pub fn unstage_lines<S: ::std::hash::BuildHasher>(
    repo: &Repository,
    file_path: &str,
    selected_lines: &HashSet<u32, S>,
) -> Result<u32, PgsError> {
    let head_content = read_head_blob(repo, file_path)?;
    let index_content = read_index_blob(repo, file_path)?;

    let head_text = String::from_utf8_lossy(&head_content);
    let index_text = String::from_utf8_lossy(&index_content);

    let trailing_newline = index_text.ends_with('\n');

    let diff = TextDiff::from_lines(head_text.as_ref(), index_text.as_ref());

    let mut result_lines: Vec<&str> = Vec::new();
    let mut lines_affected: u32 = 0;

    for change in diff.iter_all_changes() {
        match change.tag() {
            similar::ChangeTag::Equal => {
                result_lines.push(change.value());
            }
            similar::ChangeTag::Delete => {
                // Line exists in HEAD but was deleted in index (staged deletion).
                // old_index() gives the 0-based index in the old (HEAD) text.
                let old_line = change
                    .old_index()
                    .map_or(0, |i| u32::try_from(i).unwrap_or(u32::MAX) + 1);
                if selected_lines.contains(&old_line) {
                    // Restore from HEAD — unstage the deletion
                    result_lines.push(change.value());
                    lines_affected += 1;
                }
                // else: keep it deleted (don't unstage)
            }
            similar::ChangeTag::Insert => {
                // Line was added in the index (staged addition).
                // new_index() gives the 0-based index in the new (index) text.
                let new_line = change
                    .new_index()
                    .map_or(0, |i| u32::try_from(i).unwrap_or(u32::MAX) + 1);
                if selected_lines.contains(&new_line) {
                    // Drop it — unstage the addition (revert to HEAD)
                    lines_affected += 1;
                } else {
                    // Keep the staged addition
                    result_lines.push(change.value());
                }
            }
        }
    }

    let mut new_content = result_lines.join("");

    // Preserve trailing newline semantics from the original index content:
    // if the original had a trailing newline, ensure we do too; if not, strip it.
    if trailing_newline && !new_content.ends_with('\n') {
        new_content.push('\n');
    } else if !trailing_newline && new_content.ends_with('\n') {
        new_content.pop();
    }

    // Write the new blob and update the index
    let oid = repo.blob(new_content.as_bytes())?;
    let mut index = repo.index()?;
    let entry = build_index_entry(
        &index,
        file_path,
        oid,
        saturating_u32(new_content.len()),
        None,
    );
    index.add(&entry)?;
    index.write()?;

    Ok(lines_affected)
}

/// Unstage a single hunk by converting it to selected line numbers.
///
/// Extracts line numbers from the hunk's `DiffLineInfo` entries for
/// `Addition` and `Deletion` lines, then delegates to `unstage_lines`.
///
/// Returns the number of lines affected.
///
/// # Errors
///
/// Returns any error from `unstage_lines`.
pub fn unstage_hunk(repo: &Repository, file_path: &str, hunk: &HunkInfo) -> Result<u32, PgsError> {
    let mut selected = HashSet::new();
    for line in &hunk.lines {
        match line.origin {
            LineOrigin::Addition | LineOrigin::Deletion => {
                selected.insert(line.line_number);
            }
            // `Mixed` tags split-hunk classification ranges; `DiffLineInfo` never carries it.
            LineOrigin::Context | LineOrigin::Mixed => {}
        }
    }
    unstage_lines(repo, file_path, &selected)
}

/// Count the number of lines in a byte slice (for reporting).
fn count_lines(content: &[u8]) -> u32 {
    if content.is_empty() {
        return 0;
    }
    let text = String::from_utf8_lossy(content);
    saturating_u32(text.lines().count())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use git2::Repository;
    use tempfile::TempDir;

    use super::*;

    /// Create a repo with an initial commit containing the given files.
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

        let mut index = repo.index().expect("index");
        for &(path, content) in files {
            let file_path = dir.path().join(path);
            if let Some(parent) = file_path.parent() {
                std::fs::create_dir_all(parent).expect("create parent dirs");
            }
            std::fs::write(&file_path, content).expect("write file");
            index.add_path(Path::new(path)).expect("add to index");
        }
        index.write().expect("write index");
        let tree_oid = index.write_tree().expect("write tree");
        {
            let tree = repo.find_tree(tree_oid).expect("find tree");
            let sig = repo.signature().expect("sig");
            repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
                .expect("commit");
        }
        (dir, repo)
    }

    /// Stage content into the index by writing to the workdir and adding.
    fn stage_content(repo: &Repository, dir: &Path, path: &str, content: &str) {
        let file_path = dir.join(path);
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent).expect("create parent dirs");
        }
        std::fs::write(&file_path, content).expect("write file");
        let mut index = repo.index().expect("index");
        index.add_path(Path::new(path)).expect("add to index");
        index.write().expect("write index");
    }

    /// Read the current index blob content as a string.
    fn read_index_content(repo: &Repository, path: &str) -> Option<String> {
        let index = repo.index().expect("index");
        let entry = index.get_path(Path::new(path), 0)?;
        let blob = repo.find_blob(entry.id).expect("find blob");
        Some(String::from_utf8_lossy(blob.content()).to_string())
    }

    #[test]
    fn unstage_file_modified_restores_head() {
        let (dir, repo) = setup_repo_with_commit(&[("file.txt", "original\n")]);
        stage_content(&repo, dir.path(), "file.txt", "modified\n");

        // Verify index has the modified content
        let before = read_index_content(&repo, "file.txt").expect("in index");
        assert_eq!(before, "modified\n");

        // Unstage
        let lines = unstage_file(&repo, "file.txt").expect("unstage_file");
        assert!(lines > 0);

        // Verify index now matches HEAD
        let after = read_index_content(&repo, "file.txt").expect("in index");
        assert_eq!(after, "original\n");
    }

    #[test]
    fn unstage_file_added_removes_from_index() {
        // Create a repo with an initial commit (different file)
        let (dir, repo) = setup_repo_with_commit(&[("existing.txt", "hello\n")]);

        // Stage a new file that doesn't exist in HEAD
        stage_content(&repo, dir.path(), "new_file.txt", "brand new\n");

        // Verify it's in the index
        let before = read_index_content(&repo, "new_file.txt");
        assert!(before.is_some());

        // Unstage
        let lines = unstage_file(&repo, "new_file.txt").expect("unstage_file");
        assert!(lines > 0);

        // Verify it's removed from the index
        let after = read_index_content(&repo, "new_file.txt");
        assert!(after.is_none(), "new_file.txt should be removed from index");
    }

    #[test]
    fn unstage_file_deleted_restores_head_blob() {
        let (_dir, repo) = setup_repo_with_commit(&[("doomed.txt", "keep me\n")]);

        // Stage the deletion: remove from index
        {
            let mut index = repo.index().expect("index");
            index.remove_path(Path::new("doomed.txt")).expect("remove");
            index.write().expect("write");
        }

        // Verify it's gone from the index
        let before = read_index_content(&repo, "doomed.txt");
        assert!(before.is_none(), "should be removed from index");

        // Unstage the deletion
        let lines = unstage_file(&repo, "doomed.txt").expect("unstage_file");
        assert!(lines > 0);

        // Verify HEAD blob is restored
        let after = read_index_content(&repo, "doomed.txt").expect("should be back");
        assert_eq!(after, "keep me\n");
    }

    #[test]
    fn unstage_lines_selects_subset() {
        // HEAD: "line1\nline2\nline3\n"
        // Index: "line1\nMODIFIED\nline3\nnew line4\n"
        // Unstage only the modification (line2 change), keep the addition (line4).
        let (dir, repo) = setup_repo_with_commit(&[("f.txt", "line1\nline2\nline3\n")]);
        stage_content(
            &repo,
            dir.path(),
            "f.txt",
            "line1\nMODIFIED\nline3\nnew line4\n",
        );

        // The HEAD→index diff:
        //   line1    (equal)
        //   -line2   (delete, old_index=1 → line 2 in HEAD)
        //   +MODIFIED (insert, new_index=1 → line 2 in index)
        //   line3    (equal)
        //   +new line4 (insert, new_index=3 → line 4 in index)
        //
        // To unstage only the modification: select line 2 (HEAD deletion) and line 2 (index insertion).
        let selected: HashSet<u32> = std::iter::once(2).collect();
        let lines = unstage_lines(&repo, "f.txt", &selected).expect("unstage_lines");
        assert!(lines > 0, "should have affected lines");

        // Result should be: "line1\nline2\nline3\nnew line4\n"
        // (line2 restored from HEAD, new line4 kept)
        let result = read_index_content(&repo, "f.txt").expect("in index");
        assert_eq!(result, "line1\nline2\nline3\nnew line4\n");
    }

    #[test]
    fn unstage_lines_preserves_trailing_newline() {
        let (dir, repo) = setup_repo_with_commit(&[("f.txt", "aaa\n")]);
        stage_content(&repo, dir.path(), "f.txt", "aaa\nbbb\n");

        // Index content ends with \n. After unstaging the addition (line 2),
        // result should still end with \n.
        let selected: HashSet<u32> = std::iter::once(2).collect();
        unstage_lines(&repo, "f.txt", &selected).expect("unstage_lines");

        let result = read_index_content(&repo, "f.txt").expect("in index");
        assert!(
            result.ends_with('\n'),
            "result should end with newline, got: {result:?}"
        );
        assert_eq!(result, "aaa\n");
    }

    #[test]
    fn unstage_lines_preserves_no_trailing_newline() {
        let (dir, repo) = setup_repo_with_commit(&[("f.txt", "aaa")]);
        stage_content(&repo, dir.path(), "f.txt", "aaa\nbbb");

        // Index content does NOT end with \n. After unstaging the addition,
        // result should also NOT end with \n.
        let selected: HashSet<u32> = std::iter::once(2).collect();
        unstage_lines(&repo, "f.txt", &selected).expect("unstage_lines");

        let result = read_index_content(&repo, "f.txt").expect("in index");
        assert!(
            !result.ends_with('\n'),
            "result should NOT end with newline, got: {result:?}"
        );
        assert_eq!(result, "aaa");
    }

    #[test]
    fn unstage_hunk_with_pure_deletion_restores_line() {
        // Reproduce bug: unstage_hunk collects Addition+Deletion line numbers.
        // For a pure deletion staged hunk, only Deletion lines exist.
        // The line_number for a Deletion in a HEAD→index diff uses old_lineno (HEAD side),
        // but unstage_lines looks for old_line numbers in selected_lines to restore them.
        // Verify the full round-trip: stage deletion → unstage hunk → index restored.
        let (dir, repo) = setup_repo_with_commit(&[("f.txt", "line1\nline2\nline3\n")]);

        // Stage the deletion of line2 into the index
        stage_content(&repo, dir.path(), "f.txt", "line1\nline3\n");

        // Verify the deletion is staged
        let staged_before = read_index_content(&repo, "f.txt").expect("in index");
        assert_eq!(staged_before, "line1\nline3\n", "deletion should be staged");

        // Build the HEAD→index diff to get the actual hunk metadata
        let diff = crate::git::diff::diff_head_to_index(&repo, 3).expect("diff");
        let count = diff.deltas().count();
        assert_eq!(count, 1, "expected 1 staged delta");

        let patch = git2::Patch::from_diff(&diff, 0)
            .expect("patch from diff")
            .expect("patch is Some");
        assert!(patch.num_hunks() >= 1, "expected at least 1 hunk");

        // Extract hunk info manually (same logic as build_scan_result)
        let (hunk_header, _) = patch.hunk(0).expect("hunk 0");
        let line_count = patch.num_lines_in_hunk(0).expect("line count");
        let mut lines = Vec::new();
        for l in 0..line_count {
            let line = patch.line_in_hunk(0, l).expect("line");
            let origin = match line.origin_value() {
                git2::DiffLineType::Context => crate::models::LineOrigin::Context,
                git2::DiffLineType::Addition => crate::models::LineOrigin::Addition,
                git2::DiffLineType::Deletion => crate::models::LineOrigin::Deletion,
                _ => continue,
            };
            let line_number = match origin {
                crate::models::LineOrigin::Deletion => line.old_lineno().unwrap_or(0),
                crate::models::LineOrigin::Context
                | crate::models::LineOrigin::Addition
                | crate::models::LineOrigin::Mixed => line.new_lineno().unwrap_or(0),
            };
            lines.push(crate::models::DiffLineInfo {
                line_number,
                origin,
                content: std::str::from_utf8(line.content())
                    .unwrap_or("")
                    .trim_end_matches('\n')
                    .trim_end_matches('\r')
                    .to_string(),
            });
        }

        let hunk = crate::models::HunkInfo {
            hunk_id: "test_deletion_hunk".into(),
            old_start: hunk_header.old_start(),
            old_lines: hunk_header.old_lines(),
            new_start: hunk_header.new_start(),
            new_lines: hunk_header.new_lines(),
            header: String::from_utf8_lossy(hunk_header.header())
                .trim_end()
                .to_string(),
            lines,
            checksum: "test".into(),
            whitespace_only: false,
        };

        // Verify the hunk contains a deletion line (line2 was deleted from index)
        let has_deletion = hunk
            .lines
            .iter()
            .any(|l| l.origin == crate::models::LineOrigin::Deletion);
        assert!(has_deletion, "hunk should contain a deletion line");

        // Unstage the deletion hunk — should restore line2 in the index
        let affected = unstage_hunk(&repo, "f.txt", &hunk).expect("unstage_hunk");
        assert!(affected > 0, "should have affected at least one line");

        // Index should now match HEAD content
        let after = read_index_content(&repo, "f.txt").expect("in index after unstage");
        assert_eq!(
            after, "line1\nline2\nline3\n",
            "index should be restored to HEAD after unstaging deletion hunk"
        );
    }

    #[test]
    fn unstage_hunk_delegates_to_unstage_lines() {
        // HEAD: "alpha\nbeta\ngamma\n"
        // Index: "alpha\nBETA\ngamma\n"
        // Hunk describes the change from beta->BETA.
        let (dir, repo) = setup_repo_with_commit(&[("h.txt", "alpha\nbeta\ngamma\n")]);
        stage_content(&repo, dir.path(), "h.txt", "alpha\nBETA\ngamma\n");

        let hunk = HunkInfo {
            hunk_id: "test_hunk".into(),
            old_start: 2,
            old_lines: 1,
            new_start: 2,
            new_lines: 1,
            header: "@@ -2,1 +2,1 @@".into(),
            lines: vec![
                crate::models::DiffLineInfo {
                    line_number: 2,
                    origin: LineOrigin::Deletion,
                    content: "beta".into(),
                },
                crate::models::DiffLineInfo {
                    line_number: 2,
                    origin: LineOrigin::Addition,
                    content: "BETA".into(),
                },
            ],
            checksum: "test_checksum".into(),
            whitespace_only: false,
        };

        let lines = unstage_hunk(&repo, "h.txt", &hunk).expect("unstage_hunk");
        assert!(lines > 0, "should have affected lines");

        // Verify index is restored to HEAD content
        let result = read_index_content(&repo, "h.txt").expect("in index");
        assert_eq!(result, "alpha\nbeta\ngamma\n");
    }
}
