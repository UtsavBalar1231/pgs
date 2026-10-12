//! Protocol-level coverage for the `pgs-mcp` stdio server: both session
//! shapes it serves (a `2026-07-28` session opened by `server/discover`, and an
//! `initialize` handshake at an earlier revision), advertised capabilities,
//! version negotiation, and the `tools/list` cache contract.

mod common;

use std::io::Read;

use common::{
    MCP_PROTOCOL_VERSION, call_tool, initialize_legacy_session, list_tools, open_session,
    read_response, read_stdout_line, setup_repo, shutdown_child, spawn_mcp_stdio, take_mcp_stderr,
    write_json_line,
};
use serde_json::{Value, json};

/// Every revision the server serves, oldest first.
const SUPPORTED_VERSIONS: [&str; 3] = ["2025-06-18", "2025-11-25", MCP_PROTOCOL_VERSION];

/// The newest revision that still has an `initialize` handshake.
const NEWEST_INITIALIZE_VERSION: &str = "2025-11-25";

/// Every tool the server is contracted to expose, in `tools/list` order.
const EXPECTED_TOOL_NAMES: [&str; 11] = [
    "pgs_scan",
    "pgs_status",
    "pgs_stage",
    "pgs_unstage",
    "pgs_commit",
    "pgs_log",
    "pgs_overview",
    "pgs_split_hunk",
    "pgs_plan_check",
    "pgs_plan_diff",
    "pgs_plan_apply",
];

fn assert_tools_capability_only(capabilities: &Value) {
    let capabilities = capabilities
        .as_object()
        .expect("capabilities must be an object");
    assert!(capabilities.contains_key("tools"));
    for absent in ["tasks", "prompts", "resources", "logging"] {
        assert!(
            !capabilities.contains_key(absent),
            "{absent} is not served; capabilities were {capabilities:?}"
        );
    }
}

fn legacy_status_call(repo_path: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "pgs_status", "arguments": { "repo_path": repo_path } }
    })
}

#[test]
fn mcp_stdio_discover_advertises_tools_capability_only() {
    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();

    let response = open_session(&mut stdin, &mut stdout);
    shutdown_child(child);

    let result = &response["result"];
    assert_tools_capability_only(&result["capabilities"]);
    assert_eq!(
        result["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
        "pgs-mcp"
    );
    assert!(
        !result["instructions"]
            .as_str()
            .expect("discover must carry instructions")
            .is_empty(),
        "instructions must not be blank: {result}"
    );
}

#[test]
fn mcp_stdio_discover_reports_every_supported_version() {
    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();

    let response = open_session(&mut stdin, &mut stdout);
    shutdown_child(child);

    let supported: Vec<&str> = response["result"]["supportedVersions"]
        .as_array()
        .expect("discover must list supported versions")
        .iter()
        .map(|version| version.as_str().expect("version is a string"))
        .collect();
    assert_eq!(supported, SUPPORTED_VERSIONS);
}

#[test]
fn mcp_stdio_initialize_at_each_legacy_revision_negotiates_it() {
    for version in ["2025-06-18", NEWEST_INITIALIZE_VERSION] {
        let (child, mut stdin, mut stdout) = spawn_mcp_stdio();

        let response = initialize_legacy_session(&mut stdin, &mut stdout, version);
        shutdown_child(child);

        let result = &response["result"];
        assert_eq!(result["protocolVersion"], version);
        assert_eq!(result["serverInfo"]["name"], "pgs-mcp");
        assert_tools_capability_only(&result["capabilities"]);
    }
}

// 2026-07-28 has no `initialize`, so a client that names it there (Claude Code
// does, after a successful discover) is answered with the newest revision that
// still has one, rather than refused.
#[test]
fn mcp_stdio_initialize_naming_the_modern_revision_falls_back_to_newest_legacy() {
    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();

    let response = initialize_legacy_session(&mut stdin, &mut stdout, MCP_PROTOCOL_VERSION);
    shutdown_child(child);

    assert_eq!(
        response["result"]["protocolVersion"],
        NEWEST_INITIALIZE_VERSION
    );
}

#[test]
fn mcp_stdio_initialize_below_the_floor_is_answered_with_a_supported_revision() {
    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();

    let response = initialize_legacy_session(&mut stdin, &mut stdout, "2024-11-05");
    shutdown_child(child);

    // The client must disconnect when the answer differs from its request; it is
    // never served the older contract.
    assert_eq!(
        response["result"]["protocolVersion"],
        NEWEST_INITIALIZE_VERSION
    );
}

#[test]
fn mcp_stdio_request_declaring_an_unsupported_version_is_rejected() {
    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();

    write_json_line(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "server/discover",
            "params": {
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": "2024-11-05",
                    "io.modelcontextprotocol/clientCapabilities": {}
                }
            }
        }),
    );

    let response = read_response(&mut stdout);
    shutdown_child(child);

    assert!(
        response.get("result").is_none(),
        "an unsupported revision must not be served: {response}"
    );
    assert_eq!(response["error"]["code"], -32022);
    let supported: Vec<&str> = response["error"]["data"]["supported"]
        .as_array()
        .expect("rejection must list the supported versions")
        .iter()
        .map(|version| version.as_str().expect("version is a string"))
        .collect();
    assert_eq!(supported, SUPPORTED_VERSIONS);
}

#[test]
fn mcp_stdio_ping_is_answered_on_a_legacy_session() {
    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();
    initialize_legacy_session(&mut stdin, &mut stdout, NEWEST_INITIALIZE_VERSION);

    write_json_line(
        &mut stdin,
        &json!({ "jsonrpc": "2.0", "id": 2, "method": "ping" }),
    );

    let response = read_response(&mut stdout);
    shutdown_child(child);

    assert_eq!(response["id"], 2);
    assert!(
        response.get("error").is_none(),
        "legacy ping failed: {response}"
    );
}

#[test]
fn mcp_stdio_list_tools_returns_the_full_frozen_tool_set() {
    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();
    open_session(&mut stdin, &mut stdout);

    let response = list_tools(&mut stdin, &mut stdout);
    shutdown_child(child);

    let tools = response["result"]["tools"]
        .as_array()
        .expect("tools/list must return a tools array");
    let names: Vec<&str> = tools
        .iter()
        .map(|tool| tool["name"].as_str().expect("tool name is a string"))
        .collect();

    assert_eq!(names, EXPECTED_TOOL_NAMES);

    for tool in tools {
        assert!(
            tool.get("execution").is_none(),
            "task execution metadata was removed: {tool}"
        );
        assert!(
            tool.get("taskSupport").is_none(),
            "task support metadata was removed: {tool}"
        );
    }
}

#[test]
fn mcp_stdio_list_tools_carries_cache_metadata() {
    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();
    open_session(&mut stdin, &mut stdout);

    let response = list_tools(&mut stdin, &mut stdout);
    shutdown_child(child);

    // rmcp would otherwise default to `ttlMs: 0` / `cacheScope: "private"`,
    // and Claude Code rejects a tools/list that omits them (claude-code#88128).
    let result = &response["result"];
    assert_eq!(result["ttlMs"], 3_600_000);
    assert_eq!(result["cacheScope"], "public");
}

#[test]
fn mcp_stdio_modern_call_tool_result_is_marked_complete() {
    let (repo_dir, _repo) = setup_repo();
    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();
    open_session(&mut stdin, &mut stdout);

    let response = call_tool(
        &mut stdin,
        &mut stdout,
        2,
        "pgs_status",
        &json!({ "repo_path": repo_dir.path().display().to_string() }),
    );
    shutdown_child(child);

    assert_eq!(response["result"]["resultType"], "complete");
    assert_eq!(response["result"]["isError"], false);
}

#[test]
fn mcp_stdio_legacy_session_lists_and_calls_every_tool_path() {
    let (repo_dir, _repo) = setup_repo();
    let repo_path = repo_dir.path().display().to_string();

    for version in ["2025-06-18", NEWEST_INITIALIZE_VERSION] {
        let (child, mut stdin, mut stdout) = spawn_mcp_stdio();
        initialize_legacy_session(&mut stdin, &mut stdout, version);

        write_json_line(
            &mut stdin,
            &json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/list", "params": {} }),
        );
        let listed = read_response(&mut stdout);
        let names: Vec<&str> = listed["result"]["tools"]
            .as_array()
            .unwrap_or_else(|| panic!("{version} tools/list failed: {listed}"))
            .iter()
            .map(|tool| tool["name"].as_str().expect("tool name is a string"))
            .collect();
        assert_eq!(names, EXPECTED_TOOL_NAMES, "{version}");

        write_json_line(&mut stdin, &legacy_status_call(&repo_path));
        let called = read_response(&mut stdout);
        shutdown_child(child);

        let result = &called["result"];
        assert_eq!(result["isError"], false, "{version}: {called}");
        assert_eq!(result["structuredContent"]["pgs"]["command"], "status");
        assert!(
            result.get("resultType").is_none(),
            "{version} predates resultType: {called}"
        );
    }
}

#[test]
fn mcp_stdio_stdout_contains_only_jsonrpc_messages() {
    let (mut child, mut stdin, mut stdout) = spawn_mcp_stdio();
    let mut stderr = take_mcp_stderr(&mut child);

    // Read raw lines rather than going through the parsed helpers: the point of
    // this test is that nothing but JSON-RPC ever reaches stdout.
    write_json_line(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": NEWEST_INITIALIZE_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "pgs-test-client", "version": "0.1.0" }
            }
        }),
    );
    let first_line = read_stdout_line(&mut stdout);

    write_json_line(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }),
    );

    write_json_line(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        }),
    );
    let second_line = read_stdout_line(&mut stdout);

    drop(stdin);
    shutdown_child(child);

    let mut remaining_stdout = String::new();
    stdout.read_to_string(&mut remaining_stdout).unwrap();

    let mut remaining_stderr = String::new();
    stderr.read_to_string(&mut remaining_stderr).unwrap();

    let mut stdout_lines = vec![first_line, second_line];
    stdout_lines.extend(
        remaining_stdout
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(ToOwned::to_owned),
    );

    for line in stdout_lines {
        let parsed: Value = serde_json::from_str(&line)
            .unwrap_or_else(|_| panic!("stdout contains non-JSON line: {line}"));
        assert_eq!(
            parsed["jsonrpc"], "2.0",
            "stdout line is not JSON-RPC: {line}"
        );
    }
}

#[test]
fn mcp_local_launch_example_works() {
    let (repo_dir, _repo) = setup_repo();
    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();
    open_session(&mut stdin, &mut stdout);

    let status_response = call_tool(
        &mut stdin,
        &mut stdout,
        2,
        "pgs_status",
        &json!({ "repo_path": repo_dir.path().display().to_string() }),
    );
    shutdown_child(child);

    assert_eq!(status_response["jsonrpc"], "2.0");
    assert_eq!(status_response["id"], 2);
    assert_eq!(status_response["result"]["isError"], false);
    assert_eq!(
        status_response["result"]["structuredContent"]["outcome"],
        "ok"
    );
    assert_eq!(
        status_response["result"]["structuredContent"]["pgs"]["command"],
        "status"
    );
}

// A 2026-07-28 client never sends `initialize`, so closing stdin is how its
// session normally ends. That must not look like a crash to the client.
#[test]
fn mcp_stdio_modern_session_exits_cleanly_when_the_client_closes_stdin() {
    let (mut child, mut stdin, mut stdout) = spawn_mcp_stdio();
    open_session(&mut stdin, &mut stdout);

    drop(stdin);
    let status = child.wait().unwrap();

    assert!(status.success(), "pgs-mcp exited with {status}");
}
