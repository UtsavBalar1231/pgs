//! Tests for `pgs split-hunk <id>`: a hunk is split into contiguous
//! `{ start, end, origin_mix }` ranges where `origin_mix` is `addition`,
//! `deletion`, or `mixed`. Context lines break runs. Output is descriptive —
//! the agent decides what to stage.

mod common;

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use common::{commit_file, run_pgs, run_pgs_raw, setup_repo, write_file};
use serde_json::{Value, json};

// ─── CLI RED tests ────────────────────────────────────────────────────────────

/// A hunk consisting of a single contiguous addition run must yield exactly
/// one range with `origin_mix == "addition"`.
///
/// Expected RED failure: `pgs split-hunk <id>` is not yet a registered
/// subcommand, so clap returns `InvalidSubcommand` (exit code 2). After
/// TODO 23 this becomes GREEN.
#[test]
fn split_hunk_on_single_addition_run_returns_one_range_addition_only() {
    let (dir, repo) = setup_repo();
    commit_file(
        &repo,
        dir.path(),
        "f.rs",
        "one\ntwo\nthree\nfour\nfive\n",
        "initial",
    );
    // Append two lines at the tail — pure addition run.
    write_file(
        dir.path(),
        "f.rs",
        "one\ntwo\nthree\nfour\nfive\nsix\nseven\n",
    );

    let scan = run_pgs(dir.path(), &["scan"]).success();
    let scan_json: Value =
        serde_json::from_slice(&scan.get_output().stdout).expect("scan JSON parses");
    let hunk_id = scan_json["files"][0]["hunks"][0]["id"]
        .as_str()
        .expect("scan exposes a hunk id")
        .to_owned();

    let split = run_pgs(dir.path(), &["split-hunk", &hunk_id]).success();
    let split_json: Value =
        serde_json::from_slice(&split.get_output().stdout).expect("split JSON parses");

    assert_eq!(split_json["version"], "v1");
    assert_eq!(split_json["command"], "split");
    assert_eq!(split_json["hunk_id"], hunk_id);

    let ranges = split_json["ranges"]
        .as_array()
        .expect("ranges must be an array");
    assert_eq!(
        ranges.len(),
        1,
        "a single addition run produces exactly one range"
    );
    assert_eq!(ranges[0]["origin_mix"], "addition");
    let start = ranges[0]["start"].as_u64().expect("start is u32");
    let end = ranges[0]["end"].as_u64().expect("end is u32");
    assert!(
        start <= end,
        "range {start}-{end} must be 1-indexed and non-empty"
    );
}

/// A hunk with interleaved additions and deletions must split into multiple
/// ranges, each classified by its own `origin_mix`.
///
/// Expected RED failure: subcommand missing (pre-TODO-23) or classifier
/// absent (pre-TODO-24 if CLI lands before the algorithm).
#[test]
fn split_hunk_on_mixed_hunk_returns_multiple_ranges() {
    let (dir, repo) = setup_repo();
    commit_file(
        &repo,
        dir.path(),
        "f.rs",
        "alpha\nbeta\ngamma\ndelta\nepsilon\n",
        "initial",
    );
    // Replace middle chunk so we get a deletion run followed by an addition
    // run inside the same hunk (typical mixed hunk).
    write_file(
        dir.path(),
        "f.rs",
        "alpha\nNEW_BETA\nNEW_GAMMA\nNEW_DELTA\nepsilon\n",
    );

    let scan = run_pgs(dir.path(), &["scan"]).success();
    let scan_json: Value =
        serde_json::from_slice(&scan.get_output().stdout).expect("scan JSON parses");
    let hunk_id = scan_json["files"][0]["hunks"][0]["id"]
        .as_str()
        .expect("scan exposes a hunk id")
        .to_owned();

    let split = run_pgs(dir.path(), &["split-hunk", &hunk_id]).success();
    let split_json: Value =
        serde_json::from_slice(&split.get_output().stdout).expect("split JSON parses");

    let ranges = split_json["ranges"]
        .as_array()
        .expect("ranges must be an array");
    assert!(
        ranges.len() >= 2,
        "a mixed hunk should produce at least two ranges, got {}: {ranges:?}",
        ranges.len()
    );

    let mixes: Vec<&str> = ranges
        .iter()
        .filter_map(|r| r["origin_mix"].as_str())
        .collect();
    let allowed: [&str; 3] = ["addition", "deletion", "mixed"];
    for mix in &mixes {
        assert!(
            allowed.contains(mix),
            "origin_mix `{mix}` must be one of {allowed:?}"
        );
    }
    assert!(
        mixes.iter().any(|m| *m == "addition")
            || mixes.iter().any(|m| *m == "deletion")
            || mixes.iter().any(|m| *m == "mixed"),
        "mixed hunk must produce at least one classified run"
    );
}

/// An unknown 12-hex hunk ID must fail with exit code 2 and error code
/// `unknown_hunk_id`.
///
/// Expected RED failure: subcommand missing → clap `InvalidSubcommand` exit
/// code 2 is returned, but without the `unknown_hunk_id` error code in the
/// JSON envelope. After TODO 23 the subcommand exists and the failure path
/// flips to the real resolver.
#[test]
fn split_hunk_unknown_id_returns_user_error() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "f.rs", "one\ntwo\n", "initial");
    write_file(dir.path(), "f.rs", "one\ntwo\nthree\n");

    let assert = run_pgs(dir.path(), &["split-hunk", "deadbeefcafe"]).failure();
    let code = assert.get_output().status.code().unwrap_or(-1);
    assert_eq!(
        code, 2,
        "unknown hunk ID must return exit code 2 (user error)"
    );

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap_or_default();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap_or_default();
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("unknown_hunk_id"),
        "expected `unknown_hunk_id` error code in output, got:\nstdout: {stdout}\nstderr: {stderr}"
    );
}

/// Workdir changed between scan and split-hunk → `stale_scan` retryable error
/// (exit code 3).
///
/// Expected RED failure: subcommand missing → exit code 2 instead of 3.
/// After TODO 23 the real freshness check produces the retryable 3.
#[test]
fn split_hunk_after_content_change_returns_stale_scan_retryable() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "f.rs", "one\ntwo\nthree\n", "initial");
    write_file(dir.path(), "f.rs", "one\ntwo\nthree\nfour\n");

    let scan = run_pgs(dir.path(), &["scan"]).success();
    let scan_json: Value =
        serde_json::from_slice(&scan.get_output().stdout).expect("scan JSON parses");
    let hunk_id = scan_json["files"][0]["hunks"][0]["id"]
        .as_str()
        .expect("scan exposes a hunk id")
        .to_owned();

    // Mutate the working tree AFTER the scan so the recorded file_checksum
    // no longer matches — freshness must fire.
    write_file(dir.path(), "f.rs", "one\ntwo\nthree\nfour\nfive\nsix\n");

    let assert = run_pgs(dir.path(), &["split-hunk", &hunk_id]).failure();
    let code = assert.get_output().status.code().unwrap_or(-1);
    assert_eq!(
        code, 3,
        "stale-scan must return exit code 3 (retryable conflict)"
    );

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap_or_default();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap_or_default();
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("stale_scan"),
        "expected `stale_scan` error code in output, got:\nstdout: {stdout}\nstderr: {stderr}"
    );
}

/// Attempting `split-hunk` on a binary file's hunk ID is a user error — binary
/// files expose no granular hunks and therefore no split surface.
///
/// Expected RED failure: subcommand missing. After TODO 23 the resolver will
/// reject the binary hunk id with `unknown_hunk_id` (no granular hunks exist
/// for the binary file), which satisfies the "user error" contract.
#[test]
fn split_hunk_on_binary_file_returns_user_error() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "blob.bin", "placeholder\n", "initial");
    // Overwrite with binary content (NULL bytes) — pgs classifies this as binary.
    let binary: Vec<u8> = vec![0x00, 0x01, 0x02, 0xFF, 0xFE, 0x7F];
    std::fs::write(dir.path().join("blob.bin"), binary).expect("write binary");

    // Binary files have no granular hunks; any plausible 12-hex ID is unknown.
    let assert = run_pgs(dir.path(), &["split-hunk", "abcdef012345"]).failure();
    let code = assert.get_output().status.code().unwrap_or(-1);
    assert_eq!(
        code, 2,
        "split-hunk against a missing hunk (binary file has none) must be a user error (exit 2)"
    );

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap_or_default();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap_or_default();
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("unknown_hunk_id"),
        "expected runtime `unknown_hunk_id` error (not a clap parse error), got:\nstdout: {stdout}\nstderr: {stderr}"
    );
}

/// Text-mode marker smoke check: the compact marker stream must open with
/// `split.begin` and close with `split.end`, with at least one `split.range`
/// between them.
///
/// Expected RED failure: subcommand missing.
#[test]
fn split_hunk_text_markers_wrap_range_records() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "f.rs", "one\ntwo\nthree\n", "initial");
    write_file(dir.path(), "f.rs", "one\ntwo\nthree\nfour\nfive\n");

    let scan = run_pgs(dir.path(), &["scan"]).success();
    let scan_json: Value =
        serde_json::from_slice(&scan.get_output().stdout).expect("scan JSON parses");
    let hunk_id = scan_json["files"][0]["hunks"][0]["id"]
        .as_str()
        .expect("scan exposes a hunk id")
        .to_owned();

    let assert = run_pgs_raw(dir.path(), &["split-hunk", &hunk_id]).success();
    let text = String::from_utf8(assert.get_output().stdout.clone()).unwrap();

    assert!(
        text.contains("@@pgs:v1 split.begin "),
        "expected split.begin marker, got:\n{text}"
    );
    assert!(
        text.contains("@@pgs:v1 split.range "),
        "expected at least one split.range marker, got:\n{text}"
    );
    assert!(
        text.contains("@@pgs:v1 split.end "),
        "expected split.end marker, got:\n{text}"
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

/// The frozen MCP contract must expose a `pgs_split_hunk` tool whose input
/// schema requires `repo_path` and `hunk_id`, and whose annotations mark it
/// read-only.
///
/// Expected RED failure: `pgs_split_hunk` is not listed in `tools/list` until
/// TODO 24 registers it.
#[test]
fn pgs_split_hunk_mcp_tool_exposes_correct_schema() {
    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();
    initialize_session(&mut stdin, &mut stdout);

    let response = list_tools(&mut stdin, &mut stdout);
    shutdown_child(child);

    let tools = response["result"]["tools"]
        .as_array()
        .expect("tools list must be an array");
    let split = tools
        .iter()
        .find(|tool| tool["name"] == "pgs_split_hunk")
        .expect("tools/list must expose `pgs_split_hunk`");

    let schema = &split["inputSchema"];
    let required: Vec<&str> = schema["required"]
        .as_array()
        .expect("inputSchema.required must be an array")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        required.contains(&"repo_path"),
        "pgs_split_hunk must require `repo_path`, got: {required:?}"
    );
    assert!(
        required.contains(&"hunk_id"),
        "pgs_split_hunk must require `hunk_id`, got: {required:?}"
    );

    let annotations = &split["annotations"];
    assert_eq!(
        annotations["readOnlyHint"], true,
        "pgs_split_hunk must be annotated as read-only"
    );
    assert_eq!(
        annotations["destructiveHint"], false,
        "pgs_split_hunk must not be destructive"
    );
}

/// A successful `pgs_split_hunk` MCP call must return a `structuredContent`
/// payload with the split-hunk output shape: `outcome: ok`, `pgs.version: v1`,
/// `pgs.command: split`, `pgs.hunk_id`, and a `pgs.ranges` array.
///
/// Expected RED failure: tool not registered → `tools/call` returns an error
/// ("tool not found") instead of a structured success envelope.
#[test]
fn pgs_split_hunk_mcp_tool_returns_structured_content() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "f.rs", "one\ntwo\nthree\n", "initial");
    write_file(dir.path(), "f.rs", "one\ntwo\nthree\nfour\nfive\n");

    let scan = run_pgs(dir.path(), &["scan"]).success();
    let scan_json: Value =
        serde_json::from_slice(&scan.get_output().stdout).expect("scan JSON parses");
    let hunk_id = scan_json["files"][0]["hunks"][0]["id"]
        .as_str()
        .expect("scan exposes a hunk id")
        .to_owned();

    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();
    initialize_session(&mut stdin, &mut stdout);
    let response = call_tool(
        &mut stdin,
        &mut stdout,
        "pgs_split_hunk",
        &json!({
            "repo_path": dir.path().display().to_string(),
            "hunk_id": hunk_id
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
    assert_eq!(pgs["command"], "split");
    assert_eq!(pgs["hunk_id"], hunk_id);

    let ranges = pgs["ranges"]
        .as_array()
        .expect("pgs.ranges must be an array");
    assert!(
        !ranges.is_empty(),
        "structured content must include at least one split range"
    );
    for range in ranges {
        let mix = range["origin_mix"]
            .as_str()
            .expect("origin_mix is a string");
        let allowed: [&str; 3] = ["addition", "deletion", "mixed"];
        assert!(
            allowed.contains(&mix),
            "origin_mix `{mix}` must be one of {allowed:?}"
        );
        assert!(range["start"].is_u64(), "start must be u32");
        assert!(range["end"].is_u64(), "end must be u32");
    }
}
