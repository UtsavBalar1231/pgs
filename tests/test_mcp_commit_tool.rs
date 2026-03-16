mod common;

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use common::{commit_file, run_pgs, setup_repo, write_file};
use serde_json::{Value, json};

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
                "clientInfo": {
                    "name": "pgs-test-client",
                    "version": "0.1.0"
                }
            }
        }),
    );

    let initialize_response: Value = serde_json::from_str(&read_stdout_line(stdout)).unwrap();
    assert_eq!(initialize_response["jsonrpc"], "2.0");
    assert_eq!(initialize_response["id"], 1);
    assert_eq!(
        initialize_response["result"]["protocolVersion"],
        common::MCP_PROTOCOL_VERSION_BASELINE
    );

    write_json_line(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }),
    );
}

fn list_tools(stdin: &mut ChildStdin, stdout: &mut BufReader<ChildStdout>) -> Value {
    write_json_line(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        }),
    );

    serde_json::from_str(&read_stdout_line(stdout)).unwrap()
}

fn call_commit_tool(
    stdin: &mut ChildStdin,
    stdout: &mut BufReader<ChildStdout>,
    arguments: &Value,
) -> Value {
    write_json_line(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "pgs_commit",
                "arguments": arguments
            }
        }),
    );

    serde_json::from_str(&read_stdout_line(stdout)).unwrap()
}

fn shutdown_child(mut child: Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn mcp_commit_tool_matches_cli_contract() {
    let (dir, repo) = setup_repo();
    commit_file(
        &repo,
        dir.path(),
        "hello.txt",
        "line1\nline2\n",
        "add hello",
    );
    write_file(dir.path(), "hello.txt", "line1\nline2\nline3\n");
    run_pgs(dir.path(), &["stage", "hello.txt"]).success();

    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();
    initialize_session(&mut stdin, &mut stdout);

    let list_response = list_tools(&mut stdin, &mut stdout);
    assert_eq!(list_response["jsonrpc"], "2.0");
    assert_eq!(list_response["id"], 2);

    let tools = list_response["result"]["tools"].as_array().unwrap();
    let commit_tool = tools
        .iter()
        .find(|tool| tool["name"] == "pgs_commit")
        .expect("tools/list should include pgs_commit");
    let required = commit_tool["inputSchema"]["required"].as_array().unwrap();
    assert!(
        required.iter().any(|field| field == "repo_path"),
        "pgs_commit input schema should require repo_path"
    );
    assert!(
        required.iter().any(|field| field == "message"),
        "pgs_commit input schema should require message"
    );

    let response = call_commit_tool(
        &mut stdin,
        &mut stdout,
        &json!({
            "repo_path": dir.path().display().to_string(),
            "message": "feat: add line3"
        }),
    );

    shutdown_child(child);

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 3);

    let result = &response["result"];
    assert_eq!(result["isError"], false);

    let structured = &result["structuredContent"];
    assert_eq!(structured["outcome"], "ok");
    let pgs = &structured["pgs"];
    assert_eq!(pgs["version"], "v1");
    assert_eq!(pgs["command"], "commit");
    let commit_hash = pgs["commit_hash"].as_str().unwrap();
    assert_eq!(commit_hash.len(), 40, "commit hash should be 40 hex chars");
    assert_eq!(pgs["message"], "feat: add line3");
    assert!(pgs["author"].as_str().unwrap().contains("Test"));
    assert_eq!(pgs["files_changed"], 1);
    assert_eq!(pgs["insertions"], 1);
    assert_eq!(pgs["deletions"], 0);

    let content = result["content"].as_array().unwrap();
    let summary = content[0]["text"].as_str().unwrap();
    assert!(summary.contains("Created commit"));
    assert!(summary.contains("affecting 1 file(s)."));
}

#[test]
fn mcp_commit_tool_returns_no_effect_when_nothing_staged() {
    let (dir, _repo) = setup_repo();

    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();
    initialize_session(&mut stdin, &mut stdout);

    let response = call_commit_tool(
        &mut stdin,
        &mut stdout,
        &json!({
            "repo_path": dir.path().display().to_string(),
            "message": "feat: no staged changes"
        }),
    );

    shutdown_child(child);

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 3);

    let result = &response["result"];
    assert_eq!(result["isError"], false);

    let structured = &result["structuredContent"];
    assert_eq!(structured["outcome"], "no_effect");
    assert!(structured.get("pgs").is_none());
    assert_eq!(structured["pgs_error"]["kind"], "no_effect");
    assert_eq!(structured["pgs_error"]["code"], "no_changes");
    assert_eq!(structured["pgs_error"]["exit_code"], 1);

    let content = result["content"].as_array().unwrap();
    let summary = content[0]["text"].as_str().unwrap();
    assert!(summary.contains("no changes"));
}

#[test]
fn mcp_commit_tool_requires_non_empty_message() {
    let (dir, _repo) = setup_repo();

    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();
    initialize_session(&mut stdin, &mut stdout);

    let response = call_commit_tool(
        &mut stdin,
        &mut stdout,
        &json!({
            "repo_path": dir.path().display().to_string(),
            "message": ""
        }),
    );

    shutdown_child(child);

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 3);
    assert!(response.get("result").is_none());
    assert!(
        response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("message must be a non-empty string")
    );
}
