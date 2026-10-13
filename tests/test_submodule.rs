//! A submodule (gitlink) records a commit, not file content. pgs stages a moved
//! submodule pointer as a whole: the index entry takes the submodule's current
//! HEAD commit, as `git add <submodule>` does. There are no lines to select.

mod common;

use std::path::Path;
use std::process::Command;

use common::{run_pgs, setup_repo};
use git2::{Oid, Repository};
use serde_json::Value;

const GITLINK: u32 = 0o160_000;

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@test.com"])
        .args(["-c", "protocol.file.allow=always"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

/// A superproject tracking `sub` at commit "one", whose checkout has since
/// moved to commit "two". Returns the two commit ids.
fn moved_embedded_submodule(dir: &Path) -> (Oid, Oid) {
    let sub = dir.join("sub");
    std::fs::create_dir(&sub).unwrap();
    git(&sub, &["init", "-q"]);
    git(&sub, &["commit", "-q", "--allow-empty", "-m", "one"]);
    let one = git(&sub, &["rev-parse", "HEAD"]);
    git(dir, &["add", "sub"]);
    git(dir, &["commit", "-q", "-m", "add submodule"]);
    git(&sub, &["commit", "-q", "--allow-empty", "-m", "two"]);
    let two = git(&sub, &["rev-parse", "HEAD"]);
    (Oid::from_str(&one).unwrap(), Oid::from_str(&two).unwrap())
}

fn index_entry(dir: &Path, path: &str) -> Option<(u32, Oid)> {
    let repo = Repository::open(dir).unwrap();
    let index = repo.index().unwrap();
    index
        .get_path(Path::new(path), 0)
        .map(|entry| (entry.mode, entry.id))
}

fn error_code(output: &assert_cmd::assert::Assert) -> String {
    let error: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();
    error["code"].as_str().unwrap_or_default().to_owned()
}

#[test]
fn stage_moved_submodule_records_its_new_commit() {
    let (dir, _repo) = setup_repo();
    let (_, two) = moved_embedded_submodule(dir.path());

    run_pgs(dir.path(), &["stage", "sub"]).success();

    assert_eq!(index_entry(dir.path(), "sub"), Some((GITLINK, two)));
}

#[test]
fn stage_moved_submodule_checked_out_through_a_gitfile() {
    let (dir, _repo) = setup_repo();
    let upstream = tempfile::TempDir::new().unwrap();
    git(upstream.path(), &["init", "-q"]);
    git(
        upstream.path(),
        &["commit", "-q", "--allow-empty", "-m", "one"],
    );
    let url = upstream.path().to_str().unwrap();
    git(dir.path(), &["submodule", "add", "-q", url, "sub"]);
    git(dir.path(), &["commit", "-q", "-m", "add submodule"]);
    let sub = dir.path().join("sub");
    git(&sub, &["commit", "-q", "--allow-empty", "-m", "two"]);
    let two = Oid::from_str(&git(&sub, &["rev-parse", "HEAD"])).unwrap();

    run_pgs(dir.path(), &["stage", "sub"]).success();

    assert_eq!(index_entry(dir.path(), "sub"), Some((GITLINK, two)));
}

#[test]
fn stage_submodule_hunk_or_line_range_is_whole_path_only() {
    let (dir, _repo) = setup_repo();
    moved_embedded_submodule(dir.path());
    let scan = run_pgs(dir.path(), &["scan"]).success();
    let scan: Value = serde_json::from_slice(&scan.get_output().stdout).unwrap();
    let hunk = scan["files"][0]["hunks"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let by_hunk = run_pgs(dir.path(), &["stage", &hunk]).code(2);
    let by_range = run_pgs(dir.path(), &["stage", "sub:1-1"]).code(2);

    assert_eq!(error_code(&by_hunk), "granular_on_whole_file");
    assert_eq!(error_code(&by_range), "granular_on_whole_file");
}

#[test]
fn unstage_submodule_restores_the_head_commit() {
    let (dir, _repo) = setup_repo();
    let (one, _) = moved_embedded_submodule(dir.path());
    git(dir.path(), &["add", "sub"]);

    run_pgs(dir.path(), &["unstage", "sub"]).success();

    assert_eq!(index_entry(dir.path(), "sub"), Some((GITLINK, one)));
}

#[test]
fn commit_with_a_submodule_selection_commits_the_new_pointer() {
    let (dir, _repo) = setup_repo();
    let (_, two) = moved_embedded_submodule(dir.path());

    run_pgs(dir.path(), &["commit", "-m", "chore: bump sub", "sub"]).success();

    let repo = Repository::open(dir.path()).unwrap();
    let tree = repo.head().unwrap().peel_to_tree().unwrap();
    let entry = tree.get_path(Path::new("sub")).unwrap();
    assert_eq!(entry.id(), two);
    assert_eq!(u32::try_from(entry.filemode()).unwrap(), GITLINK);
}

#[test]
fn stage_dry_run_explain_on_a_submodule_previews_no_lines() {
    let (dir, _repo) = setup_repo();
    let (one, _) = moved_embedded_submodule(dir.path());

    let output = run_pgs(dir.path(), &["stage", "--dry-run", "--explain", "sub"]).success();
    let result: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();

    assert_eq!(result["status"], "dry_run");
    assert_eq!(
        result["previews"][0]["preview_lines"],
        serde_json::json!([])
    );
    assert_eq!(index_entry(dir.path(), "sub"), Some((GITLINK, one)));
}

#[test]
fn unstage_a_newly_added_submodule_removes_its_entry() {
    let (dir, _repo) = setup_repo();
    let upstream = tempfile::TempDir::new().unwrap();
    git(upstream.path(), &["init", "-q"]);
    git(
        upstream.path(),
        &["commit", "-q", "--allow-empty", "-m", "one"],
    );
    let url = upstream.path().to_str().unwrap();
    git(dir.path(), &["submodule", "add", "-q", url, "sub"]);

    run_pgs(dir.path(), &["unstage", "sub"]).success();

    assert_eq!(index_entry(dir.path(), "sub"), None);
    assert!(
        index_entry(dir.path(), ".gitmodules").is_some(),
        "unstaging the pointer leaves .gitmodules staged"
    );
}
