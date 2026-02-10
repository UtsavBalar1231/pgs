mod common;

use common::{commit_file, run_agstage, setup_repo, write_file};

#[test]
fn unstage_file_restores_to_head() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "add hello");
    write_file(dir.path(), "hello.txt", "line1\nline2\n");

    // Stage the file
    run_agstage(dir.path(), &["stage", "hello.txt"]).success();

    // Verify it is staged
    let status_output = run_agstage(dir.path(), &["status"]).success();
    let status_stdout = String::from_utf8(status_output.get_output().stdout.clone()).unwrap();
    let status_json: serde_json::Value = serde_json::from_str(&status_stdout).unwrap();
    assert!(
        !status_json["staged_files"].as_array().unwrap().is_empty(),
        "file should be staged before unstage"
    );

    // Unstage the file
    run_agstage(dir.path(), &["unstage", "hello.txt"]).success();

    // Verify status shows nothing staged
    let status_output2 = run_agstage(dir.path(), &["status"]).success();
    let status_stdout2 = String::from_utf8(status_output2.get_output().stdout.clone()).unwrap();
    let status_json2: serde_json::Value = serde_json::from_str(&status_stdout2).unwrap();
    let staged = status_json2["staged_files"].as_array().unwrap();
    assert!(staged.is_empty(), "after unstage, nothing should be staged");
}

#[test]
fn unstage_dry_run_keeps_staged() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "add hello");
    write_file(dir.path(), "hello.txt", "line1\nline2\n");

    // Stage the file
    run_agstage(dir.path(), &["stage", "hello.txt"]).success();

    // Unstage with --dry-run
    let output = run_agstage(dir.path(), &["unstage", "--dry-run", "hello.txt"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["status"], "DryRun");

    // Verify file is still staged
    let status_output = run_agstage(dir.path(), &["status"]).success();
    let status_stdout = String::from_utf8(status_output.get_output().stdout.clone()).unwrap();
    let status_json: serde_json::Value = serde_json::from_str(&status_stdout).unwrap();
    let staged = status_json["staged_files"].as_array().unwrap();
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
    run_agstage(dir.path(), &["stage", "hello.txt"]).success();

    // Try to unstage a nonexistent hunk ID
    run_agstage(dir.path(), &["unstage", "deadbeef0000"]).code(2);
}
