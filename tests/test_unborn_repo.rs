//! A repository with no commits yet: no HEAD, and no `.git/index` until the
//! first file is staged. Every command must work there, since that is where a
//! new project's first commit is built.

mod common;

use assert_cmd::Command;
use git2::Repository;
use serde_json::Value;
use tempfile::TempDir;

use common::{run_pgs, write_file};

fn unborn_repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    let mut config = repo.config().unwrap();
    config.set_str("user.name", "Test").unwrap();
    config.set_str("user.email", "test@test.com").unwrap();
    config.set_bool("core.autocrlf", false).unwrap();
    drop(config);
    drop(repo);
    write_file(dir.path(), "f.txt", "a\nb\n");
    dir
}

fn json(output: &assert_cmd::assert::Assert) -> Value {
    serde_json::from_slice(&output.get_output().stdout).unwrap()
}

#[test]
fn stage_status_and_commit_in_a_repo_with_no_commits_build_a_root_commit() {
    let dir = unborn_repo();

    run_pgs(dir.path(), &["stage", "f.txt"]).success();
    let status = json(&run_pgs(dir.path(), &["status"]).success());
    assert_eq!(status["files"][0]["path"], "f.txt");
    run_pgs(dir.path(), &["overview"]).success();

    let commit = json(&run_pgs(dir.path(), &["commit", "-m", "feat: first"]).success());
    assert_eq!(commit["files_changed"], 1);

    let repo = Repository::open(dir.path()).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.parent_count(), 0);
    assert_eq!(head.message().unwrap(), "feat: first\n");
}

#[test]
fn unstage_in_a_repo_with_no_commits_removes_the_staged_file() {
    let dir = unborn_repo();
    run_pgs(dir.path(), &["stage", "f.txt"]).success();

    run_pgs(dir.path(), &["unstage", "f.txt"]).success();

    let repo = Repository::open(dir.path()).unwrap();
    assert!(repo.index().unwrap().is_empty());
}

#[test]
fn amend_in_a_repo_with_no_commits_is_a_user_error() {
    let dir = unborn_repo();
    run_pgs(dir.path(), &["stage", "f.txt"]).success();

    let output = Command::new(assert_cmd::cargo::cargo_bin!("pgs"))
        .args(["--json", "--repo"])
        .arg(dir.path())
        .args(["commit", "--amend", "-m", "feat: x"])
        .assert()
        .code(2);

    assert_eq!(json(&output)["code"], "invalid_arguments");
}

#[test]
fn commit_with_nothing_staged_in_a_repo_with_no_commits_has_no_effect() {
    let dir = unborn_repo();

    let output = run_pgs(dir.path(), &["commit", "-m", "feat: empty"]).code(1);

    assert_eq!(json(&output)["code"], "no_changes");
}
