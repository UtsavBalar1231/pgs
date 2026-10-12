//! `plan-apply` turns a `CommitPlan` into commits, one per planned commit, in
//! order. It refuses before writing anything when the plan cannot finish, and
//! when a later commit fails it reports the commits already created.

mod common;

use std::path::Path;

use common::{
    call_tool, commit_file, open_session, run_pgs, run_pgs_stdin, setup_repo, shutdown_child,
    spawn_mcp_stdio, write_file,
};
use git2::Repository;
use serde_json::{Value, json};

fn head_messages(dir: &Path) -> Vec<String> {
    let repo = Repository::open(dir).unwrap();
    let mut walk = repo.revwalk().unwrap();
    walk.push_head().unwrap();
    walk.map(|oid| {
        let commit = repo.find_commit(oid.unwrap()).unwrap();
        commit.message().unwrap().to_owned()
    })
    .collect()
}

fn file_at(dir: &Path, rev: &str, path: &str) -> String {
    let repo = Repository::open(dir).unwrap();
    let tree = repo.revparse_single(rev).unwrap().peel_to_tree().unwrap();
    let blob = tree
        .get_path(Path::new(path))
        .unwrap()
        .to_object(&repo)
        .unwrap();
    String::from_utf8(blob.peel_to_blob().unwrap().content().to_vec()).unwrap()
}

fn staged_count(dir: &Path) -> usize {
    let repo = Repository::open(dir).unwrap();
    let head = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
    repo.diff_tree_to_index(head.as_ref(), None, None)
        .unwrap()
        .deltas()
        .count()
}

fn apply(dir: &Path, plan: &Value, extra: &[&str]) -> (Option<i32>, Value) {
    let mut args = vec!["plan-apply"];
    args.extend_from_slice(extra);
    let output = run_pgs_stdin(dir, &args, &plan.to_string());
    let output = output.get_output();
    (
        output.status.code(),
        serde_json::from_slice(&output.stdout).unwrap(),
    )
}

fn hunk_ids(dir: &Path) -> Vec<String> {
    let scan = run_pgs(dir, &["scan"]).success();
    let scan: Value = serde_json::from_slice(&scan.get_output().stdout).unwrap();
    scan["files"][0]["hunks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["id"].as_str().unwrap().to_owned())
        .collect()
}

fn two_files() -> (tempfile::TempDir, Repository) {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "a.txt", "a\n", "add a");
    commit_file(&repo, dir.path(), "b.txt", "b\n", "add b");
    write_file(dir.path(), "a.txt", "a\nA\n");
    write_file(dir.path(), "b.txt", "b\nB\n");
    (dir, repo)
}

fn two_commit_plan() -> Value {
    json!({"commits": [
        {"id": "c1", "selections": ["a.txt"], "message": "feat: a"},
        {"id": "c2", "selections": ["b.txt"], "message": "feat: b"}
    ]})
}

/// Twenty numbered lines, edited near the top and near the bottom, so the file
/// has two hunks and the first one shifts the index-side start of the second.
fn two_hunk_file() -> (tempfile::TempDir, Repository, String) {
    let (dir, repo) = setup_repo();
    let base = (1..=20).fold(String::new(), |base, n| base + &format!("l{n}\n"));
    commit_file(&repo, dir.path(), "f.txt", &base, "add f");
    let edited = base
        .replace("l2\n", "l2\nnew1\nnew2\n")
        .replace("l18\n", "L18\n");
    write_file(dir.path(), "f.txt", &edited);
    (dir, repo, edited)
}

#[test]
fn plan_apply_creates_one_commit_per_planned_commit_in_order() {
    let (dir, _repo) = two_files();

    let (code, out) = apply(dir.path(), &two_commit_plan(), &[]);

    assert_eq!(code, Some(0), "{out}");
    assert_eq!(out["status"], "applied");
    assert_eq!(out["commits"][0]["id"], "c1");
    assert_eq!(out["commits"][1]["staged"][0]["selection"], "b.txt");
    assert_eq!(
        head_messages(dir.path())[..2],
        ["feat: b\n".to_owned(), "feat: a\n".to_owned()]
    );
    assert_eq!(file_at(dir.path(), "HEAD~1", "b.txt"), "b\n");
    assert_eq!(file_at(dir.path(), "HEAD", "b.txt"), "b\nB\n");
}

#[test]
fn plan_apply_follows_a_hunk_id_shifted_by_an_earlier_commit() {
    let (dir, _repo, edited) = two_hunk_file();
    let ids = hunk_ids(dir.path());
    assert_eq!(ids.len(), 2, "fixture must produce two hunks");
    let plan = json!({"commits": [
        {"id": "top", "selections": [ids[0]], "message": "feat: top"},
        {"id": "bottom", "selections": [ids[1]], "message": "feat: bottom"}
    ]});

    let (code, out) = apply(dir.path(), &plan, &[]);

    assert_eq!(code, Some(0), "{out}");
    assert!(file_at(dir.path(), "HEAD~1", "f.txt").contains("new1\nnew2\n"));
    assert!(file_at(dir.path(), "HEAD~1", "f.txt").contains("l18\n"));
    assert_eq!(file_at(dir.path(), "HEAD", "f.txt"), edited);
}

#[test]
fn plan_apply_ignores_an_exclude_whose_hunk_an_earlier_commit_took() {
    let (dir, _repo, edited) = two_hunk_file();
    let ids = hunk_ids(dir.path());
    let plan = json!({"commits": [
        {"selections": [ids[0]], "message": "feat: top"},
        {"selections": ["f.txt"], "exclude": [ids[0]], "message": "feat: rest"}
    ]});

    let (code, out) = apply(dir.path(), &plan, &[]);

    assert_eq!(code, Some(0), "{out}");
    assert_eq!(file_at(dir.path(), "HEAD", "f.txt"), edited);
}

#[test]
fn plan_apply_dry_run_reports_each_commit_and_writes_nothing() {
    let (dir, _repo) = two_files();
    let before = head_messages(dir.path());

    let (code, out) = apply(dir.path(), &two_commit_plan(), &["--dry-run"]);

    assert_eq!(code, Some(0), "{out}");
    assert_eq!(out["status"], "dry_run");
    assert_eq!(out["commits"][1]["staged"][0]["lines_affected"], 1);
    assert!(out["commits"][0]["commit_hash"].is_null(), "{out}");
    assert_eq!(head_messages(dir.path()), before);
    assert_eq!(staged_count(dir.path()), 0);
}

#[test]
fn plan_apply_refuses_when_something_is_already_staged() {
    let (dir, _repo) = two_files();
    run_pgs(dir.path(), &["stage", "a.txt"]).success();
    let before = head_messages(dir.path());

    let (code, out) = apply(dir.path(), &two_commit_plan(), &[]);

    assert_eq!(code, Some(2), "{out}");
    assert_eq!(out["code"], "invalid_arguments");
    assert_eq!(head_messages(dir.path()), before);
}

#[test]
fn plan_apply_refuses_a_planned_commit_without_a_message() {
    let (dir, _repo) = two_files();
    let plan = json!({"commits": [
        {"selections": ["a.txt"], "message": "feat: a"},
        {"selections": ["b.txt"]}
    ]});

    let (code, out) = apply(dir.path(), &plan, &[]);

    assert_eq!(code, Some(2), "{out}");
    assert!(out["message"].as_str().unwrap().contains("#2"), "{out}");
    assert_eq!(head_messages(dir.path()).len(), 3);
}

#[test]
fn plan_apply_refuses_overlapping_commits() {
    let (dir, _repo) = two_files();
    let plan = json!({"commits": [
        {"selections": ["a.txt"], "message": "feat: a"},
        {"selections": ["a.txt", "b.txt"], "message": "feat: ab"}
    ]});

    let (code, out) = apply(dir.path(), &plan, &[]);

    assert_eq!(code, Some(2), "{out}");
    assert!(
        out["message"].as_str().unwrap().contains("1 overlap"),
        "{out}"
    );
    assert_eq!(head_messages(dir.path()).len(), 3);
}

#[test]
fn plan_apply_refuses_a_selection_that_names_no_change_before_committing() {
    let (dir, _repo) = two_files();
    // Line 1 of b.txt is unchanged context inside its hunk.
    let plan = json!({"commits": [
        {"selections": ["a.txt"], "message": "feat: a"},
        {"selections": ["b.txt:1-1"], "message": "feat: b"}
    ]});

    let (code, out) = apply(dir.path(), &plan, &[]);

    assert_eq!(code, Some(1), "{out}");
    assert_eq!(out["code"], "selection_empty");
    assert_eq!(head_messages(dir.path()).len(), 3);
}

#[test]
fn plan_apply_refuses_with_commit_hooks_unless_no_verify() {
    let (dir, _repo) = two_files();
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

    let (code, out) = apply(dir.path(), &two_commit_plan(), &[]);
    assert_eq!(code, Some(2), "{out}");
    assert_eq!(out["code"], "commit_hooks_present");

    let (code, out) = apply(dir.path(), &two_commit_plan(), &["--no-verify"]);
    assert_eq!(code, Some(0), "{out}");
    assert_eq!(head_messages(dir.path()).len(), 5);
}

#[test]
fn plan_apply_failing_partway_keeps_earlier_commits_and_reports_them() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "a.txt", "a\n", "add a");
    // No trailing newline: staging only the appended line would have to
    // terminate `x` too, which the range never named, so pgs refuses it at
    // write time.
    commit_file(&repo, dir.path(), "b.txt", "x", "add b");
    write_file(dir.path(), "a.txt", "a\nA\n");
    write_file(dir.path(), "b.txt", "x\ny\n");
    let plan = json!({"commits": [
        {"id": "c1", "selections": ["a.txt"], "message": "feat: a"},
        {"id": "c2", "selections": ["b.txt:2-2"], "message": "feat: b"}
    ]});

    let (code, out) = apply(dir.path(), &plan, &[]);

    assert_eq!(code, Some(2), "{out}");
    assert_eq!(out["status"], "partial");
    assert_eq!(out["commits"].as_array().unwrap().len(), 1);
    assert!(out["commits"][0]["commit_hash"].is_string(), "{out}");
    assert_eq!(out["failed"]["id"], "c2");
    assert_eq!(out["failed"]["code"], "unterminated_interior_line");
    assert_eq!(head_messages(dir.path())[0], "feat: a\n");
    assert_eq!(staged_count(dir.path()), 0, "failed step must roll back");
}

#[test]
fn plan_apply_text_mode_emits_one_marker_per_commit() {
    let (dir, _repo) = two_files();
    let output = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("pgs"))
        .args(["--repo", dir.path().to_str().unwrap(), "plan-apply"])
        .write_stdin(two_commit_plan().to_string())
        .assert()
        .success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    let kinds: Vec<&str> = stdout
        .lines()
        .filter_map(|l| l.strip_prefix("@@pgs:v1 "))
        .filter_map(|l| l.split(' ').next())
        .collect();
    assert_eq!(
        kinds,
        [
            "plan.apply.begin",
            "plan.apply.commit",
            "plan.apply.commit",
            "plan.apply.end"
        ]
    );
}

#[test]
fn mcp_plan_apply_tool_commits_the_plan() {
    let (dir, _repo) = two_files();

    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();
    open_session(&mut stdin, &mut stdout);
    let response = call_tool(
        &mut stdin,
        &mut stdout,
        2,
        "pgs_plan_apply",
        &json!({
            "repo_path": dir.path().display().to_string(),
            "plan": two_commit_plan()
        }),
    );
    shutdown_child(child);

    let result = &response["result"];
    assert_eq!(result["isError"], false, "{response}");
    assert_eq!(result["structuredContent"]["pgs"]["status"], "applied");
    assert_eq!(head_messages(dir.path())[0], "feat: b\n");
}
