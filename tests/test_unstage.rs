mod common;

use std::path::Path;

use common::{commit_file, read_staged_blob, run_pgs, setup_repo, write_file};
use pgs::git::diff::{build_scan_result, diff_head_to_index};

#[test]
fn unstage_file_restores_to_head() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "add hello");
    write_file(dir.path(), "hello.txt", "line1\nline2\n");

    // Stage the file
    run_pgs(dir.path(), &["stage", "hello.txt"]).success();

    // Verify it is staged
    let status_output = run_pgs(dir.path(), &["status"]).success();
    let status_stdout = String::from_utf8(status_output.get_output().stdout.clone()).unwrap();
    let status_json: serde_json::Value = serde_json::from_str(&status_stdout).unwrap();
    assert!(
        !status_json["files"].as_array().unwrap().is_empty(),
        "file should be staged before unstage"
    );

    // Unstage the file
    let output = run_pgs(dir.path(), &["unstage", "hello.txt"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["version"], "v1");
    assert_eq!(json["command"], "unstage");
    assert_eq!(json["status"], "ok");
    let items = json["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["selection"], "hello.txt");
    assert!(items[0]["lines_affected"].as_u64().unwrap() > 0);
    assert!(json["backup_id"].is_string());

    // Verify status shows nothing staged
    let status_output2 = run_pgs(dir.path(), &["status"]).success();
    let status_stdout2 = String::from_utf8(status_output2.get_output().stdout.clone()).unwrap();
    let status_json2: serde_json::Value = serde_json::from_str(&status_stdout2).unwrap();
    let files = status_json2["files"].as_array().unwrap();
    assert!(files.is_empty(), "after unstage, nothing should be staged");
}

#[test]
fn unstage_dry_run_keeps_staged() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "add hello");
    write_file(dir.path(), "hello.txt", "line1\nline2\n");

    // Stage the file
    run_pgs(dir.path(), &["stage", "hello.txt"]).success();

    // Unstage with --dry-run
    let output = run_pgs(dir.path(), &["unstage", "--dry-run", "hello.txt"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["version"], "v1");
    assert_eq!(json["command"], "unstage");
    assert_eq!(json["status"], "dry_run");
    assert_eq!(json["backup_id"], serde_json::Value::Null);
    let items = json["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["selection"], "hello.txt");
    assert!(items[0]["lines_affected"].as_u64().unwrap() > 0);
    assert!(items[0].get("lines_staged").is_none());

    // Verify file is still staged
    let status_output = run_pgs(dir.path(), &["status"]).success();
    let status_stdout = String::from_utf8(status_output.get_output().stdout.clone()).unwrap();
    let status_json: serde_json::Value = serde_json::from_str(&status_stdout).unwrap();
    let staged = status_json["files"].as_array().unwrap();
    assert!(
        !staged.is_empty(),
        "dry-run unstage should not remove staged changes"
    );
}

#[test]
fn unstage_unknown_hunk_returns_exit_code_2() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "add hello");
    write_file(dir.path(), "hello.txt", "line1\nline2\n");

    // Stage the file first so unstage has something to work with
    run_pgs(dir.path(), &["stage", "hello.txt"]).success();

    // Try to unstage a nonexistent hunk ID
    run_pgs(dir.path(), &["unstage", "deadbeef0000"]).code(2);
}

#[test]
fn unstage_multiple_line_selections_same_file_reports_each_selection_item() {
    let (dir, repo) = setup_repo();
    commit_file(
        &repo,
        dir.path(),
        "multi.txt",
        "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nline15\n",
        "add multi",
    );
    write_file(
        dir.path(),
        "multi.txt",
        "line1\nchanged-2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nchanged-12\nline13\nline14\nline15\n",
    );

    run_pgs(dir.path(), &["stage", "multi.txt"]).success();

    let output = run_pgs(dir.path(), &["unstage", "multi.txt:2-2", "multi.txt:12-12"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["status"], "ok");
    let items = json["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["selection"], "multi.txt:2-2");
    assert_eq!(items[1]["selection"], "multi.txt:12-12");
    assert!(items[0]["lines_affected"].as_u64().unwrap() > 0);
    assert!(items[1]["lines_affected"].as_u64().unwrap() > 0);
}

#[test]
fn unstage_directory_unstages_all_matching_files() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "subdir/file1.txt", "a\n", "add subdir");
    commit_file(&repo, dir.path(), "subdir/file2.txt", "b\n", "add file2");
    write_file(dir.path(), "subdir/file1.txt", "a\nmodified1\n");
    write_file(dir.path(), "subdir/file2.txt", "b\nmodified2\n");

    run_pgs(dir.path(), &["stage", "subdir/file1.txt"]).success();
    run_pgs(dir.path(), &["stage", "subdir/file2.txt"]).success();

    let status_before = run_pgs(dir.path(), &["status"]).success();
    let stdout_before = String::from_utf8(status_before.get_output().stdout.clone()).unwrap();
    let json_before: serde_json::Value = serde_json::from_str(&stdout_before).unwrap();
    assert_eq!(
        json_before["files"].as_array().unwrap().len(),
        2,
        "both files should be staged before unstage"
    );

    run_pgs(dir.path(), &["unstage", "subdir/"]).success();

    let status_after = run_pgs(dir.path(), &["status"]).success();
    let stdout_after = String::from_utf8(status_after.get_output().stdout.clone()).unwrap();
    let json_after: serde_json::Value = serde_json::from_str(&stdout_after).unwrap();
    let files_after = json_after["files"].as_array().unwrap();
    assert!(
        files_after.is_empty(),
        "both files should be unstaged after unstage subdir/"
    );
}

/// Read the current content of a path from the git index as a String.
fn read_index_content(repo: &git2::Repository, path: &str) -> Option<String> {
    let index = repo.index().expect("index");
    let entry = index.get_path(Path::new(path), 0)?;
    let blob = repo.find_blob(entry.id).expect("find blob");
    Some(String::from_utf8_lossy(blob.content()).to_string())
}

/// Write content to the workdir and stage it via `index.add_path` (direct libgit2 — does NOT invoke
/// the pgs staging path, which is under test).
fn stage_content_direct(repo: &git2::Repository, dir: &std::path::Path, path: &str, content: &str) {
    let file_path = dir.join(path);
    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent).expect("create parent dirs");
    }
    std::fs::write(&file_path, content).expect("write workdir file");
    let mut index = repo.index().expect("index");
    index.add_path(Path::new(path)).expect("add_path");
    index.write().expect("write index");
}

#[test]
fn unstage_hunk_by_id_does_not_leak_adjacent_hunk_when_head_old_line_aliases_index_new_line() {
    // -----------------------------------------------------------------------
    // Fixture design
    // -----------------------------------------------------------------------
    // HEAD: 30 lines "L01\n" … "L30\n"
    // Index (post-staging): L01-L04 (unchanged), HEAD lines 5-14 DELETED (10-line
    //   deletion), HEAD lines 15-22 unchanged (8-line gap = index lines 5-12),
    //   "INSERTED\n" injected at index line 13, HEAD lines 23-30 unchanged
    //   (index lines 14-21).  Total index = 4 + 8 + 1 + 8 = 21 lines.
    //
    // The 8-line gap ensures libgit2 emits TWO distinct HEAD→index hunks with
    // context=3 (need gap > 2*3 = 6 lines):
    //   Hunk A: pure Deletion of HEAD lines 5-14 (line_numbers 5..14 in HEAD-old-space)
    //   Hunk B: pure Addition of "INSERTED" at index-new-line 13
    //
    // Bug reproduced by: unstage_hunk(hunk_a) puts {5..14} into `selected`.
    // Then unstage_lines walks the HEAD→index diff and hits the Insert of
    // "INSERTED" with new_line=13.  selected.contains(13) → true → the addition
    // is silently dropped.  Index loses "INSERTED" even though only hunk A was
    // requested.
    // -----------------------------------------------------------------------

    let (dir, repo) = setup_repo();

    // Build HEAD content: 30 lines "L01\n" through "L30\n"
    let head_content: String = (1u32..=30).fold(String::new(), |mut s, n| {
        use std::fmt::Write;
        let _ = writeln!(s, "L{n:02}");
        s
    });
    commit_file(&repo, dir.path(), "f.txt", &head_content, "add f.txt");

    // Build index content:
    //   lines 1-4  : L01-L04  (same as HEAD)
    //   lines 5-12 : L15-L22  (HEAD lines 5-14 deleted; L15 becomes index line 5)
    //   line  13   : INSERTED
    //   lines 14-21: L23-L30  (same as HEAD)
    let index_content: String = {
        use std::fmt::Write;
        let mut s = String::new();
        for n in 1u32..=4 {
            let _ = writeln!(s, "L{n:02}");
        }
        for n in 15u32..=22 {
            let _ = writeln!(s, "L{n:02}");
        }
        s.push_str("INSERTED\n");
        for n in 23u32..=30 {
            let _ = writeln!(s, "L{n:02}");
        }
        s
    };

    // Stage via direct index.add_path — must NOT use `pgs stage` because that
    // exercises the buggy staging path we are testing.
    stage_content_direct(&repo, dir.path(), "f.txt", &index_content);

    // ------------------------------------------------------------------
    // Assertion 1 (fixture invariant): HEAD→index diff must have exactly 2 hunks.
    // ------------------------------------------------------------------
    let diff = pgs::git::diff::diff_head_to_index(&repo, 3).expect("diff_head_to_index");
    let scan = pgs::git::diff::build_scan_result(&repo, &diff, None).expect("build_scan_result");

    assert_eq!(
        scan.files.len(),
        1,
        "expected exactly 1 staged file in HEAD→index diff"
    );
    assert_eq!(
        scan.files[0].hunks.len(),
        2,
        "fixture invariant: HEAD→index diff must have exactly 2 hunks (hunk A = deletion, hunk B = addition)"
    );

    let hunk_a = &scan.files[0].hunks[0];
    let hunk_b = &scan.files[0].hunks[1];

    // Sanity check hunk roles
    let hunk_a_has_deletions = hunk_a
        .lines
        .iter()
        .any(|l| l.origin == pgs::models::LineOrigin::Deletion);
    let hunk_b_has_additions = hunk_b
        .lines
        .iter()
        .any(|l| l.origin == pgs::models::LineOrigin::Addition);
    assert!(hunk_a_has_deletions, "hunks[0] should be the deletion hunk");
    assert!(hunk_b_has_additions, "hunks[1] should be the addition hunk");

    // ------------------------------------------------------------------
    // Action: unstage only hunk A (the deletion)
    // ------------------------------------------------------------------
    let affected =
        pgs::git::unstaging::unstage_hunk(&repo, "f.txt", hunk_a).expect("unstage_hunk(hunk_a)");
    assert!(
        affected > 0,
        "unstaging hunk A should affect at least one line"
    );

    // ------------------------------------------------------------------
    // Assertion 2: hunk A's HEAD lines are restored (this passes even on buggy code).
    // The index should now contain HEAD lines L05-L14 back in positions 5-14.
    // ------------------------------------------------------------------
    let index_after = read_index_content(&repo, "f.txt")
        .expect("f.txt should still be in the index after unstage_hunk(A)");

    for n in 5u32..=14 {
        assert!(
            index_after.contains(&format!("L{n:02}\n")),
            "after unstage_hunk(A): index should contain L{n:02} (restored from HEAD) but got:\n{index_after}"
        );
    }

    // ------------------------------------------------------------------
    // Assertion 3 (the RED assertion): hunk B's addition must NOT have been
    // touched.  Under the bug, unstage_hunk(A) puts HEAD-old-line 13 into
    // `selected`, which aliases index-new-line 13 (where INSERTED lives), so
    // unstage_lines drops the addition.
    // ------------------------------------------------------------------
    assert!(
        index_after.contains("INSERTED\n"),
        "LEAKED: unstage_hunk(A) incorrectly removed hunk B's addition — \
         HEAD-old-line 13 aliased index-new-line 13 in the shared HashSet. \
         Index after unstage:\n{index_after}"
    );
}

/// Stage a 40-line file with two separated modifications, then return the ids of the
/// two hunks in the HEAD -> Index diff that `unstage` resolves against.
fn staged_two_hunk_fixture(dir: &Path, repo: &git2::Repository) -> Vec<String> {
    let base: Vec<String> = (1..=40).map(|i| format!("line{i}\n")).collect();
    let base = base.concat();
    commit_file(repo, dir, "f.txt", &base, "add f");
    let modified: Vec<String> = (1..=40)
        .map(|i| match i {
            5 => "CHANGED5\n".to_owned(),
            28 => "CHANGED28\n".to_owned(),
            _ => format!("line{i}\n"),
        })
        .collect();
    let modified = modified.concat();
    write_file(dir, "f.txt", &modified);
    run_pgs(dir, &["stage", "f.txt"]).success();

    let reopened = git2::Repository::open(dir).unwrap();
    let diff = diff_head_to_index(&reopened, 3).unwrap();
    let scan = build_scan_result(&reopened, &diff, None).unwrap();
    assert_eq!(scan.files.len(), 1, "fixture must stage exactly one file");
    assert_eq!(
        scan.files[0].hunks.len(),
        2,
        "fixture must produce exactly two staged hunks"
    );
    scan.files[0]
        .hunks
        .iter()
        .map(|h| h.hunk_id.clone())
        .collect()
}

fn staged_blob(dir: &Path) -> String {
    let reopened = git2::Repository::open(dir).unwrap();
    read_staged_blob(&reopened, "f.txt")
}

#[test]
fn unstage_same_file_hunk_and_line_range_rejects_without_touching_index() {
    let (dir, repo) = setup_repo();
    let hunk_ids = staged_two_hunk_fixture(dir.path(), &repo);
    let before = staged_blob(dir.path());

    let output = run_pgs(dir.path(), &["unstage", &hunk_ids[0], "f.txt:28-28"]).code(2);
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["code"], "invalid_selection");
    assert!(
        json["message"].as_str().unwrap().contains("f.txt"),
        "message must name the offending file: {json}"
    );
    assert_eq!(
        staged_blob(dir.path()),
        before,
        "rejection must leave the index untouched"
    );
}

#[test]
fn unstage_same_file_whole_file_and_line_range_rejects_without_touching_index() {
    let (dir, repo) = setup_repo();
    staged_two_hunk_fixture(dir.path(), &repo);
    let before = staged_blob(dir.path());

    run_pgs(dir.path(), &["unstage", "f.txt", "f.txt:28-28"]).code(2);
    assert_eq!(
        staged_blob(dir.path()),
        before,
        "rejection must leave the index untouched"
    );
}

#[test]
fn unstage_multiple_line_ranges_on_one_file_succeeds() {
    let (dir, repo) = setup_repo();
    staged_two_hunk_fixture(dir.path(), &repo);

    run_pgs(dir.path(), &["unstage", "f.txt:5-5", "f.txt:28-28"]).success();

    let staged = staged_blob(dir.path());
    assert!(!staged.contains("CHANGED5"), "blob:\n{staged}");
    assert!(!staged.contains("CHANGED28"), "blob:\n{staged}");
}

#[test]
fn unstage_multiple_hunk_ids_on_one_file_succeeds() {
    let (dir, repo) = setup_repo();
    let hunk_ids = staged_two_hunk_fixture(dir.path(), &repo);

    run_pgs(dir.path(), &["unstage", &hunk_ids[0], &hunk_ids[1]]).success();

    let staged = staged_blob(dir.path());
    assert!(!staged.contains("CHANGED5"), "blob:\n{staged}");
    assert!(!staged.contains("CHANGED28"), "blob:\n{staged}");
}

#[test]
fn unstage_different_selector_kinds_on_different_files_succeeds() {
    let (dir, repo) = setup_repo();
    // g.txt must be committed BEFORE f.txt is staged: `commit_file` commits the whole
    // index, which would swallow f.txt's staged changes and empty the HEAD -> Index diff.
    commit_file(&repo, dir.path(), "g.txt", "g1\n", "add g");
    let hunk_ids = staged_two_hunk_fixture(dir.path(), &repo);
    write_file(dir.path(), "g.txt", "g1\ng2\n");
    run_pgs(dir.path(), &["stage", "g.txt"]).success();

    run_pgs(dir.path(), &["unstage", &hunk_ids[0], "g.txt"]).success();

    let staged = staged_blob(dir.path());
    assert!(!staged.contains("CHANGED5"), "blob:\n{staged}");
    assert!(
        staged.contains("CHANGED28"),
        "only hunk 0 was unstaged; blob:\n{staged}"
    );
}
