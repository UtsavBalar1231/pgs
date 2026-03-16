mod common;

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use common::{commit_file, run_pgs, setup_repo, write_file};
use serde_json::{Value, json};

fn spawn_mcp_stdio_with_env(envs: &[(&str, &str)]) -> (Child, ChildStdin, BufReader<ChildStdout>) {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("pgs-mcp"));
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

fn write_json_line(stdin: &mut ChildStdin, message: &Value) {
    writeln!(stdin, "{message}").unwrap();
    stdin.flush().unwrap();
}

fn read_response(stdout: &mut BufReader<ChildStdout>) -> Value {
    let mut line = String::new();
    let bytes_read = stdout.read_line(&mut line).unwrap();
    assert!(bytes_read > 0, "expected a JSON-RPC line on stdout");
    serde_json::from_str(line.trim_end_matches(['\n', '\r'])).unwrap()
}

fn collect_responses(stdout: &mut BufReader<ChildStdout>, count: usize) -> HashMap<u64, Value> {
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

fn initialize_session(stdin: &mut ChildStdin, stdout: &mut BufReader<ChildStdout>) -> Value {
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

    let response = read_response(stdout);
    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 1);
    assert_eq!(
        response["result"]["protocolVersion"],
        common::MCP_PROTOCOL_VERSION_BASELINE
    );

    write_json_line(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }),
    );

    response
}

fn call_tool(
    stdin: &mut ChildStdin,
    request_id: u64,
    name: &str,
    arguments: &Value,
    as_task: bool,
) {
    let mut params = json!({
        "name": name,
        "arguments": arguments,
    });

    if as_task {
        params["task"] = json!({});
    }

    write_json_line(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": "tools/call",
            "params": params,
        }),
    );
}

fn list_tasks(stdin: &mut ChildStdin, request_id: u64) {
    write_json_line(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": "tasks/list",
            "params": {}
        }),
    );
}

fn get_task(stdin: &mut ChildStdin, request_id: u64, task_id: &str) {
    write_json_line(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": "tasks/get",
            "params": {
                "taskId": task_id
            }
        }),
    );
}

fn get_task_result(stdin: &mut ChildStdin, request_id: u64, task_id: &str) {
    write_json_line(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": "tasks/result",
            "params": {
                "taskId": task_id
            }
        }),
    );
}

fn cancel_task(stdin: &mut ChildStdin, request_id: u64, task_id: &str) {
    write_json_line(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": "tasks/cancel",
            "params": {
                "taskId": task_id
            }
        }),
    );
}

fn notify_cancelled(stdin: &mut ChildStdin, request_id: u64) {
    write_json_line(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "notifications/cancelled",
            "params": {
                "requestId": request_id,
                "reason": "cancelled by integration test"
            }
        }),
    );
}

fn wait_for_task_completion(
    stdin: &mut ChildStdin,
    stdout: &mut BufReader<ChildStdout>,
    next_request_id: &mut u64,
    task_id: &str,
) -> Value {
    for _ in 0..128 {
        get_task(stdin, *next_request_id, task_id);
        let response = read_response(stdout);
        *next_request_id += 1;

        let status = response["result"]["status"]
            .as_str()
            .expect("tasks/get must return a status string");
        if status == "completed" {
            return response;
        }

        std::thread::yield_now();
    }

    panic!("task {task_id} did not complete after repeated tasks/get polling");
}

fn shutdown_child(mut child: Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn mcp_tasks_scan_and_status_supported() {
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
    write_file(dir.path(), "hello.txt", "line1\nline2\nline3\nline4\n");

    let (child, mut stdin, mut stdout) =
        spawn_mcp_stdio_with_env(&[("PGS_MCP_TEST_SCAN_DELAY_MS", "200")]);
    let initialize_response = initialize_session(&mut stdin, &mut stdout);

    let tasks = initialize_response["result"]["capabilities"]["tasks"]
        .as_object()
        .expect("initialize should advertise tasks capability");
    assert!(tasks.get("list").is_some());
    assert!(tasks.get("cancel").is_some());
    assert!(tasks["requests"]["tools"].get("call").is_some());

    call_tool(
        &mut stdin,
        2,
        "pgs_scan",
        &json!({
            "repo_path": dir.path().display().to_string()
        }),
        true,
    );
    let scan_response = read_response(&mut stdout);
    assert_eq!(scan_response["id"], 2);
    let scan_task_id = scan_response["result"]["task"]["taskId"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(scan_response["result"]["task"]["status"], "working");

    list_tasks(&mut stdin, 3);
    let list_response = read_response(&mut stdout);
    let listed_tasks = list_response["result"]["tasks"].as_array().unwrap();
    assert!(
        listed_tasks
            .iter()
            .any(|task| task["taskId"] == scan_task_id)
    );

    get_task(&mut stdin, 4, &scan_task_id);
    let get_scan_response = read_response(&mut stdout);
    assert_eq!(get_scan_response["result"]["taskId"], scan_task_id);
    assert_eq!(get_scan_response["result"]["status"], "working");

    cancel_task(&mut stdin, 5, &scan_task_id);
    let cancel_response = read_response(&mut stdout);
    assert_eq!(cancel_response["result"]["taskId"], scan_task_id);
    assert_eq!(cancel_response["result"]["status"], "cancelled");

    get_task(&mut stdin, 6, &scan_task_id);
    let cancelled_scan = read_response(&mut stdout);
    assert_eq!(cancelled_scan["result"]["status"], "cancelled");

    call_tool(
        &mut stdin,
        7,
        "pgs_status",
        &json!({
            "repo_path": dir.path().display().to_string()
        }),
        true,
    );
    let status_response = read_response(&mut stdout);
    assert_eq!(status_response["id"], 7);
    let status_task_id = status_response["result"]["task"]["taskId"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(status_response["result"]["task"]["status"], "working");

    let mut next_request_id = 8;
    let completed_status = wait_for_task_completion(
        &mut stdin,
        &mut stdout,
        &mut next_request_id,
        &status_task_id,
    );
    assert_eq!(completed_status["result"]["taskId"], status_task_id);
    assert_eq!(completed_status["result"]["status"], "completed");

    get_task_result(&mut stdin, next_request_id, &status_task_id);
    let task_result_response = read_response(&mut stdout);
    assert_eq!(task_result_response["id"], next_request_id);
    assert_eq!(task_result_response["result"]["isError"], false);
    assert_eq!(
        task_result_response["result"]["structuredContent"]["outcome"],
        "ok"
    );
    assert_eq!(
        task_result_response["result"]["structuredContent"]["pgs"]["command"],
        "status"
    );

    shutdown_child(child);
}

#[test]
fn mcp_tasks_rejected_for_mutating_tools() {
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

    let (child, mut stdin, mut stdout) = spawn_mcp_stdio_with_env(&[]);
    initialize_session(&mut stdin, &mut stdout);

    call_tool(
        &mut stdin,
        2,
        "pgs_stage",
        &json!({
            "repo_path": dir.path().display().to_string(),
            "selections": ["hello.txt"]
        }),
        true,
    );
    call_tool(
        &mut stdin,
        3,
        "pgs_unstage",
        &json!({
            "repo_path": dir.path().display().to_string(),
            "selections": ["hello.txt"]
        }),
        true,
    );
    call_tool(
        &mut stdin,
        4,
        "pgs_commit",
        &json!({
            "repo_path": dir.path().display().to_string(),
            "message": "feat: should be rejected"
        }),
        true,
    );

    let responses = collect_responses(&mut stdout, 3);
    for request_id in [2_u64, 3, 4] {
        let response = responses.get(&request_id).unwrap();
        assert!(response.get("result").is_none());
        assert_eq!(response["error"]["code"], -32602);
        assert!(
            response["error"]["message"]
                .as_str()
                .unwrap()
                .contains("task-based invocation")
        );
    }

    shutdown_child(child);
}

#[test]
fn mcp_mutating_requests_serialize_per_repo() {
    let (dir, repo) = setup_repo();
    commit_file(
        &repo,
        dir.path(),
        "hello.txt",
        "line1\nline2\n",
        "add hello",
    );
    write_file(dir.path(), "hello.txt", "line1\nline2\nline3\n");

    let (child, mut stdin, mut stdout) =
        spawn_mcp_stdio_with_env(&[("PGS_MCP_TEST_STAGE_DELAY_MS", "200")]);
    initialize_session(&mut stdin, &mut stdout);

    call_tool(
        &mut stdin,
        2,
        "pgs_stage",
        &json!({
            "repo_path": dir.path().display().to_string(),
            "selections": ["hello.txt"]
        }),
        false,
    );
    call_tool(
        &mut stdin,
        3,
        "pgs_commit",
        &json!({
            "repo_path": repo.path().display().to_string(),
            "message": "feat: serialized commit"
        }),
        false,
    );

    let responses = collect_responses(&mut stdout, 2);
    shutdown_child(child);

    let stage_response = responses.get(&2).unwrap();
    assert_eq!(
        stage_response["result"]["structuredContent"]["outcome"],
        "ok"
    );

    let commit_response = responses.get(&3).unwrap();
    assert_eq!(
        commit_response["result"]["structuredContent"]["outcome"],
        "ok"
    );
    assert_eq!(
        commit_response["result"]["structuredContent"]["pgs"]["message"],
        "feat: serialized commit"
    );

    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.summary(), Some("feat: serialized commit"));
}

#[test]
fn mcp_cancelled_mutation_preserves_atomicity() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "a.txt", "a1\na2\n", "add a");
    commit_file(&repo, dir.path(), "b.txt", "b1\nb2\n", "add b");
    write_file(dir.path(), "a.txt", "a1\na2\na3\n");
    write_file(dir.path(), "b.txt", "b1\nb2\nb3\n");

    let (child, mut stdin, mut stdout) =
        spawn_mcp_stdio_with_env(&[("PGS_MCP_TEST_STAGE_DELAY_MS", "200")]);
    initialize_session(&mut stdin, &mut stdout);

    call_tool(
        &mut stdin,
        2,
        "pgs_stage",
        &json!({
            "repo_path": dir.path().display().to_string(),
            "selections": ["a.txt"]
        }),
        false,
    );
    call_tool(
        &mut stdin,
        3,
        "pgs_stage",
        &json!({
            "repo_path": dir.path().display().to_string(),
            "selections": ["b.txt"]
        }),
        false,
    );
    notify_cancelled(&mut stdin, 3);

    let responses = collect_responses(&mut stdout, 2);
    shutdown_child(child);

    let first_stage = responses.get(&2).unwrap();
    assert_eq!(first_stage["result"]["structuredContent"]["outcome"], "ok");

    let cancelled_stage = responses.get(&3).unwrap();
    assert!(cancelled_stage.get("result").is_none());
    assert!(
        cancelled_stage["error"]["message"]
            .as_str()
            .unwrap()
            .contains("cancelled before execution started")
    );

    let status_output = run_pgs(dir.path(), &["status"]).success();
    let status_stdout = String::from_utf8(status_output.get_output().stdout.clone()).unwrap();
    let status_json: Value = serde_json::from_str(&status_stdout).unwrap();
    let files = status_json["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["path"], "a.txt");
}
