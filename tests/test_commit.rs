mod common;

use common::{commit_file, run_agstage, setup_repo, write_file};

#[test]
fn commit_staged_changes() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "add hello");
    write_file(dir.path(), "hello.txt", "line1\nline2\n");

    // Stage the file
    run_agstage(dir.path(), &["stage", "hello.txt"]).success();

    // Commit with a message
    let output = run_agstage(dir.path(), &["commit", "-m", "feat: add line2"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["version"], "v1");
    assert_eq!(json["command"], "commit");
    assert!(
        json["commit_hash"].is_string(),
        "expected commit_hash string"
    );
    let hash = json["commit_hash"].as_str().unwrap();
    assert_eq!(hash.len(), 40, "commit hash should be 40 hex characters");

    assert_eq!(json["message"], "feat: add line2");
    assert!(json["author"].as_str().unwrap().contains("Test"));
    assert_eq!(json["files_changed"], 1);
    assert_eq!(json["insertions"], 1);
    assert_eq!(json["deletions"], 0);
}

#[test]
fn commit_nothing_staged_returns_exit_code_1() {
    let (dir, _repo) = setup_repo();

    // No staged changes — should exit 1.
    run_agstage(dir.path(), &["commit", "-m", "empty commit"]).code(1);
}
