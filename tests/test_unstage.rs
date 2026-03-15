mod common;

use common::{commit_file, run_pgs, setup_repo, write_file};

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
