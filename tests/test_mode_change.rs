#![cfg(unix)]

mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use common::{commit_file, run_pgs, setup_repo};

/// Set a file's permissions to executable (0o755).
fn make_executable(dir: &Path, rel_path: &str) {
    let full = dir.join(rel_path);
    let perms = fs::Permissions::from_mode(0o755);
    fs::set_permissions(full, perms).unwrap();
}

/// Read the file mode of an index entry.
fn read_index_mode(repo: &git2::Repository, path: &str) -> u32 {
    let index = repo.index().unwrap();
    index.get_path(Path::new(path), 0).unwrap().mode
}

#[test]
fn scan_mode_only_change_shows_file() {
    let (dir, repo) = setup_repo();
    repo.config()
        .unwrap()
        .set_bool("core.filemode", true)
        .unwrap();
    commit_file(
        &repo,
        dir.path(),
        "script.sh",
        "#!/bin/sh\necho hi\n",
        "add script",
    );
    make_executable(dir.path(), "script.sh");

    let output = run_pgs(dir.path(), &["scan"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let files = json["files"].as_array().unwrap();
    assert!(!files.is_empty(), "expected at least one file in scan");

    let file = files
        .iter()
        .find(|f| f["path"] == "script.sh")
        .expect("script.sh should appear in scan");

    assert_eq!(
        file["status"]["type"], "Modified",
        "mode-only change should appear as Modified"
    );

    // old_mode and new_mode are only present when they differ
    assert!(
        file["old_mode"].is_string(),
        "old_mode should be present when mode changed"
    );
    assert!(
        file["new_mode"].is_string(),
        "new_mode should be present when mode changed"
    );

    assert_eq!(
        file["old_mode"].as_str().unwrap(),
        "100644",
        "old mode should be regular (100644)"
    );
    assert_eq!(
        file["new_mode"].as_str().unwrap(),
        "100755",
        "new mode should be executable (100755)"
    );
}

#[test]
fn stage_mode_only_change_updates_index() {
    let (dir, repo) = setup_repo();
    repo.config()
        .unwrap()
        .set_bool("core.filemode", true)
        .unwrap();
    commit_file(
        &repo,
        dir.path(),
        "script.sh",
        "#!/bin/sh\necho hi\n",
        "add script",
    );
    make_executable(dir.path(), "script.sh");

    // Stage the mode-only change
    let output = run_pgs(dir.path(), &["stage", "script.sh"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(
        json["status"], "ok",
        "staging mode-only change should succeed"
    );

    // Verify the index now has executable mode
    let repo2 = git2::Repository::open(dir.path()).unwrap();
    let index_mode = read_index_mode(&repo2, "script.sh");
    assert_eq!(
        index_mode, 0o100_755,
        "index entry should have executable mode 0o100755 after staging, got {index_mode:#o}"
    );
}

#[test]
fn unstage_mode_change_restores_head_mode() {
    let (dir, repo) = setup_repo();
    repo.config()
        .unwrap()
        .set_bool("core.filemode", true)
        .unwrap();
    commit_file(
        &repo,
        dir.path(),
        "script.sh",
        "#!/bin/sh\necho hi\n",
        "add script",
    );
    make_executable(dir.path(), "script.sh");

    // Stage the mode change first
    run_pgs(dir.path(), &["stage", "script.sh"]).success();

    // Verify mode is staged
    let repo_check = git2::Repository::open(dir.path()).unwrap();
    let staged_mode = read_index_mode(&repo_check, "script.sh");
    assert_eq!(staged_mode, 0o100_755, "mode should be staged as 0o100755");
    drop(repo_check);

    // Now unstage it
    let output = run_pgs(dir.path(), &["unstage", "script.sh"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["status"], "ok", "unstaging should succeed");

    // Verify the index mode is restored to HEAD mode (0o100644)
    let repo3 = git2::Repository::open(dir.path()).unwrap();
    let restored_mode = read_index_mode(&repo3, "script.sh");
    assert_eq!(
        restored_mode, 0o100_644,
        "index mode should be restored to 0o100644 after unstage, got {restored_mode:#o}"
    );
}

#[test]
fn stage_content_plus_mode_stages_both() {
    let (dir, repo) = setup_repo();
    repo.config()
        .unwrap()
        .set_bool("core.filemode", true)
        .unwrap();
    commit_file(&repo, dir.path(), "script.sh", "line1\n", "add script");

    // Modify content AND make executable
    common::write_file(dir.path(), "script.sh", "line1\nline2\n");
    make_executable(dir.path(), "script.sh");

    let output = run_pgs(dir.path(), &["stage", "script.sh"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["status"], "ok", "staging content+mode should succeed");

    // Verify the index has both new content and executable mode
    let repo2 = git2::Repository::open(dir.path()).unwrap();

    // Check mode
    let index_mode = read_index_mode(&repo2, "script.sh");
    assert_eq!(
        index_mode, 0o100_755,
        "index should have executable mode after staging content+mode, got {index_mode:#o}"
    );

    // Check content — read blob from index
    let mut index = repo2.index().unwrap();
    index.read(true).unwrap();
    let entry = index.get_path(Path::new("script.sh"), 0).unwrap();
    let blob = repo2.find_blob(entry.id).unwrap();
    let content = std::str::from_utf8(blob.content()).unwrap();
    assert_eq!(
        content, "line1\nline2\n",
        "staged content should include both lines"
    );
}

/// Partial line-range stage of a file that has BOTH a mode change and content
/// edits must propagate the new mode into the index entry — the exec bit must
/// not be silently dropped.
#[test]
fn stage_lines_with_mode_change_propagates_exec_bit() {
    let (dir, repo) = setup_repo();
    repo.config()
        .unwrap()
        .set_bool("core.filemode", true)
        .unwrap();
    commit_file(
        &repo,
        dir.path(),
        "script.sh",
        "line1\nline2\nline3\n",
        "add script",
    );

    // Modify content AND make executable
    common::write_file(dir.path(), "script.sh", "line1\nMODIFIED\nline3\n");
    make_executable(dir.path(), "script.sh");

    // Stage only specific lines (line-level selection)
    let output = run_pgs(dir.path(), &["stage", "script.sh:2-2"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["status"], "ok", "line-level staging should succeed");

    // The index entry must carry the new executable mode even though only a
    // line range was staged (not the whole file).
    let repo2 = git2::Repository::open(dir.path()).unwrap();
    let index_mode = read_index_mode(&repo2, "script.sh");
    assert_eq!(
        index_mode, 0o100_755,
        "line-level staging with mode change must propagate exec bit, got {index_mode:#o}"
    );
}

/// Partial line-range stage where the file has NO mode change must preserve
/// the existing index mode (regression: `mode_override=None` must not clobber).
#[test]
fn stage_lines_content_only_preserves_existing_mode() {
    let (dir, repo) = setup_repo();
    repo.config()
        .unwrap()
        .set_bool("core.filemode", true)
        .unwrap();
    commit_file(
        &repo,
        dir.path(),
        "script.sh",
        "line1\nline2\nline3\n",
        "add script",
    );

    // Modify content only — no chmod, mode stays 0o100644
    common::write_file(dir.path(), "script.sh", "line1\nMODIFIED\nline3\n");

    let output = run_pgs(dir.path(), &["stage", "script.sh:2-2"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(
        json["status"], "ok",
        "content-only line staging should succeed"
    );

    let repo2 = git2::Repository::open(dir.path()).unwrap();
    let index_mode = read_index_mode(&repo2, "script.sh");
    assert_eq!(
        index_mode, 0o100_644,
        "content-only line staging must not change existing mode, got {index_mode:#o}"
    );
}

/// Partial hunk stage of a file with both mode change and content edits must
/// propagate the new executable mode into the index entry.
#[test]
fn stage_hunk_with_mode_change_propagates_exec_bit() {
    let (dir, repo) = setup_repo();
    repo.config()
        .unwrap()
        .set_bool("core.filemode", true)
        .unwrap();
    commit_file(
        &repo,
        dir.path(),
        "script.sh",
        "line1\nline2\nline3\n",
        "add script",
    );

    // Modify content AND make executable
    common::write_file(dir.path(), "script.sh", "line1\nMODIFIED\nline3\n");
    make_executable(dir.path(), "script.sh");

    // Scan to get the hunk ID, then stage by hunk ID
    let scan_out = run_pgs(dir.path(), &["scan"]).success();
    let scan_json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(scan_out.get_output().stdout.clone()).unwrap())
            .unwrap();
    let hunk_id = scan_json["files"][0]["hunks"][0]["id"]
        .as_str()
        .expect("hunk id");

    let output = run_pgs(dir.path(), &["stage", hunk_id]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["status"], "ok", "hunk staging should succeed");

    let repo2 = git2::Repository::open(dir.path()).unwrap();
    let index_mode = read_index_mode(&repo2, "script.sh");
    assert_eq!(
        index_mode, 0o100_755,
        "hunk staging with mode change must propagate exec bit, got {index_mode:#o}"
    );
}

#[test]
fn stage_new_executable_file_preserves_mode() {
    let (dir, repo) = setup_repo();
    repo.config()
        .unwrap()
        .set_bool("core.filemode", true)
        .unwrap();
    // Need an initial commit so HEAD exists — setup_repo() already provides one.
    commit_file(&repo, dir.path(), "existing.txt", "hello\n", "add existing");

    // Write a NEW file and make it executable
    common::write_file(dir.path(), "new_script.sh", "#!/bin/sh\necho hello\n");
    make_executable(dir.path(), "new_script.sh");

    let output = run_pgs(dir.path(), &["stage", "new_script.sh"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(
        json["status"], "ok",
        "staging new executable file should succeed"
    );

    let repo2 = git2::Repository::open(dir.path()).unwrap();
    let index_mode = read_index_mode(&repo2, "new_script.sh");
    assert_eq!(
        index_mode, 0o100_755,
        "new executable file should have mode 0o100755 in index, got {index_mode:#o}"
    );
}

#[test]
fn stage_new_file_default_mode_unchanged() {
    let (dir, repo) = setup_repo();
    repo.config()
        .unwrap()
        .set_bool("core.filemode", true)
        .unwrap();
    commit_file(&repo, dir.path(), "existing.txt", "hello\n", "add existing");

    // Write a NEW file without chmod +x
    common::write_file(dir.path(), "new_script.sh", "#!/bin/sh\necho hello\n");

    let output = run_pgs(dir.path(), &["stage", "new_script.sh"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(
        json["status"], "ok",
        "staging new non-executable file should succeed"
    );

    let repo2 = git2::Repository::open(dir.path()).unwrap();
    let index_mode = read_index_mode(&repo2, "new_script.sh");
    assert_eq!(
        index_mode, 0o100_644,
        "new non-executable file should have mode 0o100644 in index, got {index_mode:#o}"
    );
}

#[test]
fn stage_renamed_executable_file_preserves_mode() {
    let (dir, repo) = setup_repo();
    repo.config()
        .unwrap()
        .set_bool("core.filemode", true)
        .unwrap();
    commit_file(
        &repo,
        dir.path(),
        "old_script.sh",
        "#!/bin/sh\necho hello\n",
        "add old script",
    );

    // Simulate rename: remove old, write new with same content, make executable
    fs::remove_file(dir.path().join("old_script.sh")).unwrap();
    common::write_file(dir.path(), "new_script.sh", "#!/bin/sh\necho hello\n");
    make_executable(dir.path(), "new_script.sh");

    // pgs does NOT perform rename detection (`diff_index_to_workdir` never calls
    // `find_similar`), so a filesystem rename always surfaces deterministically as
    // Deleted(old_script.sh) + Added(new_script.sh) — never as Renamed.
    let scan_output = run_pgs(dir.path(), &["scan"]).success();
    let scan_stdout = String::from_utf8(scan_output.get_output().stdout.clone()).unwrap();
    let scan_json: serde_json::Value = serde_json::from_str(&scan_stdout).unwrap();
    let files = scan_json["files"].as_array().unwrap();

    let new_file = files
        .iter()
        .find(|f| f["path"] == "new_script.sh")
        .expect("new_script.sh must appear in scan — rename surfaces as Added");
    assert_eq!(
        new_file["status"]["type"], "Added",
        "pgs never emits Renamed (no find_similar); expected Added, got {:?}",
        new_file["status"]["type"]
    );

    // Stage the Added file and verify the executable mode is preserved in the index.
    let output = run_pgs(dir.path(), &["stage", "new_script.sh"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        json["status"], "ok",
        "staging Added executable file should succeed"
    );

    let repo2 = git2::Repository::open(dir.path()).unwrap();
    let index_mode = read_index_mode(&repo2, "new_script.sh");
    assert_eq!(
        index_mode, 0o100_755,
        "Added executable file must be staged with mode 0o100755, got {index_mode:#o}"
    );
}

#[test]
fn scan_mode_change_summary_counts() {
    let (dir, repo) = setup_repo();
    repo.config()
        .unwrap()
        .set_bool("core.filemode", true)
        .unwrap();
    commit_file(
        &repo,
        dir.path(),
        "script.sh",
        "#!/bin/sh\necho hi\n",
        "add script",
    );
    make_executable(dir.path(), "script.sh");

    let output = run_pgs(dir.path(), &["scan"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let mode_changed = json["summary"]["mode_changed"]
        .as_u64()
        .expect("summary.mode_changed should be a number");
    assert_eq!(
        mode_changed, 1,
        "summary.mode_changed should be 1 for a single mode-changed file"
    );
}
