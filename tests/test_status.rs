mod common;

use common::{commit_file, run_pgs, setup_repo, write_file};

#[test]
fn status_empty_returns_empty_contract() {
    let (dir, _repo) = setup_repo();

    let output = run_pgs(dir.path(), &["status"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["version"], "v1");
    assert_eq!(json["command"], "status");
    assert!(json.get("staged_files").is_none());

    let files = json["files"].as_array().unwrap();
    assert!(files.is_empty(), "expected empty files");

    assert_eq!(json["summary"]["total_files"], 0);
    assert_eq!(json["summary"]["total_additions"], 0);
    assert_eq!(json["summary"]["total_deletions"], 0);
}

#[test]
fn status_after_stage_shows_file() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "add hello");
    write_file(dir.path(), "hello.txt", "line1\nline2\n");

    // Stage the file
    run_pgs(dir.path(), &["stage", "hello.txt"]).success();

    // Check status
    let output = run_pgs(dir.path(), &["status"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["version"], "v1");
    assert_eq!(json["command"], "status");
    assert!(json.get("staged_files").is_none());

    let files = json["files"].as_array().unwrap();
    assert_eq!(files.len(), 1, "expected one staged file");
    assert_eq!(files[0]["path"], "hello.txt");
    assert_eq!(files[0]["status"]["type"], "Modified");
    assert!(files[0]["lines_added"].as_u64().unwrap() > 0);

    assert_eq!(json["summary"]["total_files"], 1);
    assert!(json["summary"]["total_additions"].as_u64().unwrap() > 0);
}
