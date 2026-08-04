mod common;

use common::{commit_file, run_pgs, setup_repo, write_file};

#[test]
fn commit_staged_changes() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "add hello");
    write_file(dir.path(), "hello.txt", "line1\nline2\n");

    // Stage the file
    run_pgs(dir.path(), &["stage", "hello.txt"]).success();

    // Commit with a message
    let output = run_pgs(dir.path(), &["commit", "-m", "feat: add line2"]).success();
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
    run_pgs(dir.path(), &["commit", "-m", "empty commit"]).code(1);
}

#[test]
fn commit_amend_message_only_rewrites_head_message_without_staged_changes() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "old subject");

    let old_head = repo.head().unwrap().peel_to_commit().unwrap();
    let old_head_id = old_head.id();
    let old_tree_id = old_head.tree_id();
    let old_parent_id = old_head.parent_id(0).unwrap();

    let message = "new subject\n\nAdd an explanatory body.";
    let output = run_pgs(dir.path(), &["commit", "--amend", "-m", message]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["message"], message);
    assert_eq!(json["files_changed"], 1);
    assert_eq!(json["insertions"], 1);
    assert_eq!(json["deletions"], 0);

    let amended = repo.head().unwrap().peel_to_commit().unwrap();
    assert_ne!(amended.id(), old_head_id);
    assert_eq!(amended.message().unwrap(), message);
    assert_eq!(amended.tree_id(), old_tree_id);
    assert_eq!(amended.parent_id(0).unwrap(), old_parent_id);
}

#[test]
fn commit_amend_with_staged_changes_replaces_head_tree() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "old subject");

    let old_head = repo.head().unwrap().peel_to_commit().unwrap();
    let old_parent_id = old_head.parent_id(0).unwrap();

    write_file(dir.path(), "hello.txt", "line1\nline2\n");
    run_pgs(dir.path(), &["stage", "hello.txt"]).success();

    let message = "new subject\n\nInclude staged line2.";
    let output = run_pgs(dir.path(), &["commit", "--amend", "-m", message]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["message"], message);
    assert_eq!(json["files_changed"], 1);
    assert_eq!(json["insertions"], 2);
    assert_eq!(json["deletions"], 0);

    let amended = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(amended.message().unwrap(), message);
    assert_eq!(amended.parent_id(0).unwrap(), old_parent_id);

    let tree = amended.tree().unwrap();
    let entry = tree.get_path(std::path::Path::new("hello.txt")).unwrap();
    let blob = repo.find_blob(entry.id()).unwrap();
    assert_eq!(
        std::str::from_utf8(blob.content()).unwrap(),
        "line1\nline2\n"
    );
}

/// Every whitespace-only form of `-m` that must be refused before any mutation.
/// U+00A0 and U+3000 are Unicode `White_Space`, so `str::trim` collapses them too.
const BLANK_MESSAGES: &[&str] = &[
    "", "   ", "\t", "\n\n", " \t \n ", "\r\n", "\r", "\u{a0}", "\u{3000}",
];

#[test]
fn commit_blank_message_returns_exit_code_2_without_committing() {
    for message in BLANK_MESSAGES {
        let (dir, repo) = setup_repo();
        commit_file(&repo, dir.path(), "hello.txt", "line1\n", "add hello");
        write_file(dir.path(), "hello.txt", "line1\nline2\n");
        run_pgs(dir.path(), &["stage", "hello.txt"]).success();

        let head_before = repo.head().unwrap().peel_to_commit().unwrap().id();
        let output = run_pgs(dir.path(), &["commit", "-m", message]).code(2);

        let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
        let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(json["code"], "empty_commit_message", "message: {message:?}");
        assert_eq!(json["exit_code"], 2, "message: {message:?}");

        let head_after = repo.head().unwrap().peel_to_commit().unwrap().id();
        assert_eq!(head_before, head_after, "commit created for {message:?}");
    }
}

#[test]
fn commit_amend_blank_message_returns_exit_code_2_preserving_head_message() {
    for message in BLANK_MESSAGES {
        let (dir, repo) = setup_repo();
        commit_file(&repo, dir.path(), "hello.txt", "line1\n", "old subject");

        let head_before = repo.head().unwrap().peel_to_commit().unwrap().id();
        run_pgs(dir.path(), &["commit", "--amend", "-m", message]).code(2);

        let head_after = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head_before, head_after.id(), "amended for {message:?}");
        assert_eq!(head_after.message().unwrap(), "old subject");
    }
}

#[test]
fn commit_amend_root_commit_blank_message_returns_exit_code_2() {
    let (dir, repo) = setup_repo();
    let root = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(
        root.parent_count(),
        0,
        "fixture HEAD should be the root commit"
    );
    let root_message = root.message().unwrap().to_owned();

    run_pgs(dir.path(), &["commit", "--amend", "-m", "  "]).code(2);

    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.id(), root.id());
    assert_eq!(head.message().unwrap(), root_message);
}

#[test]
fn commit_message_with_surrounding_whitespace_is_accepted_verbatim() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "add hello");
    write_file(dir.path(), "hello.txt", "line1\nline2\n");
    run_pgs(dir.path(), &["stage", "hello.txt"]).success();

    let message = "  feat: padded  ";
    let output = run_pgs(dir.path(), &["commit", "-m", message]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["message"], message);
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.message().unwrap(), message);
}
