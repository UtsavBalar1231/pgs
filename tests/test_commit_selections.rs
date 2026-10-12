//! `commit <selections>` stages the selections and commits in one call, and
//! rolls the staging back if the commit fails, so the index never holds a
//! half-finished step.

mod common;

use std::path::Path;

use common::{
    call_tool, commit_file, open_session, run_pgs, setup_repo, shutdown_child, spawn_mcp_stdio,
    write_file,
};
use git2::Repository;
use serde_json::{Value, json};

fn staged_paths(dir: &Path) -> Vec<String> {
    let repo = Repository::open(dir).unwrap();
    let head = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
    let diff = repo.diff_tree_to_index(head.as_ref(), None, None).unwrap();
    diff.deltas()
        .filter_map(|d| d.new_file().path().map(|p| p.display().to_string()))
        .collect()
}

#[test]
fn commit_with_selections_commits_only_those_changes() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "a.txt", "a\n", "add a");
    commit_file(&repo, dir.path(), "b.txt", "b\n", "add b");
    write_file(dir.path(), "a.txt", "a\nA\n");
    write_file(dir.path(), "b.txt", "b\nB\n");

    let output = run_pgs(dir.path(), &["commit", "-m", "feat: a", "a.txt"]).success();
    let result: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();

    assert_eq!(result["files_changed"], 1);
    assert_eq!(result["staged"][0]["selection"], "a.txt");
    assert_eq!(result["staged"][0]["lines_affected"], 1);
    let head = Repository::open(dir.path()).unwrap();
    let head = head.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.message().unwrap(), "feat: a\n");
    let scan = run_pgs(dir.path(), &["scan"]).success();
    let scan: Value = serde_json::from_slice(&scan.get_output().stdout).unwrap();
    assert_eq!(scan["files"][0]["path"], "b.txt");
}

#[test]
fn commit_failing_after_its_staging_restores_the_index() {
    let dir = tempfile::TempDir::new().unwrap();
    {
        let repo = Repository::init(dir.path()).unwrap();
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test").unwrap();
        config.set_str("user.email", "test@test.com").unwrap();
        config.set_bool("core.autocrlf", false).unwrap();
    }
    write_file(dir.path(), "f.txt", "f\n");

    // Staging succeeds; amending a branch with no commits then fails.
    let output = run_pgs(dir.path(), &["commit", "--amend", "-m", "feat: f", "f.txt"]).code(2);
    let error: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();

    assert_eq!(error["code"], "invalid_arguments");
    assert!(
        staged_paths(dir.path()).is_empty(),
        "{:?}",
        staged_paths(dir.path())
    );
}

#[test]
fn commit_refused_for_hooks_stages_nothing() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "f.txt", "f\n", "add f");
    write_file(dir.path(), "f.txt", "f\ng\n");
    let hooks = dir.path().join(".git/hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    std::fs::write(hooks.join("pre-commit"), "#!/bin/sh\nexit 0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            hooks.join("pre-commit"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }

    run_pgs(dir.path(), &["commit", "-m", "feat: g", "f.txt"]).code(2);

    assert!(
        staged_paths(dir.path()).is_empty(),
        "{:?}",
        staged_paths(dir.path())
    );
}

#[test]
fn mcp_commit_tool_with_selections_stages_and_commits() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "a.txt", "a\n", "add a");
    write_file(dir.path(), "a.txt", "a\nA\n");
    write_file(dir.path(), "new.txt", "n\n");

    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();
    open_session(&mut stdin, &mut stdout);
    let response = call_tool(
        &mut stdin,
        &mut stdout,
        2,
        "pgs_commit",
        &json!({
            "repo_path": dir.path().display().to_string(),
            "message": "feat: a",
            "selections": ["a.txt"]
        }),
    );
    shutdown_child(child);

    let pgs = &response["result"]["structuredContent"]["pgs"];
    assert_eq!(pgs["files_changed"], 1, "{response}");
    assert_eq!(pgs["staged"][0]["selection"], "a.txt");
}
