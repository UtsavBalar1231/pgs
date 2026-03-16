use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

mod common;

fn spawn_mcp_stdio() -> (
    Child,
    ChildStdin,
    BufReader<ChildStdout>,
    BufReader<ChildStderr>,
) {
    let mut child = Command::new(assert_cmd::cargo::cargo_bin!("pgs-mcp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let stdin = child.stdin.take().unwrap();
    let stdout = BufReader::new(child.stdout.take().unwrap());
    let stderr = BufReader::new(child.stderr.take().unwrap());

    (child, stdin, stdout, stderr)
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

fn shutdown_child(mut child: Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn mcp_stdio_initialize_returns_expected_capabilities() {
    let (child, mut stdin, mut stdout, _stderr) = spawn_mcp_stdio();

    write_json_line(
        &mut stdin,
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

    let initialize_response: Value = serde_json::from_str(&read_stdout_line(&mut stdout)).unwrap();
    assert_eq!(initialize_response["jsonrpc"], "2.0");
    assert_eq!(initialize_response["id"], 1);

    let result = initialize_response["result"]
        .as_object()
        .expect("initialize response must include a result object");
    assert_eq!(
        result["protocolVersion"],
        common::MCP_PROTOCOL_VERSION_BASELINE
    );
    let capabilities = result["capabilities"]
        .as_object()
        .expect("initialize result must include capabilities");
    assert!(capabilities.get("prompts").is_none());
    assert!(capabilities.get("resources").is_none());
    assert!(capabilities.get("tools").is_some());
    let tasks = capabilities["tasks"]
        .as_object()
        .expect("initialize result must advertise tasks capability");
    assert!(tasks.get("list").is_some());
    assert!(tasks.get("cancel").is_some());
    assert!(tasks["requests"].get("tools").is_some());
    assert_eq!(result["serverInfo"]["name"], "pgs-mcp");

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
            "method": "ping"
        }),
    );

    let ping_response: Value = serde_json::from_str(&read_stdout_line(&mut stdout)).unwrap();
    assert_eq!(ping_response["jsonrpc"], "2.0");
    assert_eq!(ping_response["id"], 2);
    assert!(ping_response.get("result").is_some());

    shutdown_child(child);
}

#[test]
fn mcp_stdio_stdout_contains_only_jsonrpc_messages() {
    let (child, mut stdin, mut stdout, mut stderr) = spawn_mcp_stdio();

    write_json_line(
        &mut stdin,
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
            "method": "ping"
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
    let (repo_dir, _repo) = common::setup_repo();
    let (child, mut stdin, mut stdout, _stderr) = spawn_mcp_stdio();

    write_json_line(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": common::MCP_PROTOCOL_VERSION_BASELINE,
                "capabilities": {},
                "clientInfo": {
                    "name": "other-project",
                    "version": "0.1.0"
                }
            }
        }),
    );

    let initialize_response: Value = serde_json::from_str(&read_stdout_line(&mut stdout)).unwrap();
    assert_eq!(initialize_response["jsonrpc"], "2.0");
    assert_eq!(initialize_response["id"], 1);
    assert_eq!(
        initialize_response["result"]["protocolVersion"],
        common::MCP_PROTOCOL_VERSION_BASELINE
    );
    assert_eq!(
        initialize_response["result"]["serverInfo"]["name"],
        "pgs-mcp"
    );

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
            "method": "tools/call",
            "params": {
                "name": "pgs_status",
                "arguments": {
                    "repo_path": repo_dir.path().display().to_string()
                }
            }
        }),
    );

    let status_response: Value = serde_json::from_str(&read_stdout_line(&mut stdout)).unwrap();
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

    shutdown_child(child);
}
