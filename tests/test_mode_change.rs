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

#[test]
fn stage_lines_does_not_change_mode() {
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

    // Verify the index mode is still the original 0o100644
    // (line-level staging does not carry over the mode change)
    let repo2 = git2::Repository::open(dir.path()).unwrap();
    let index_mode = read_index_mode(&repo2, "script.sh");
    assert_eq!(
        index_mode, 0o100_644,
        "line-level staging should not change index mode, got {index_mode:#o}"
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
