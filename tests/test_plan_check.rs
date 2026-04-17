//! Tests for `pgs plan-check` and the `pgs_plan_check` MCP tool. A `CommitPlan`
//! is agent-supplied input listing planned commits via `selections`; plan-check
//! runs a fresh scan and reports `overlaps`, `uncovered`, `unsafe_selectors`
//! (ranges crossing hunk boundaries), and `unknown_paths`.

mod common;

use std::fmt::Write as _;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use common::{commit_file, run_pgs, setup_repo, write_file};
use serde_json::{Value, json};

// ─── CLI RED tests ────────────────────────────────────────────────────────────

/// Helper: spawn `pgs plan-check --stdin` with `plan_json` piped on stdin, with
/// the given `--repo` target. Returns exit code + stdout + stderr.
fn run_plan_check_stdin(
    dir: &std::path::Path,
    plan_json: &str,
    extra_args: &[&str],
) -> (i32, String, String) {
    let mut cmd = std::process::Command::new(assert_cmd::cargo::cargo_bin!("pgs"))
        .arg("--json")
        .arg("--repo")
        .arg(dir.to_str().unwrap())
        .args(extra_args)
        .args(["plan-check", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn pgs plan-check --stdin");

    {
        let mut stdin = cmd.stdin.take().expect("plan-check stdin piped");
        stdin
            .write_all(plan_json.as_bytes())
            .expect("write plan JSON to stdin");
    }

    let output = cmd.wait_with_output().expect("wait for plan-check");
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

/// A plan whose single commit selects the only modified file must validate
/// clean: no overlaps, no uncovered hunks, no unsafe selectors, no unknown
/// paths. Exit code 0.
///
/// Expected RED failure: `pgs plan-check` is not yet a registered subcommand,
/// so clap returns `InvalidSubcommand` (exit code 2) — the assertion of
/// `code == 0` fails.
#[test]
fn plan_check_accepts_commit_plan_via_stdin() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "f.rs", "one\ntwo\n", "initial");
    write_file(dir.path(), "f.rs", "one\ntwo\nthree\n");

    let plan = json!({
        "version": "v1",
        "commits": [
            { "selections": ["f.rs"] }
        ]
    });
    let (code, stdout, stderr) = run_plan_check_stdin(dir.path(), &plan.to_string(), &[]);

    assert_eq!(
        code, 0,
        "a fully-covering plan must exit 0; stdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let envelope: Value = serde_json::from_str(&stdout).expect("plan-check emits JSON envelope");
    assert_eq!(envelope["version"], "v1");
    assert_eq!(envelope["command"], "plan-check");
    assert!(
        envelope["overlaps"].as_array().is_some_and(Vec::is_empty),
        "no overlaps expected on a clean plan, got {:?}",
        envelope["overlaps"]
    );
    assert!(
        envelope["uncovered"].as_array().is_some_and(Vec::is_empty),
        "no uncovered hunks expected, got {:?}",
        envelope["uncovered"]
    );
    assert!(
        envelope["unsafe_selectors"]
            .as_array()
            .is_some_and(Vec::is_empty),
        "no unsafe selectors expected, got {:?}",
        envelope["unsafe_selectors"]
    );
    assert!(
        envelope["unknown_paths"]
            .as_array()
            .is_some_and(Vec::is_empty),
        "no unknown paths expected, got {:?}",
        envelope["unknown_paths"]
    );
}

/// Two commits that both reference the same hunk must be reported as an
/// `overlap`, naming the hunk id and the two commit identifiers involved.
#[test]
fn plan_check_reports_overlap_when_selectors_double_cover_hunks() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "f.rs", "one\ntwo\n", "initial");
    write_file(dir.path(), "f.rs", "one\ntwo\nthree\n");

    let hunk_id = scan_first_hunk_id(dir.path());

    let plan = json!({
        "version": "v1",
        "commits": [
            { "id": "commitA", "selections": ["f.rs"] },
            { "id": "commitB", "selections": [hunk_id] }
        ]
    });
    let (code, stdout, stderr) = run_plan_check_stdin(dir.path(), &plan.to_string(), &[]);

    // Any issue flips exit code to 1 (no-effect category: plan had an issue).
    assert_eq!(
        code, 1,
        "overlap must surface as exit code 1; stdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let envelope: Value = serde_json::from_str(&stdout).expect("plan-check emits JSON envelope");
    let overlaps = envelope["overlaps"]
        .as_array()
        .expect("overlaps must be an array");
    assert!(
        !overlaps.is_empty(),
        "expected at least one overlap record, got: {envelope}"
    );
    let overlap = &overlaps[0];
    assert_eq!(
        overlap["hunk_id"], hunk_id,
        "overlap must cite the shared hunk id"
    );
    let commits: Vec<&str> = overlap["commits"]
        .as_array()
        .expect("commits must be an array")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        commits.contains(&"commitA") && commits.contains(&"commitB"),
        "expected both commit ids (commitA, commitB) in overlap, got: {commits:?}"
    );
}

/// A scan reports two hunks in a file, a plan covers only the first → the
/// second must show up as `uncovered` with its hunk id and path.
#[test]
fn plan_check_reports_uncovered_hunks_when_selectors_miss_changes() {
    let (dir, repo) = setup_repo();
    // Build a 40-line file so we can modify lines far apart → two separate hunks.
    let mut original = String::new();
    for i in 1..=40 {
        writeln!(&mut original, "line{i}").expect("write to string");
    }
    commit_file(&repo, dir.path(), "f.rs", &original, "initial");

    let mut modified = String::new();
    for i in 1..=40 {
        if i == 3 {
            modified.push_str("CHANGED_3\n");
        } else if i == 35 {
            modified.push_str("CHANGED_35\n");
        } else {
            writeln!(&mut modified, "line{i}").expect("write to string");
        }
    }
    write_file(dir.path(), "f.rs", &modified);

    let scan = run_pgs(dir.path(), &["scan"]).success();
    let scan_json: Value =
        serde_json::from_slice(&scan.get_output().stdout).expect("scan JSON parses");
    let hunks = scan_json["files"][0]["hunks"]
        .as_array()
        .expect("hunks array");
    assert!(
        hunks.len() >= 2,
        "test precondition: expected >=2 hunks, got {}",
        hunks.len()
    );
    let first_hunk_id = hunks[0]["id"].as_str().expect("first hunk id").to_owned();
    let second_hunk_id = hunks[1]["id"].as_str().expect("second hunk id").to_owned();

    let plan = json!({
        "version": "v1",
        "commits": [
            { "id": "only-first", "selections": [first_hunk_id] }
        ]
    });
    let (code, stdout, _) = run_plan_check_stdin(dir.path(), &plan.to_string(), &[]);
    assert_eq!(code, 1, "uncovered hunk must surface as exit code 1");

    let envelope: Value = serde_json::from_str(&stdout).expect("plan-check emits JSON envelope");
    let uncovered = envelope["uncovered"]
        .as_array()
        .expect("uncovered must be an array");
    let uncovered_ids: Vec<&str> = uncovered
        .iter()
        .filter_map(|r| r["hunk_id"].as_str())
        .collect();
    assert!(
        uncovered_ids.contains(&second_hunk_id.as_str()),
        "expected second hunk id {second_hunk_id} in uncovered, got {uncovered_ids:?}"
    );
    for record in uncovered {
        assert!(
            record["file_path"].as_str().is_some(),
            "uncovered record must include file_path: {record}"
        );
    }
}

/// A `path:A-B` range that spans two separate hunks must be flagged as
/// `spans_hunk_boundary` in `unsafe_selectors`.
#[test]
fn plan_check_flags_line_range_spanning_hunk_boundary_as_unsafe() {
    let (dir, repo) = setup_repo();
    let mut original = String::new();
    for i in 1..=40 {
        writeln!(&mut original, "line{i}").expect("write to string");
    }
    commit_file(&repo, dir.path(), "f.rs", &original, "initial");

    let mut modified = String::new();
    for i in 1..=40 {
        if i == 3 {
            modified.push_str("CHANGED_3\n");
        } else if i == 35 {
            modified.push_str("CHANGED_35\n");
        } else {
            writeln!(&mut modified, "line{i}").expect("write to string");
        }
    }
    write_file(dir.path(), "f.rs", &modified);

    // Precondition: scan yields two hunks.
    let scan = run_pgs(dir.path(), &["scan"]).success();
    let scan_json: Value =
        serde_json::from_slice(&scan.get_output().stdout).expect("scan JSON parses");
    let hunks = scan_json["files"][0]["hunks"]
        .as_array()
        .expect("hunks array");
    assert!(
        hunks.len() >= 2,
        "test precondition: expected >=2 hunks, got {}",
        hunks.len()
    );

    // A range that covers both the lines around hunk 1 (line 3) and hunk 2
    // (line 35) obviously straddles the boundary.
    let straddling = "f.rs:1-40";
    let plan = json!({
        "version": "v1",
        "commits": [
            { "id": "bad-range", "selections": [straddling] }
        ]
    });
    let (code, stdout, _) = run_plan_check_stdin(dir.path(), &plan.to_string(), &[]);
    assert_eq!(code, 1, "unsafe selector must surface as exit code 1");

    let envelope: Value = serde_json::from_str(&stdout).expect("plan-check emits JSON envelope");
    let unsafe_selectors = envelope["unsafe_selectors"]
        .as_array()
        .expect("unsafe_selectors must be an array");
    assert!(
        !unsafe_selectors.is_empty(),
        "expected at least one unsafe selector record, got: {envelope}"
    );
    let entry = &unsafe_selectors[0];
    assert_eq!(
        entry["selection"], straddling,
        "unsafe record must cite the offending selector"
    );
    assert_eq!(
        entry["reason"], "spans_hunk_boundary",
        "reason must be spans_hunk_boundary for cross-hunk ranges"
    );
}

/// An empty plan (`commits: []`) must report every hunk in the scan as
/// uncovered.
#[test]
fn plan_check_on_empty_plan_reports_all_hunks_uncovered() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "f.rs", "one\ntwo\n", "initial");
    write_file(dir.path(), "f.rs", "one\ntwo\nthree\n");

    let scan = run_pgs(dir.path(), &["scan"]).success();
    let scan_json: Value =
        serde_json::from_slice(&scan.get_output().stdout).expect("scan JSON parses");
    let total_hunks = scan_json["summary"]["total_hunks"]
        .as_u64()
        .expect("total_hunks is u64");
    assert!(
        total_hunks >= 1,
        "precondition: scan should see at least one hunk"
    );

    let plan = json!({
        "version": "v1",
        "commits": []
    });
    let (code, stdout, _) = run_plan_check_stdin(dir.path(), &plan.to_string(), &[]);
    assert_eq!(code, 1, "empty plan with unstaged hunks must exit 1");

    let envelope: Value = serde_json::from_str(&stdout).expect("plan-check emits JSON envelope");
    let uncovered = envelope["uncovered"]
        .as_array()
        .expect("uncovered must be an array");
    assert_eq!(
        uncovered.len() as u64,
        total_hunks,
        "every scan hunk must appear as uncovered when plan is empty"
    );
}

/// A selection referring to a file that is not in the scan (path absent from
/// the diff) must be reported as `unknown_paths` rather than crashing. Other
/// hunks may still show up as uncovered — that's fine.
#[test]
fn plan_check_ignores_commits_covering_untracked_files() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "f.rs", "one\ntwo\n", "initial");
    write_file(dir.path(), "f.rs", "one\ntwo\nthree\n");

    let plan = json!({
        "version": "v1",
        "commits": [
            { "id": "good", "selections": ["f.rs"] },
            { "id": "ghost", "selections": ["does/not/exist.rs"] }
        ]
    });
    let (code, stdout, _) = run_plan_check_stdin(dir.path(), &plan.to_string(), &[]);
    assert_eq!(code, 1, "unknown path must surface as exit code 1");

    let envelope: Value = serde_json::from_str(&stdout).expect("plan-check emits JSON envelope");
    let unknown_paths: Vec<&str> = envelope["unknown_paths"]
        .as_array()
        .expect("unknown_paths must be an array")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        unknown_paths.contains(&"does/not/exist.rs"),
        "expected ghost path in unknown_paths, got: {unknown_paths:?}"
    );
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

/// The frozen MCP contract must expose a `pgs_plan_check` tool whose input
/// schema requires `repo_path` and `plan`, and whose annotations mark it
/// read-only.
///
/// Expected RED failure: `pgs_plan_check` is not listed in `tools/list` until
/// TODO 27 registers it.
#[test]
fn pgs_plan_check_mcp_tool_exposes_correct_schema() {
    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();
    initialize_session(&mut stdin, &mut stdout);

    let response = list_tools(&mut stdin, &mut stdout);
    shutdown_child(child);

    let tools = response["result"]["tools"]
        .as_array()
        .expect("tools list must be an array");
    let plan_check = tools
        .iter()
        .find(|tool| tool["name"] == "pgs_plan_check")
        .expect("tools/list must expose `pgs_plan_check`");

    let schema = &plan_check["inputSchema"];
    let required: Vec<&str> = schema["required"]
        .as_array()
        .expect("inputSchema.required must be an array")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        required.contains(&"repo_path"),
        "pgs_plan_check must require `repo_path`, got: {required:?}"
    );
    assert!(
        required.contains(&"plan"),
        "pgs_plan_check must require `plan`, got: {required:?}"
    );

    let annotations = &plan_check["annotations"];
    assert_eq!(
        annotations["readOnlyHint"], true,
        "pgs_plan_check must be annotated as read-only"
    );
    assert_eq!(
        annotations["destructiveHint"], false,
        "pgs_plan_check must not be destructive"
    );
}

/// A successful `pgs_plan_check` MCP call on a clean plan must return a
/// `structuredContent` payload with `outcome: ok`, `pgs.version: v1`,
/// `pgs.command: plan-check`, and empty issue arrays.
///
/// Expected RED failure: tool not registered → `tools/call` returns an error
/// ("tool not found") instead of a structured success envelope.
#[test]
fn pgs_plan_check_mcp_tool_returns_structured_content() {
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
        "pgs_plan_check",
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
    assert_eq!(pgs["command"], "plan-check");

    for key in ["overlaps", "uncovered", "unsafe_selectors", "unknown_paths"] {
        let arr = pgs[key]
            .as_array()
            .unwrap_or_else(|| panic!("pgs.{key} must be an array, got: {pgs}"));
        assert!(
            arr.is_empty(),
            "clean plan must have empty {key}, got: {arr:?}"
        );
    }
}
