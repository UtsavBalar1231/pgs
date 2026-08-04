#![allow(dead_code)]

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Stdio};

use assert_cmd::Command;
use git2::Repository;
use serde_json::{Value, json};
use tempfile::TempDir;

pub const MCP_PROTOCOL_VERSION: &str = pgs::mcp::PROTOCOL_VERSION;

/// Create a test repo with git identity and initial commit so HEAD exists.
pub fn setup_repo() -> (TempDir, Repository) {
    let dir = TempDir::new().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    {
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test").unwrap();
        config.set_str("user.email", "test@test.com").unwrap();
    }

    // Create initial empty commit so HEAD exists
    {
        let sig = repo.signature().unwrap();
        let tree_oid = {
            let mut index = repo.index().unwrap();
            index.write_tree().unwrap()
        };
        let tree = repo.find_tree(tree_oid).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
            .unwrap();
    }

    (dir, repo)
}

/// Write a file to the working directory, creating parent dirs as needed.
pub fn write_file(dir: &Path, rel_path: &str, content: &str) {
    let full = dir.join(rel_path);
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(full, content).unwrap();
}

/// Commit a file: write to working dir, add to index, create commit.
pub fn commit_file(repo: &Repository, dir: &Path, rel_path: &str, content: &str, message: &str) {
    write_file(dir, rel_path, content);
    let mut index = repo.index().unwrap();
    index.add_path(Path::new(rel_path)).unwrap();
    index.write().unwrap();
    let tree_oid = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_oid).unwrap();
    let sig = repo.signature().unwrap();
    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let parents: Vec<&git2::Commit<'_>> = parent.iter().collect();
    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
        .unwrap();
}

/// Build and run pgs with `--repo` pointed at the test repo.
pub fn run_pgs(dir: &Path, args: &[&str]) -> assert_cmd::assert::Assert {
    Command::new(assert_cmd::cargo::cargo_bin!("pgs"))
        .arg("--json")
        .arg("--repo")
        .arg(dir.to_str().unwrap())
        .args(args)
        .assert()
}

/// Read the UTF-8 blob content for a file from the current git index.
///
/// Panics if the file has no index entry or the blob cannot be found.
pub fn read_staged_blob(repo: &Repository, path: &str) -> String {
    let index = repo.index().expect("open index");
    let entry = index
        .get_path(Path::new(path), 0)
        .expect("file should have an index entry");
    let blob = repo.find_blob(entry.id).expect("find blob by oid");
    String::from_utf8(blob.content().to_vec()).expect("blob is valid UTF-8")
}

pub fn run_pgs_raw(dir: &Path, args: &[&str]) -> assert_cmd::assert::Assert {
    Command::new(assert_cmd::cargo::cargo_bin!("pgs"))
        .arg("--repo")
        .arg(dir.to_str().unwrap())
        .args(args)
        .assert()
}

// Every MCP integration test drives the real `pgs-mcp` binary over stdio, so the
// handshake shape lives here once: a protocol revision bump is a one-file edit.

/// Spawn `pgs-mcp` over stdio with no extra environment.
pub fn spawn_mcp_stdio() -> (Child, ChildStdin, BufReader<ChildStdout>) {
    spawn_mcp_stdio_with_env(&[])
}

/// Spawn `pgs-mcp` over stdio with additional environment variables set.
///
/// Stderr is piped but left attached to the returned [`Child`]; taking it would
/// close the read end and hand the server an EPIPE on its first log line.
pub fn spawn_mcp_stdio_with_env(
    envs: &[(&str, &str)],
) -> (Child, ChildStdin, BufReader<ChildStdout>) {
    let mut command = std::process::Command::new(assert_cmd::cargo::cargo_bin!("pgs-mcp"));
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    for (key, value) in envs {
        command.env(key, value);
    }

    let mut child = command.spawn().unwrap();
    let stdin = child.stdin.take().unwrap();
    let stdout = BufReader::new(child.stdout.take().unwrap());

    (child, stdin, stdout)
}

/// Detach the child's stderr pipe so a test can drain it after shutdown.
pub fn take_mcp_stderr(child: &mut Child) -> BufReader<ChildStderr> {
    BufReader::new(child.stderr.take().unwrap())
}

pub fn write_json_line(stdin: &mut ChildStdin, message: &Value) {
    writeln!(stdin, "{message}").unwrap();
    stdin.flush().unwrap();
}

pub fn read_stdout_line(stdout: &mut BufReader<ChildStdout>) -> String {
    let mut line = String::new();
    let bytes_read = stdout.read_line(&mut line).unwrap();
    assert!(bytes_read > 0, "expected a JSON-RPC line on stdout");
    line.trim_end_matches(['\n', '\r']).to_owned()
}

pub fn read_response(stdout: &mut BufReader<ChildStdout>) -> Value {
    serde_json::from_str(&read_stdout_line(stdout)).unwrap()
}

/// Read responses until `count` distinct request ids have been seen.
///
/// Concurrent requests may complete out of order, so responses are keyed by id
/// rather than assumed to arrive in send order.
pub fn collect_responses(stdout: &mut BufReader<ChildStdout>, count: usize) -> HashMap<u64, Value> {
    let mut responses = HashMap::new();

    while responses.len() < count {
        let response = read_response(stdout);
        let id = response["id"]
            .as_u64()
            .expect("response must include a numeric id");
        responses.insert(id, response);
    }

    responses
}

/// Complete the MCP handshake at the only supported protocol revision.
pub fn initialize_session(stdin: &mut ChildStdin, stdout: &mut BufReader<ChildStdout>) -> Value {
    write_json_line(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {
                    "name": "pgs-test-client",
                    "version": "0.1.0"
                }
            }
        }),
    );

    let response = read_response(stdout);
    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 1);
    assert_eq!(response["result"]["protocolVersion"], MCP_PROTOCOL_VERSION);

    write_json_line(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }),
    );

    response
}

/// Send `tools/list` as request id 2 and return the full JSON-RPC response.
pub fn list_tools(stdin: &mut ChildStdin, stdout: &mut BufReader<ChildStdout>) -> Value {
    write_json_line(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        }),
    );

    read_response(stdout)
}

/// Send `tools/call` and return the full JSON-RPC response.
pub fn call_tool(
    stdin: &mut ChildStdin,
    stdout: &mut BufReader<ChildStdout>,
    request_id: u64,
    name: &str,
    arguments: &Value,
) -> Value {
    send_tool_call(stdin, request_id, name, arguments);
    read_response(stdout)
}

/// Send `tools/call` without reading a response, for concurrent-dispatch tests.
pub fn send_tool_call(stdin: &mut ChildStdin, request_id: u64, name: &str, arguments: &Value) {
    write_json_line(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": arguments
            }
        }),
    );
}

pub fn shutdown_child(mut child: Child) {
    let _ = child.kill();
    let _ = child.wait();
}
