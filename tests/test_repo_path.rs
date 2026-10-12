//! A repository path that names no git repository is the caller's mistake, so
//! every surface reports it as a user error, never as an internal fault.

mod common;

use assert_cmd::Command;
use common::{call_tool, open_session, shutdown_child, spawn_mcp_stdio};
use serde_json::{Value, json};
use tempfile::TempDir;

#[test]
fn cli_repo_without_git_is_a_user_error() {
    let dir = TempDir::new().unwrap();

    let output = Command::new(assert_cmd::cargo::cargo_bin!("pgs"))
        .args(["--json", "--repo"])
        .arg(dir.path())
        .arg("scan")
        .assert()
        .code(2);
    let error: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();

    assert_eq!(error["code"], "not_a_repository");
}

#[test]
fn mcp_read_and_mutating_tools_report_a_missing_repo_the_same_way() {
    let dir = TempDir::new().unwrap();
    let repo_path = dir.path().join("missing").display().to_string();
    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();
    open_session(&mut stdin, &mut stdout);

    let calls = [
        ("pgs_scan", json!({ "repo_path": repo_path })),
        (
            "pgs_stage",
            json!({ "repo_path": repo_path, "selections": ["f.txt"] }),
        ),
        (
            "pgs_unstage",
            json!({ "repo_path": repo_path, "selections": ["f.txt"] }),
        ),
        (
            "pgs_commit",
            json!({ "repo_path": repo_path, "message": "feat: x" }),
        ),
    ];
    let mut responses = Vec::new();
    for (id, (tool, arguments)) in (2..).zip(calls) {
        responses.push((
            tool,
            call_tool(&mut stdin, &mut stdout, id, tool, &arguments),
        ));
    }
    shutdown_child(child);

    for (tool, response) in responses {
        let result = &response["result"];
        assert_eq!(result["isError"], true, "{tool}: {response}");
        let error = &result["structuredContent"]["pgs_error"];
        assert_eq!(error["code"], "not_a_repository", "{tool}: {response}");
        assert_eq!(error["kind"], "user", "{tool}: {response}");
        assert_eq!(error["exit_code"], 2, "{tool}: {response}");
    }
}
