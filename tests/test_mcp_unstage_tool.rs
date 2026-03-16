mod common;

use std::fs;
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

fn call_unstage_tool(
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
                "name": "pgs_unstage",
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
fn mcp_unstage_tool_matches_cli_contract() {
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
    let unstage_tool = tools
        .iter()
        .find(|tool| tool["name"] == "pgs_unstage")
        .expect("tools/list should include pgs_unstage");
    let required = unstage_tool["inputSchema"]["required"].as_array().unwrap();
    assert!(
        required.iter().any(|field| field == "repo_path"),
        "pgs_unstage input schema should require repo_path"
    );
    assert!(
        required.iter().any(|field| field == "selections"),
        "pgs_unstage input schema should require selections"
    );

    let response = call_unstage_tool(
        &mut stdin,
        &mut stdout,
        &json!({
            "repo_path": dir.path().display().to_string(),
            "selections": ["hello.txt"]
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
    assert_eq!(pgs["command"], "unstage");
    assert_eq!(pgs["status"], "ok");
    assert!(pgs["backup_id"].is_string());

    let items = pgs["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["selection"], "hello.txt");
    assert!(items[0]["lines_affected"].as_u64().unwrap() > 0);

    let content = result["content"].as_array().unwrap();
    let summary = content[0]["text"].as_str().unwrap();
    assert!(summary.contains("Unstaged 1 selection(s)."));

    let status_output = run_pgs(dir.path(), &["status"]).success();
    let status_stdout = String::from_utf8(status_output.get_output().stdout.clone()).unwrap();
    let status_json: Value = serde_json::from_str(&status_stdout).unwrap();
    let files = status_json["files"].as_array().unwrap();
    assert!(files.is_empty(), "unstage should remove staged changes");
}

#[test]
fn mcp_unstage_tool_dry_run_matches_cli_contract() {
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

    let response = call_unstage_tool(
        &mut stdin,
        &mut stdout,
        &json!({
            "repo_path": dir.path().display().to_string(),
            "selections": ["hello.txt"],
            "dry_run": true
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
    assert_eq!(pgs["command"], "unstage");
    assert_eq!(pgs["status"], "dry_run");
    assert_eq!(pgs["backup_id"], Value::Null);

    let status_output = run_pgs(dir.path(), &["status"]).success();
    let status_stdout = String::from_utf8(status_output.get_output().stdout.clone()).unwrap();
    let status_json: Value = serde_json::from_str(&status_stdout).unwrap();
    let files = status_json["files"].as_array().unwrap();
    assert!(!files.is_empty(), "dry-run should not modify the index");
}

#[test]
fn mcp_unstage_tool_surfaces_retryable_conflict() {
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

    let lock_path = repo.path().join("index.lock");
    fs::write(&lock_path, b"locked").unwrap();

    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();
    initialize_session(&mut stdin, &mut stdout);

    let response = call_unstage_tool(
        &mut stdin,
        &mut stdout,
        &json!({
            "repo_path": dir.path().display().to_string(),
            "selections": ["hello.txt"]
        }),
    );

    shutdown_child(child);
    let _ = fs::remove_file(&lock_path);

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 3);

    let result = &response["result"];
    assert_eq!(result["isError"], true);

    let structured = &result["structuredContent"];
    assert_eq!(structured["outcome"], "error");
    assert!(structured.get("pgs").is_none());
    assert_eq!(structured["pgs_error"]["kind"], "retryable");
    assert_eq!(structured["pgs_error"]["code"], "index_locked");
    assert_eq!(structured["pgs_error"]["exit_code"], 3);
    assert_eq!(structured["pgs_error"]["retryable"], true);

    let guidance = structured["pgs_error"]["guidance"].as_str().unwrap();
    assert!(guidance.contains("lock"));
}
