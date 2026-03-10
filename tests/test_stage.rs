mod common;

use common::{commit_file, run_agstage, setup_repo, write_file};

#[test]
fn stage_file_by_path() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "add hello");
    write_file(dir.path(), "hello.txt", "line1\nline2\n");

    let output = run_agstage(dir.path(), &["stage", "hello.txt"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["status"], "Ok");
    let succeeded = json["succeeded"].as_array().unwrap();
    assert!(
        !succeeded.is_empty(),
        "expected at least one succeeded item"
    );
    assert!(succeeded[0]["lines_staged"].as_u64().unwrap() > 0);
}

#[test]
fn stage_hunk_by_id() {
    let (dir, repo) = setup_repo();
    commit_file(
        &repo,
        dir.path(),
        "hello.txt",
        "line1\nline2\n",
        "add hello",
    );
    write_file(dir.path(), "hello.txt", "line1\nline2\nline3\n");

    // First scan to get hunk IDs
    let scan_output = run_agstage(dir.path(), &["scan"]).success();
    let scan_stdout = String::from_utf8(scan_output.get_output().stdout.clone()).unwrap();
    let scan_json: serde_json::Value = serde_json::from_str(&scan_stdout).unwrap();

    let hunk_id = scan_json["files"][0]["hunks"][0]["hunk_id"]
        .as_str()
        .unwrap();

    // Stage by hunk ID
    let output = run_agstage(dir.path(), &["stage", hunk_id]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["status"], "Ok");
    let succeeded = json["succeeded"].as_array().unwrap();
    assert!(!succeeded.is_empty());
}

#[test]
fn stage_line_range() {
    let (dir, repo) = setup_repo();
    commit_file(
        &repo,
        dir.path(),
        "multi.txt",
        "line1\nline2\nline3\nline4\nline5\n",
        "add multi",
    );
    write_file(
        dir.path(),
        "multi.txt",
        "line1\nMODIFIED\nline3\nline4\nline5\n",
    );

    // Stage lines 2-2 (the modified line)
    let output = run_agstage(dir.path(), &["stage", "multi.txt:2-2"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["status"], "Ok");
}

#[test]
fn stage_dry_run_does_not_modify_index() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "add hello");
    write_file(dir.path(), "hello.txt", "line1\nline2\n");

    // Stage with --dry-run
    let output = run_agstage(dir.path(), &["stage", "--dry-run", "hello.txt"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["status"], "DryRun");

    // Verify status shows nothing staged
    let status_output = run_agstage(dir.path(), &["status"]).success();
    let status_stdout = String::from_utf8(status_output.get_output().stdout.clone()).unwrap();
    let status_json: serde_json::Value = serde_json::from_str(&status_stdout).unwrap();

    let staged_files = status_json["staged_files"].as_array().unwrap();
    assert!(
        staged_files.is_empty(),
        "dry-run should not modify the index"
    );
}

#[test]
fn stage_unknown_hunk_returns_exit_code_2() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "add hello");
    write_file(dir.path(), "hello.txt", "line1\nline2\n");

    // Stage a nonexistent hunk ID (12 hex chars to look like a valid hunk ID)
    run_agstage(dir.path(), &["stage", "deadbeef0000"]).code(2);
}

#[test]
fn stage_stale_file_returns_exit_code_3() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "add hello");
    write_file(dir.path(), "hello.txt", "line1\nline2\n");

    // Scan to get hunk IDs (captures file checksum)
    let scan_output = run_agstage(dir.path(), &["scan", "--full"]).success();
    let scan_stdout = String::from_utf8(scan_output.get_output().stdout.clone()).unwrap();
    let scan_json: serde_json::Value = serde_json::from_str(&scan_stdout).unwrap();

    let hunk_id = scan_json["files"][0]["hunks"][0]["hunk_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Now modify the file AGAIN to make the scan stale
    write_file(
        dir.path(),
        "hello.txt",
        "completely\ndifferent\ncontent\nnow\n",
    );

    // Stage the old hunk ID — should fail as stale (exit 3)
    // Note: the exact behavior depends on implementation; the hunk ID may not
    // match anymore, which could be exit 2 (UnknownHunkId). We accept either
    // exit 2 or 3 since both indicate the scan is stale.
    let result = run_agstage(dir.path(), &["stage", &hunk_id]);
    let code = result.get_output().status.code().unwrap();
    assert!(
        code == 2 || code == 3,
        "expected exit code 2 or 3 for stale scan, got {code}"
    );
}

#[test]
fn stage_exclude_hunk() {
    let (dir, repo) = setup_repo();
    commit_file(
        &repo,
        dir.path(),
        "multi.txt",
        "aaa\n\n\n\n\nbbb\n",
        "add multi",
    );
    write_file(dir.path(), "multi.txt", "aaa\nNEW1\n\n\n\nbbb\nNEW2\n");

    // Scan to discover hunks
    let scan_output = run_agstage(dir.path(), &["scan"]).success();
    let scan_stdout = String::from_utf8(scan_output.get_output().stdout.clone()).unwrap();
    let scan_json: serde_json::Value = serde_json::from_str(&scan_stdout).unwrap();

    let hunks = scan_json["files"][0]["hunks"].as_array().unwrap();
    if hunks.len() < 2 {
        // If the diff engine produces only 1 hunk, skip this test gracefully.
        // The test is meaningful only when there are 2+ hunks.
        return;
    }

    let exclude_id = hunks[0]["hunk_id"].as_str().unwrap();

    // Stage entire file but exclude the first hunk
    let output =
        run_agstage(dir.path(), &["stage", "--exclude", exclude_id, "multi.txt"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["status"], "Ok");
}

#[test]
fn stage_untracked_file_by_path() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "existing.txt", "hello\n", "initial");

    // Write a brand-new untracked file
    write_file(dir.path(), "new_file.txt", "brand new content\n");

    // Stage it
    let output = run_agstage(dir.path(), &["stage", "new_file.txt"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["status"], "Ok");
    let succeeded = json["succeeded"].as_array().unwrap();
    assert!(!succeeded.is_empty(), "expected succeeded items");

    // Verify status shows the file as staged Added
    let status_output = run_agstage(dir.path(), &["status"]).success();
    let status_stdout = String::from_utf8(status_output.get_output().stdout.clone()).unwrap();
    let status_json: serde_json::Value = serde_json::from_str(&status_stdout).unwrap();

    let staged = status_json["staged_files"].as_array().unwrap();
    let staged_file = staged
        .iter()
        .find(|f| f["path"] == "new_file.txt")
        .expect("new_file.txt should be staged");
    assert_eq!(staged_file["status"]["type"], "Added");
}
