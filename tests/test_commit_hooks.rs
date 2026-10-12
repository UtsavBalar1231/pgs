//! libgit2 runs no hooks, so a commit through pgs would silently skip the
//! pre-commit, prepare-commit-msg and commit-msg hooks `git commit` runs (secret
//! scanners, linters, message checks). pgs refuses instead, unless the caller
//! opts out with `--no-verify` as `git commit --no-verify` would.

mod common;

use std::path::Path;

use common::{commit_file, run_pgs, setup_repo, write_file};
use serde_json::Value;

fn write_hook(dir: &Path, name: &str, executable: bool) {
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if executable { 0o755 } else { 0o644 };
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
    }
    #[cfg(not(unix))]
    let _ = executable;
}

fn staged_change() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "f.txt", "a\n", "add f");
    write_file(dir.path(), "f.txt", "a\nb\n");
    run_pgs(dir.path(), &["stage", "f.txt"]).success();
    (dir, repo)
}

#[test]
fn commit_with_an_active_pre_commit_hook_is_refused() {
    let (dir, _repo) = staged_change();
    write_hook(&dir.path().join(".git/hooks"), "pre-commit", true);

    let output = run_pgs(dir.path(), &["commit", "-m", "feat: b"]).code(2);
    let error: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();

    assert_eq!(error["code"], "commit_hooks_present");
    assert!(
        error["message"].as_str().unwrap().contains("pre-commit"),
        "{error}"
    );
}

#[test]
fn commit_with_no_verify_commits_despite_hooks() {
    let (dir, repo) = staged_change();
    write_hook(&dir.path().join(".git/hooks"), "commit-msg", true);

    run_pgs(dir.path(), &["commit", "-m", "feat: b", "--no-verify"]).success();

    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.message().unwrap(), "feat: b\n");
}

#[test]
fn commit_honours_core_hooks_path() {
    let (dir, repo) = staged_change();
    repo.config()
        .unwrap()
        .set_str("core.hooksPath", ".githooks")
        .unwrap();
    write_hook(&dir.path().join(".githooks"), "prepare-commit-msg", true);

    let output = run_pgs(dir.path(), &["commit", "-m", "feat: b"]).code(2);
    let error: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();

    assert_eq!(error["code"], "commit_hooks_present");
}

#[test]
fn commit_ignores_sample_and_unrelated_hooks() {
    let (dir, _repo) = staged_change();
    let hooks = dir.path().join(".git/hooks");
    write_hook(&hooks, "pre-commit.sample", true);
    write_hook(&hooks, "post-checkout", true);

    run_pgs(dir.path(), &["commit", "-m", "feat: b"]).success();
}

#[cfg(unix)]
#[test]
fn commit_ignores_a_hook_git_would_not_run_because_it_is_not_executable() {
    let (dir, _repo) = staged_change();
    write_hook(&dir.path().join(".git/hooks"), "pre-commit", false);

    run_pgs(dir.path(), &["commit", "-m", "feat: b"]).success();
}
