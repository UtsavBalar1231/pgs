//! Staging honours git's clean filters, so a repository with
//! `core.autocrlf=true` (the Git for Windows default) stores LF in the index
//! while its checkout holds CRLF, exactly as `git add` would.

mod common;

use common::{commit_file, read_staged_blob, run_pgs, setup_repo, write_file};
use git2::Repository;
use serde_json::Value;
use std::path::Path;

fn staged(dir: &Path) -> String {
    read_staged_blob(&Repository::open(dir).unwrap(), "f.txt")
}

/// LF in HEAD and the index, CRLF in the workdir with line 2 edited.
fn autocrlf_fixture() -> (tempfile::TempDir, Repository) {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "f.txt", "a\nb\nc\n", "add f");
    repo.config()
        .unwrap()
        .set_bool("core.autocrlf", true)
        .unwrap();
    write_file(dir.path(), "f.txt", "a\r\nB\r\nc\r\n");
    (dir, repo)
}

#[test]
fn stage_line_range_under_autocrlf_stores_lf_content() {
    let (dir, _repo) = autocrlf_fixture();

    run_pgs(dir.path(), &["stage", "f.txt:2-2"]).success();

    assert_eq!(staged(dir.path()), "a\nB\nc\n");
}

#[test]
fn stage_hunk_under_autocrlf_stores_lf_content() {
    let (dir, _repo) = autocrlf_fixture();
    let scan = run_pgs(dir.path(), &["scan"]).success();
    let scan: Value = serde_json::from_slice(&scan.get_output().stdout).unwrap();
    let hunk_id = scan["files"][0]["hunks"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    run_pgs(dir.path(), &["stage", &hunk_id]).success();

    assert_eq!(staged(dir.path()), "a\nB\nc\n");
}

#[test]
fn stage_whole_file_under_autocrlf_stores_lf_content() {
    let (dir, _repo) = autocrlf_fixture();

    run_pgs(dir.path(), &["stage", "f.txt"]).success();

    assert_eq!(staged(dir.path()), "a\nB\nc\n");
}

#[test]
fn stage_under_autocrlf_leaves_nothing_unstaged() {
    let (dir, _repo) = autocrlf_fixture();

    run_pgs(dir.path(), &["stage", "f.txt:2-2"]).success();

    // With the filter applied the index matches the checkout, so a rescan
    // finds nothing left to stage.
    run_pgs(dir.path(), &["scan"]).code(1);
}

#[test]
fn line_ending_change_without_autocrlf_is_still_refused() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "f.txt", "a\nb\nc\n", "add f");
    write_file(dir.path(), "f.txt", "a\r\nB\r\nc\r\n");

    let output = run_pgs(dir.path(), &["stage", "f.txt:2-2"]).code(2);
    let error: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();

    assert_eq!(error["code"], "crlf_mismatch");
    assert_eq!(staged(dir.path()), "a\nb\nc\n");
}
