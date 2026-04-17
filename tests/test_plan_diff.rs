//! Tests for `pgs plan-diff` and the `pgs_plan_diff` MCP tool. A `CommitPlan`
//! is agent-supplied input listing planned commits; plan-diff runs a fresh
//! scan and classifies each entry as `still_valid`, `shifted`, or `gone`
//! relative to the current workdir state.
//!
//! A6 extends `CommitPlan` / `PlannedCommit` additively via `#[serde(default)]`
//! fields (no schema version bump).

mod common;

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use common::{commit_file, run_pgs, setup_repo, write_file};
use serde_json::{Value, json};

// ─── CLI RED tests ────────────────────────────────────────────────────────────

/// Helper: spawn `pgs plan-diff --stdin` with `plan_json` piped on stdin.
/// Returns (exit code, stdout, stderr).
fn run_plan_diff_stdin(
    dir: &std::path::Path,
    plan_json: &str,
    extra_args: &[&str],
) -> (i32, String, String) {
    let mut cmd = std::process::Command::new(assert_cmd::cargo::cargo_bin!("pgs"))
        .arg("--json")
        .arg("--repo")
        .arg(dir.to_str().unwrap())
        .args(extra_args)
        .args(["plan-diff", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn pgs plan-diff --stdin");

    {
        let mut stdin = cmd.stdin.take().expect("plan-diff stdin piped");
        stdin
            .write_all(plan_json.as_bytes())
            .expect("write plan JSON to stdin");
    }

    let output = cmd.wait_with_output().expect("wait for plan-diff");
    let code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8(output.stdout).unwrap_or_default();
    let stderr = String::from_utf8(output.stderr).unwrap_or_default();
    (code, stdout, stderr)
}

fn scan_first_hunk_id(dir: &std::path::Path) -> String {
    let scan = run_pgs(dir, &["scan"]).success();
    let scan_json: Value =
        serde_json::from_slice(&scan.get_output().stdout).expect("scan JSON parses");
    scan_json["files"][0]["hunks"][0]["id"]
        .as_str()
        .expect("scan exposes at least one hunk id")
        .to_owned()
}

/// An unchanged workdir relative to the plan must classify every entry as
/// `still_valid` — no `shifted`, no `gone`.
///
/// Expected RED failure: `pgs plan-diff` is not a registered subcommand, so
/// clap returns `InvalidSubcommand` before the JSON envelope can be inspected.
#[test]
fn plan_diff_unchanged_scan_reports_all_entries_still_valid() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "f.rs", "one\ntwo\n", "initial");
    write_file(dir.path(), "f.rs", "one\ntwo\nthree\n");

    let hunk_id = scan_first_hunk_id(dir.path());
    let plan = json!({
        "version": "v1",
        "commits": [
            { "id": "c1", "selections": [hunk_id.clone()] }
        ]
    });

    let (code, stdout, stderr) = run_plan_diff_stdin(dir.path(), &plan.to_string(), &[]);
    assert_eq!(code, 0, "clean plan-diff must exit 0. stderr: {stderr}");

    let envelope: Value = serde_json::from_str(&stdout).expect("plan-diff emits JSON envelope");
    assert_eq!(envelope["version"], "v1");
    assert_eq!(envelope["command"], "plan-diff");

    let still_valid = envelope["still_valid"]
        .as_array()
        .expect("still_valid must be an array");
    assert_eq!(
        still_valid.len(),
        1,
        "all entries should be still_valid, got: {envelope}"
    );
    assert!(
        envelope["shifted"].as_array().unwrap().is_empty(),
        "shifted must be empty when tree is unchanged"
    );
    assert!(
        envelope["gone"].as_array().unwrap().is_empty(),
        "gone must be empty when tree is unchanged"
    );
}

/// After landing a commit that consumes the hunks a plan entry references,
/// `plan-diff` must classify the entry as `gone` with reason
/// `covered_by_commit` (no hunks remain for that path in the fresh scan).
///
/// Expected RED failure: no `plan-diff` subcommand → exit 2.
#[test]
fn plan_diff_after_commit_reports_covered_entries_as_gone() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "f.rs", "one\ntwo\n", "initial");
    write_file(dir.path(), "f.rs", "one\ntwo\nthree\n");

    let hunk_id = scan_first_hunk_id(dir.path());
    let plan = json!({
        "version": "v1",
        "commits": [
            { "id": "c1", "selections": [hunk_id.clone()] }
        ]
    });

    // Stage and commit the referenced hunk so a fresh scan reports no hunks
    // for f.rs.
    run_pgs(dir.path(), &["stage", &hunk_id]).success();
    run_pgs(dir.path(), &["commit", "-m", "landed the hunk"]).success();

    let (code, stdout, stderr) = run_plan_diff_stdin(dir.path(), &plan.to_string(), &[]);
    assert_eq!(
        code, 1,
        "gone entries must surface as exit code 1. stderr: {stderr}"
    );

    let envelope: Value = serde_json::from_str(&stdout).expect("plan-diff emits JSON envelope");
    let gone = envelope["gone"].as_array().expect("gone must be an array");
    assert_eq!(
        gone.len(),
        1,
        "consumed hunk must appear as gone, envelope: {envelope}"
    );
    let reason = gone[0]["reason"]
        .as_str()
        .expect("gone entry must expose a reason");
    assert_eq!(
        reason, "covered_by_commit",
        "consumed-by-commit should be reason `covered_by_commit`"
    );
}

/// When an edit shifts the hunk id but the same conceptual change is still
/// present at overlapping lines in the same file, plan-diff must classify the
/// entry as `shifted` with a new 12-hex hunk id different from the captured
/// one.
///
/// Expected RED failure: subcommand missing → exit 2.
#[test]
fn plan_diff_after_content_edit_reports_shifted_entries() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "f.rs", "one\ntwo\n", "initial");
    write_file(dir.path(), "f.rs", "one\ntwo\nthree\n");

    let old_hunk_id = scan_first_hunk_id(dir.path());
    let plan = json!({
        "version": "v1",
        "commits": [
            { "id": "c1", "selections": [old_hunk_id.clone()] }
        ]
    });

    // Edit the file: insert an extra line BEFORE the tracked hunk so the
    // hunk id recomputes (new_start shifts) but the conceptual hunk is still
    // at the tail of the file.
    write_file(dir.path(), "f.rs", "zero\none\ntwo\nthree\n");

    let (code, stdout, stderr) = run_plan_diff_stdin(dir.path(), &plan.to_string(), &[]);
    assert_eq!(
        code, 1,
        "shifted entries must surface as exit code 1. stderr: {stderr}"
    );

    let envelope: Value = serde_json::from_str(&stdout).expect("plan-diff emits JSON envelope");
    let shifted = envelope["shifted"]
        .as_array()
        .expect("shifted must be an array");
    assert_eq!(
        shifted.len(),
        1,
        "edited file should surface as shifted, envelope: {envelope}"
    );
    let new_id = shifted[0]["new_hunk_id"]
        .as_str()
        .expect("shifted entry must expose new_hunk_id");
    assert_eq!(new_id.len(), 12, "hunk ids are 12-hex");
    assert_ne!(
        new_id, old_hunk_id,
        "shifted hunk id must differ from captured one"
    );
    assert_eq!(
        shifted[0]["old_hunk_id"], old_hunk_id,
        "shifted entry must preserve the captured hunk id"
    );
    assert!(
        shifted[0]["match_confidence"].is_string(),
        "shifted entry must include a match_confidence"
    );
}

/// A plan that references a file no longer in the workdir must classify the
/// entry as `gone` with reason `path_missing`.
///
/// Expected RED failure: subcommand missing → exit 2.
#[test]
fn plan_diff_on_missing_file_reports_entry_as_gone() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "f.rs", "one\n", "initial");

    let plan = json!({
        "version": "v1",
        "commits": [
            { "id": "c1", "selections": ["does/not/exist.rs"] }
        ]
    });

    let (code, stdout, stderr) = run_plan_diff_stdin(dir.path(), &plan.to_string(), &[]);
    assert_eq!(
        code, 1,
        "missing-file entry must surface as exit code 1. stderr: {stderr}"
    );

    let envelope: Value = serde_json::from_str(&stdout).expect("plan-diff emits JSON envelope");
    let gone = envelope["gone"].as_array().expect("gone must be an array");
    assert_eq!(gone.len(), 1, "missing path must appear as gone");
    assert_eq!(gone[0]["file_path"], "does/not/exist.rs");
    assert_eq!(
        gone[0]["reason"], "path_missing",
        "missing file must carry reason `path_missing`"
    );
}

/// `pgs plan-diff --stdin` must accept a `CommitPlan` piped on stdin and
/// produce a diff report.
///
/// Expected RED failure: subcommand missing → exit 2, never reaches the
/// envelope parse step.
#[test]
fn plan_diff_accepts_commit_plan_via_stdin() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "f.rs", "one\ntwo\n", "initial");
    write_file(dir.path(), "f.rs", "one\ntwo\nthree\n");

    let plan = json!({
        "version": "v1",
        "commits": [
            { "selections": ["f.rs"] }
        ]
    });

    let (code, stdout, _stderr) = run_plan_diff_stdin(dir.path(), &plan.to_string(), &[]);
    assert_eq!(code, 0, "stdin-fed plan against clean tree must exit 0");
    let envelope: Value = serde_json::from_str(&stdout).expect("plan-diff emits JSON envelope");
    assert_eq!(envelope["command"], "plan-diff");
    assert_eq!(envelope["version"], "v1");
}

/// A `CommitPlan` JSON blob carrying extra fields from a hypothetical future
/// schema version must parse cleanly — unknown fields silently ignored,
/// known fields round-trip — per the `#[serde(default)]` contract that A6
/// extends.
///
/// Expected RED failure: subcommand missing → exit 2 (never parses).
#[test]
fn plan_diff_preserves_unknown_fields_in_plan_input() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "f.rs", "one\ntwo\n", "initial");
    write_file(dir.path(), "f.rs", "one\ntwo\nthree\n");

    let plan = json!({
        "version": "v1",
        "future_top_level_field": "ignored",
        "commits": [
            {
                "id": "c1",
                "selections": ["f.rs"],
                "future_per_commit_field": { "kind": "whatever" }
            }
        ]
    });

    let (code, stdout, stderr) = run_plan_diff_stdin(dir.path(), &plan.to_string(), &[]);
    assert_eq!(
        code, 0,
        "plan with unknown fields must parse cleanly. stderr: {stderr}"
    );
    let envelope: Value = serde_json::from_str(&stdout).expect("plan-diff emits JSON envelope");
    assert_eq!(envelope["command"], "plan-diff");
    let still_valid = envelope["still_valid"]
        .as_array()
        .expect("still_valid must be an array");
    assert_eq!(still_valid.len(), 1, "known fields must still round-trip");
}

// ─── MCP RED tests ────────────────────────────────────────────────────────────

fn spawn_mcp_stdio() -> (Child, ChildStdin, BufReader<ChildStdout>) {
    let mut child = Command::new(assert_cmd::cargo::cargo_bin!("pgs-mcp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let stdin = child.stdin.take().unwrap();
    let stdout = BufReader::new(child.stdout.take().unwrap());
    (child, stdin, stdout)
}

fn write_json_line(stdin: &mut ChildStdin, message: &Value) {
    writeln!(stdin, "{message}").unwrap();
    stdin.flush().unwrap();
}

fn read_stdout_line(stdout: &mut BufReader<ChildStdout>) -> String {
    let mut line = String::new();
    let bytes_read = stdout.read_line(&mut line).unwrap();
    assert!(bytes_read > 0, "expected a JSON-RPC line on stdout");
    line.trim_end_matches(['\n', '\r']).to_owned()
}

fn initialize_session(stdin: &mut ChildStdin, stdout: &mut BufReader<ChildStdout>) {
    write_json_line(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": common::MCP_PROTOCOL_VERSION_BASELINE,
                "capabilities": {},
                "clientInfo": { "name": "pgs-test-client", "version": "0.1.0" }
            }
        }),
    );
    let _initialize_response: Value = serde_json::from_str(&read_stdout_line(stdout)).unwrap();
    write_json_line(
        stdin,
        &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
    );
}

fn list_tools(stdin: &mut ChildStdin, stdout: &mut BufReader<ChildStdout>) -> Value {
    write_json_line(
        stdin,
        &json!({ "jsonrpc": "2.0", "id": 10, "method": "tools/list" }),
    );
    serde_json::from_str(&read_stdout_line(stdout)).unwrap()
}

fn call_tool(
    stdin: &mut ChildStdin,
    stdout: &mut BufReader<ChildStdout>,
    name: &str,
    arguments: &Value,
) -> Value {
    write_json_line(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 11,
            "method": "tools/call",
            "params": { "name": name, "arguments": arguments }
        }),
    );
    serde_json::from_str(&read_stdout_line(stdout)).unwrap()
}

fn shutdown_child(mut child: Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// The frozen MCP contract must expose a `pgs_plan_diff` tool whose input
/// schema requires `repo_path` and `plan`, and whose annotations mark it
/// read-only.
///
/// Expected RED failure: `pgs_plan_diff` is not in `tools/list` until TODO 30
/// registers it.
#[test]
fn pgs_plan_diff_mcp_tool_exposes_correct_schema() {
    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();
    initialize_session(&mut stdin, &mut stdout);

    let response = list_tools(&mut stdin, &mut stdout);
    shutdown_child(child);

    let tools = response["result"]["tools"]
        .as_array()
        .expect("tools list must be an array");
    let plan_diff = tools
        .iter()
        .find(|tool| tool["name"] == "pgs_plan_diff")
        .expect("tools/list must expose `pgs_plan_diff`");

    let schema = &plan_diff["inputSchema"];
    let required: Vec<&str> = schema["required"]
        .as_array()
        .expect("inputSchema.required must be an array")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        required.contains(&"repo_path"),
        "pgs_plan_diff must require `repo_path`, got: {required:?}"
    );
    assert!(
        required.contains(&"plan"),
        "pgs_plan_diff must require `plan`, got: {required:?}"
    );

    let annotations = &plan_diff["annotations"];
    assert_eq!(
        annotations["readOnlyHint"], true,
        "pgs_plan_diff must be annotated as read-only"
    );
    assert_eq!(
        annotations["destructiveHint"], false,
        "pgs_plan_diff must not be destructive"
    );
}

/// A successful `pgs_plan_diff` MCP call must return a `structuredContent`
/// payload with `outcome`, `pgs.version: v1`, `pgs.command: plan-diff`, and
/// the three classification arrays.
///
/// Expected RED failure: tool not registered → `tools/call` returns an error
/// envelope instead of a structured-content success.
#[test]
fn pgs_plan_diff_mcp_tool_returns_structured_content() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "f.rs", "one\ntwo\n", "initial");
    write_file(dir.path(), "f.rs", "one\ntwo\nthree\n");

    let plan = json!({
        "version": "v1",
        "commits": [
            { "selections": ["f.rs"] }
        ]
    });

    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();
    initialize_session(&mut stdin, &mut stdout);
    let response = call_tool(
        &mut stdin,
        &mut stdout,
        "pgs_plan_diff",
        &json!({
            "repo_path": dir.path().display().to_string(),
            "plan": plan
        }),
    );
    shutdown_child(child);

    assert_eq!(response["jsonrpc"], "2.0");
    let result = &response["result"];
    assert_eq!(result["isError"], false);

    let structured = &result["structuredContent"];
    assert_eq!(structured["outcome"], "ok");
    let pgs = &structured["pgs"];
    assert_eq!(pgs["version"], "v1");
    assert_eq!(pgs["command"], "plan-diff");

    for key in ["still_valid", "shifted", "gone"] {
        assert!(
            pgs[key].is_array(),
            "pgs.{key} must be an array, got: {pgs}"
        );
    }
}
