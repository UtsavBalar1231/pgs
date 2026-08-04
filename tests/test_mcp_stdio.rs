//! Protocol-level coverage for the `pgs-mcp` stdio server: handshake shape,
//! advertised capabilities, modern-only version negotiation, and the
//! `tools/list` cache contract required by MCP `2026-07-28`.

mod common;

use std::io::Read;

use common::{
    MCP_PROTOCOL_VERSION, call_tool, initialize_session, list_tools, read_stdout_line, setup_repo,
    shutdown_child, spawn_mcp_stdio, take_mcp_stderr, write_json_line,
};
use serde_json::{Value, json};

/// A pre-2026 revision, used only to prove the server refuses it.
const LEGACY_PROTOCOL_VERSION: &str = "2025-11-25";

/// Every tool the server is contracted to expose, in `tools/list` order.
const EXPECTED_TOOL_NAMES: [&str; 10] = [
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
];

#[test]
fn mcp_stdio_initialize_advertises_tools_capability_only() {
    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();

    let initialize_response = initialize_session(&mut stdin, &mut stdout);
    let result = initialize_response["result"]
        .as_object()
        .expect("initialize response must include a result object");

    let capabilities = result["capabilities"]
        .as_object()
        .expect("initialize result must include capabilities");
    assert!(capabilities.contains_key("tools"));
    assert!(
        !capabilities.contains_key("tasks"),
        "task support was removed; capabilities were {capabilities:?}"
    );
    assert!(!capabilities.contains_key("prompts"));
    assert!(!capabilities.contains_key("resources"));
    assert_eq!(result["serverInfo"]["name"], "pgs-mcp");

    shutdown_child(child);
}

#[test]
fn mcp_stdio_initialize_at_supported_version_negotiates_it() {
    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();

    let response = initialize_session(&mut stdin, &mut stdout);

    assert_eq!(response["result"]["protocolVersion"], MCP_PROTOCOL_VERSION);
    assert_eq!(response["result"]["serverInfo"]["name"], "pgs-mcp");

    shutdown_child(child);
}

#[test]
fn mcp_stdio_initialize_at_pre_2026_version_never_negotiates_it() {
    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();

    write_json_line(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": LEGACY_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "pgs-test-client", "version": "0.1.0" }
            }
        }),
    );

    let response: Value = serde_json::from_str(&read_stdout_line(&mut stdout)).unwrap();
    shutdown_child(child);

    // rmcp answers an unsupported `initialize` with the server's own revision
    // rather than an error, so the refusal is the non-matching echo: the client
    // must abort per the MCP lifecycle and is never served the old contract.
    assert_eq!(response["id"], 1);
    assert_eq!(response["result"]["protocolVersion"], MCP_PROTOCOL_VERSION);
}

#[test]
fn mcp_stdio_request_declaring_a_pre_2026_version_is_rejected() {
    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();

    // Requests that carry their own version in `_meta` are validated against
    // the server's supported set, so this path rejects outright.
    write_json_line(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "server/discover",
            "params": {
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": LEGACY_PROTOCOL_VERSION,
                    "io.modelcontextprotocol/clientCapabilities": {}
                }
            }
        }),
    );

    let response: Value = serde_json::from_str(&read_stdout_line(&mut stdout)).unwrap();
    shutdown_child(child);

    assert!(
        response.get("result").is_none(),
        "a legacy-versioned request must not be served: {response}"
    );
    assert_eq!(response["error"]["code"], -32022);
    let supported: Vec<&str> = response["error"]["data"]["supported"]
        .as_array()
        .expect("rejection must list the supported versions")
        .iter()
        .map(|version| version.as_str().expect("version is a string"))
        .collect();
    assert_eq!(supported, vec![MCP_PROTOCOL_VERSION]);
}

#[test]
fn mcp_stdio_ping_is_not_a_known_method() {
    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();
    initialize_session(&mut stdin, &mut stdout);

    write_json_line(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "ping"
        }),
    );

    let ping_response: Value = serde_json::from_str(&read_stdout_line(&mut stdout)).unwrap();
    assert_eq!(ping_response["id"], 2);
    assert!(
        ping_response.get("result").is_none(),
        "ping was dropped in {MCP_PROTOCOL_VERSION}; got {ping_response}"
    );
    assert_eq!(ping_response["error"]["code"], -32601);

    shutdown_child(child);
}

#[test]
fn mcp_stdio_list_tools_returns_the_full_frozen_tool_set() {
    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();
    initialize_session(&mut stdin, &mut stdout);

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
    initialize_session(&mut stdin, &mut stdout);

    let response = list_tools(&mut stdin, &mut stdout);
    shutdown_child(child);

    // rmcp leaves both fields unset by default, which a spec-strict
    // 2026-07-28 client rejects outright (rust-sdk#1114). The server sets them
    // explicitly; this pins that it keeps doing so.
    let result = &response["result"];
    assert_eq!(result["ttlMs"], 3_600_000);
    assert_eq!(result["cacheScope"], "public");
}

#[test]
fn mcp_stdio_call_tool_result_is_marked_complete() {
    let (repo_dir, _repo) = setup_repo();
    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();
    initialize_session(&mut stdin, &mut stdout);

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
fn mcp_stdio_server_discover_reports_only_the_modern_version() {
    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();

    // `server/discover` runs before the handshake, so the client supplies its
    // own version and capabilities through request `_meta`.
    write_json_line(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "server/discover",
            "params": {
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": MCP_PROTOCOL_VERSION,
                    "io.modelcontextprotocol/clientCapabilities": {}
                }
            }
        }),
    );

    let response: Value = serde_json::from_str(&read_stdout_line(&mut stdout)).unwrap();
    shutdown_child(child);

    let result = &response["result"];
    let supported: Vec<&str> = result["supportedVersions"]
        .as_array()
        .expect("discover must list supported versions")
        .iter()
        .map(|version| version.as_str().expect("version is a string"))
        .collect();
    assert_eq!(supported, vec![MCP_PROTOCOL_VERSION]);

    assert!(
        result["capabilities"].get("tasks").is_none(),
        "task support was removed: {result}"
    );
    assert!(
        !result["instructions"]
            .as_str()
            .expect("discover must carry instructions")
            .is_empty()
    );
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
                "protocolVersion": MCP_PROTOCOL_VERSION,
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
    initialize_session(&mut stdin, &mut stdout);

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
