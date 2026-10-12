//! A submodule (gitlink) is a commit pointer, not file content, so pgs cannot
//! build a blob for it. Selecting one is the caller's choice, not a pgs fault.

mod common;

use std::path::Path;
use std::process::Command;

use common::{run_pgs, setup_repo};
use serde_json::Value;

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@test.com"])
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

#[test]
fn stage_modified_submodule_is_an_unsupported_entry() {
    let (dir, _repo) = setup_repo();
    let sub = dir.path().join("sub");
    std::fs::create_dir(&sub).unwrap();
    git(&sub, &["init", "-q"]);
    git(&sub, &["commit", "-q", "--allow-empty", "-m", "one"]);
    git(dir.path(), &["add", "sub"]);
    git(dir.path(), &["commit", "-q", "-m", "add submodule"]);
    git(&sub, &["commit", "-q", "--allow-empty", "-m", "two"]);

    let output = run_pgs(dir.path(), &["stage", "sub"]).code(2);
    let error: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();

    assert_eq!(error["code"], "unsupported_entry");
}
