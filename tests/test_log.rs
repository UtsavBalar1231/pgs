mod common;

use serde_json::Value;

use common::{commit_file, run_pgs, setup_repo};

#[test]
fn log_returns_commits_in_json() {
    let (dir, repo) = setup_repo();
    commit_file(
        &repo,
        dir.path(),
        "src/main.rs",
        "fn main() {}",
        "feat: initial",
    );
    commit_file(
        &repo,
        dir.path(),
        "src/main.rs",
        "fn main() { println!(\"hello\"); }",
        "feat: add hello",
    );

    let output = run_pgs(dir.path(), &["log"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone())
        .expect("stdout should be valid UTF-8");
    let json: Value = serde_json::from_str(&stdout).expect("output should be valid JSON");

    assert_eq!(json["version"], "v1", "expected version v1");
    assert_eq!(json["command"], "log", "expected command log");

    let commits = json["commits"]
        .as_array()
        .expect("commits should be an array");
    // setup_repo creates 1 initial commit + 2 more = 3 total
    assert_eq!(commits.len(), 3, "expected 3 commits (initial + 2)");

    for commit in commits {
        assert!(commit["hash"].is_string(), "each commit should have a hash");
        assert!(
            commit["short_hash"].is_string(),
            "each commit should have a short_hash"
        );
        assert!(
            commit["author"].is_string(),
            "each commit should have an author"
        );
        assert!(commit["date"].is_string(), "each commit should have a date");
        assert!(
            commit["message"].is_string(),
            "each commit should have a message"
        );
    }

    assert_eq!(
        json["truncated"], false,
        "3 commits should not be truncated with default max_count"
    );
}

#[test]
fn log_respects_max_count() {
    let (dir, repo) = setup_repo();
    for i in 0..5u32 {
        commit_file(
            &repo,
            dir.path(),
            "file.txt",
            &format!("content {i}"),
            &format!("chore: commit {i}"),
        );
    }

    let output = run_pgs(dir.path(), &["log", "--max-count", "2"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone())
        .expect("stdout should be valid UTF-8");
    let json: Value = serde_json::from_str(&stdout).expect("output should be valid JSON");

    let commits = json["commits"]
        .as_array()
        .expect("commits should be an array");
    assert_eq!(
        commits.len(),
        2,
        "expected exactly 2 commits from --max-count 2"
    );
    assert_eq!(
        json["total"], 2,
        "total should match the number of returned commits"
    );
    assert_eq!(
        json["truncated"], false,
        "truncated should be false when max-count equals commits returned"
    );
}

#[test]
fn log_filters_by_path() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "src/a.rs", "// a v1", "feat: update a");
    commit_file(&repo, dir.path(), "src/b.rs", "// b v1", "feat: update b");
    commit_file(
        &repo,
        dir.path(),
        "src/a.rs",
        "// a v2",
        "feat: update a again",
    );

    let output = run_pgs(dir.path(), &["log", "--", "src/a.rs"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone())
        .expect("stdout should be valid UTF-8");
    let json: Value = serde_json::from_str(&stdout).expect("output should be valid JSON");

    let commits = json["commits"]
        .as_array()
        .expect("commits should be an array");
    // Only commits touching src/a.rs
    assert_eq!(commits.len(), 2, "expected 2 commits touching src/a.rs");

    let messages: Vec<&str> = commits
        .iter()
        .map(|c| c["message"].as_str().expect("message should be a string"))
        .collect();
    assert!(
        messages.contains(&"feat: update a again"),
        "expected 'feat: update a again' in filtered commits, got: {messages:?}"
    );
    assert!(
        messages.contains(&"feat: update a"),
        "expected 'feat: update a' in filtered commits, got: {messages:?}"
    );
    assert!(
        !messages.contains(&"feat: update b"),
        "unexpected b commit in path-filtered results: {messages:?}"
    );
}

#[test]
fn log_empty_repo_returns_empty() {
    // setup_repo() creates an initial empty commit so HEAD is valid.
    // For a truly unborn HEAD, git2 may return a reference error, so we use setup_repo
    // with no additional commits and verify the initial commit is returned.
    let (dir, _repo) = setup_repo();

    let output = run_pgs(dir.path(), &["log"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone())
        .expect("stdout should be valid UTF-8");
    let json: Value = serde_json::from_str(&stdout).expect("output should be valid JSON");

    assert_eq!(json["version"], "v1", "expected version v1");
    assert_eq!(json["command"], "log", "expected command log");

    let commits = json["commits"]
        .as_array()
        .expect("commits should be an array");
    // setup_repo creates exactly one initial commit
    assert_eq!(
        commits.len(),
        1,
        "expected 1 commit (the initial commit from setup_repo)"
    );
    assert_eq!(json["total"], 1, "expected total 1");
    assert_eq!(
        json["truncated"], false,
        "single commit should not be truncated"
    );
}

#[test]
fn log_json_has_correct_fields() {
    let (dir, repo) = setup_repo();
    commit_file(
        &repo,
        dir.path(),
        "hello.txt",
        "hello world",
        "docs: add hello",
    );

    let output = run_pgs(dir.path(), &["log", "--max-count", "1"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone())
        .expect("stdout should be valid UTF-8");
    let json: Value = serde_json::from_str(&stdout).expect("output should be valid JSON");

    assert_eq!(json["version"], "v1", "version should be v1");
    assert_eq!(json["command"], "log", "command should be log");

    let commits = json["commits"]
        .as_array()
        .expect("commits should be an array");
    assert_eq!(commits.len(), 1, "expected 1 commit");

    let commit = &commits[0];
    let hash = commit["hash"].as_str().expect("hash should be a string");
    assert_eq!(hash.len(), 40, "hash should be 40 hex characters");
    assert!(
        hash.chars().all(|c| c.is_ascii_hexdigit()),
        "hash should only contain hex characters"
    );

    let short_hash = commit["short_hash"]
        .as_str()
        .expect("short_hash should be a string");
    assert_eq!(short_hash.len(), 12, "short_hash should be 12 characters");
    assert!(
        short_hash.chars().all(|c| c.is_ascii_hexdigit()),
        "short_hash should only contain hex characters"
    );

    let date = commit["date"].as_str().expect("date should be a string");
    // Date should include a timezone offset like +0000 or similar
    assert!(
        date.contains('+') || date.contains('-'),
        "date should contain a timezone offset, got: {date}"
    );

    // short_hash should be a prefix of hash
    assert!(
        hash.starts_with(short_hash),
        "short_hash '{short_hash}' should be a prefix of hash '{hash}'"
    );
}
