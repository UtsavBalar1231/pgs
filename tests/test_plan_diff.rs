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

/// Helper: spawn `pgs plan-diff` with `plan_json` piped on stdin.
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
        .arg("plan-diff")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn pgs plan-diff via stdin");

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
            { "id": "c1", "selections": [&hunk_id] }
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
            { "id": "c1", "selections": [&hunk_id] }
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

/// When an edit changes the hunk content with no checksum or range signal
/// linking the captured id to any live hunk, plan-diff must classify the entry
/// as `gone/no_match` — never a spurious `shifted/Low`.
///
/// Expected RED failure: subcommand missing → exit 2.
#[test]
fn plan_diff_after_content_edit_reports_gone_no_match() {
    // The plan captures a bare hunk id. The workdir then changes in a way
    // that alters the hunk content (adding "zero\n" shifts the whole diff),
    // so there is no checksum or range signal linking the captured id to any
    // live hunk. Correct result: gone/no_match — not a spurious shifted/Low.
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "f.rs", "one\ntwo\n", "initial");
    write_file(dir.path(), "f.rs", "one\ntwo\nthree\n");

    let old_hunk_id = scan_first_hunk_id(dir.path());
    let plan = json!({
        "version": "v1",
        "commits": [
            { "id": "c1", "selections": [&old_hunk_id] }
        ]
    });

    // Inserting "zero\n" before the existing content changes the diff from
    // "add three\n" to "add zero\n + add three\n" — a new hunk with a new
    // checksum. No genuine content or range relationship survives.
    write_file(dir.path(), "f.rs", "zero\none\ntwo\nthree\n");

    let (code, stdout, stderr) = run_plan_diff_stdin(dir.path(), &plan.to_string(), &[]);
    assert_eq!(
        code, 1,
        "gone entries must surface as exit code 1. stderr: {stderr}"
    );

    let envelope: Value = serde_json::from_str(&stdout).expect("plan-diff emits JSON envelope");
    assert!(
        envelope["shifted"].as_array().unwrap().is_empty(),
        "no spurious shifted entry expected, got: {envelope}"
    );
    let gone = envelope["gone"].as_array().expect("gone must be an array");
    assert_eq!(gone.len(), 1, "one gone entry expected, got: {envelope}");
    assert_eq!(
        gone[0]["reason"].as_str(),
        Some("no_match"),
        "reason must be no_match, got: {envelope}"
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

/// `pgs plan-diff` must accept a `CommitPlan` piped on stdin and
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

// ─── Fuzzy-match integration tests ───────────────────────────────────────────

/// Hunk that shifts position within a file (`hunk_id` changes at `new_start`)
/// but keeps the same content (checksum stable). The plan carries the old
/// `hunk_id` as a bare selection plus `expected_checksum`. After the shift the
/// old id no longer resolves → cross-file checksum search finds the hunk at
/// its new position → `shifted/High`.
#[test]
fn plan_diff_shifted_hunk_via_checksum_classifies_shifted_high() {
    let (dir, repo) = setup_repo();
    let base = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk\nl\nm\nn\no\np\nq\nr\ns\nt\n";
    commit_file(&repo, dir.path(), "f.rs", base, "initial");

    // Phase 1: add BOTTOM_NEW at the end — one hunk far from the top.
    write_file(dir.path(), "f.rs", &format!("{base}BOTTOM_NEW\n"));
    let scan1: Value = {
        let out = run_pgs(dir.path(), &["scan", "--full"]).success();
        serde_json::from_slice(&out.get_output().stdout).expect("scan JSON")
    };
    let old_hunk_id = scan1["files"][0]["hunks"][0]["id"]
        .as_str()
        .expect("hunk id")
        .to_owned();
    let checksum = scan1["files"][0]["hunks"][0]["checksum"]
        .as_str()
        .expect("hunk checksum in full mode")
        .to_owned();

    // Phase 2: also insert TOP_NEW at the very beginning. TOP_NEW is far from
    // BOTTOM_NEW so BOTTOM_NEW's context lines are unchanged → same checksum,
    // but BOTTOM_NEW's new_start shifts → different hunk_id.
    write_file(dir.path(), "f.rs", &format!("TOP_NEW\n{base}BOTTOM_NEW\n"));

    let plan = json!({
        "version": "v1",
        "commits": [{ "id": "c1", "selections": [&old_hunk_id], "expected_checksum": checksum }]
    });
    let (code, stdout, stderr) = run_plan_diff_stdin(dir.path(), &plan.to_string(), &[]);
    assert_eq!(code, 1, "shifted exits 1. stderr: {stderr}");

    let envelope: Value = serde_json::from_str(&stdout).expect("plan-diff JSON");
    let shifted = envelope["shifted"].as_array().expect("shifted array");
    assert_eq!(
        shifted.len(),
        1,
        "one shifted entry expected, got: {envelope}"
    );
    assert_eq!(shifted[0]["match_confidence"], "high", "got: {envelope}");
    assert!(
        envelope["gone"].as_array().unwrap().is_empty(),
        "no false gone: {envelope}"
    );
}

/// Bare hunk-id that is stale (not in the current scan) with `expected_checksum`
/// matching a hunk in `zzz.rs`. `aaa.rs` has a live hunk and comes first
/// alphabetically — the old code searched only that first file and missed the
/// match. The fixed code searches all files and finds it in `zzz.rs`.
#[test]
fn plan_diff_bare_hunk_id_cross_file_checksum_match_classifies_shifted_high() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "aaa.rs", "p\nq\nr\n", "initial aaa");
    commit_file(&repo, dir.path(), "zzz.rs", "x\ny\nz\n", "initial zzz");

    write_file(dir.path(), "zzz.rs", "x\ny\nNEW\nz\n");
    let scan_json: Value = {
        let out = run_pgs(dir.path(), &["scan", "--full"]).success();
        serde_json::from_slice(&out.get_output().stdout).expect("scan JSON")
    };
    let zzz_file = scan_json["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["path"] == "zzz.rs")
        .expect("zzz.rs in scan");
    let zzz_checksum = zzz_file["hunks"][0]["checksum"]
        .as_str()
        .expect("zzz.rs hunk checksum")
        .to_owned();

    // Give aaa.rs a live hunk with different content so it comes first in scan
    // but does not have the matching checksum.
    write_file(dir.path(), "aaa.rs", "p\nDECOY\nq\nr\n");

    let plan = json!({
        "version": "v1",
        "commits": [{
            "id": "c1",
            "selections": ["deadbeef1234"],
            "expected_checksum": zzz_checksum
        }]
    });
    let (code, stdout, stderr) = run_plan_diff_stdin(dir.path(), &plan.to_string(), &[]);
    assert_eq!(code, 1, "shifted exits 1. stderr: {stderr}");

    let envelope: Value = serde_json::from_str(&stdout).expect("plan-diff JSON");
    let shifted = envelope["shifted"].as_array().expect("shifted array");
    assert_eq!(
        shifted.len(),
        1,
        "cross-file match → shifted, got: {envelope}"
    );
    assert_eq!(shifted[0]["match_confidence"], "high", "got: {envelope}");
    assert_eq!(
        shifted[0]["file_path"], "zzz.rs",
        "located in zzz.rs, got: {envelope}"
    );
    assert!(
        envelope["gone"].as_array().unwrap().is_empty(),
        "no false gone: {envelope}"
    );
}

/// `Lines` selection (`f.rs:5-10`) that no longer overlaps any live hunk,
/// paired with `expected_checksum` matching a hunk at a different position and
/// NO `captured_hunk_id`. The old code gated fuzzy-matching on `captured_hunk_id`
/// being present, so this was incorrectly classified `gone/no_match`. The fixed
/// code calls `find_fuzzy_match` unconditionally → `shifted/High`.
#[test]
fn plan_diff_lines_checksum_only_no_captured_hunk_id_classifies_shifted_high() {
    let (dir, repo) = setup_repo();
    let base = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\n\
                line11\nline12\nline13\nline14\nline15\nline16\nline17\nline18\nline19\nline20\n";
    commit_file(&repo, dir.path(), "f.rs", base, "initial");

    // Phase 1: append CHANGE at the end — hunk far below lines 5-10.
    write_file(dir.path(), "f.rs", &format!("{base}CHANGE\n"));
    let scan1: Value = {
        let out = run_pgs(dir.path(), &["scan", "--full"]).success();
        serde_json::from_slice(&out.get_output().stdout).expect("scan JSON")
    };
    let checksum = scan1["files"][0]["hunks"][0]["checksum"]
        .as_str()
        .expect("hunk checksum")
        .to_owned();

    // Phase 2: also prepend a line at the top so CHANGE shifts further down.
    // Context lines around CHANGE (line18..line20) are unaffected → same checksum.
    write_file(dir.path(), "f.rs", &format!("PREPEND\n{base}CHANGE\n"));

    // Plan uses range 5-10 (no live hunk there), checksum matches CHANGE hunk,
    // no captured_hunk_id supplied.
    let plan = json!({
        "version": "v1",
        "commits": [{
            "id": "c1",
            "selections": ["f.rs:5-10"],
            "expected_checksum": checksum
        }]
    });
    let (code, stdout, stderr) = run_plan_diff_stdin(dir.path(), &plan.to_string(), &[]);
    assert_eq!(code, 1, "shifted exits 1. stderr: {stderr}");

    let envelope: Value = serde_json::from_str(&stdout).expect("plan-diff JSON");
    let shifted = envelope["shifted"].as_array().expect("shifted array");
    assert_eq!(
        shifted.len(),
        1,
        "checksum-only Lines match → shifted, got: {envelope}"
    );
    assert_eq!(shifted[0]["match_confidence"], "high", "got: {envelope}");
    assert!(
        envelope["gone"].as_array().unwrap().is_empty(),
        "no false gone: {envelope}"
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
