mod common;

use common::{
    call_tool, commit_file, initialize_session, list_tools, run_pgs, setup_repo, shutdown_child,
    spawn_mcp_stdio, write_file,
};
use serde_json::json;
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
        !required.iter().any(|field| field == "message"),
        "message is now one of two alternative sources, so it cannot be schema-required"
    );
    let schema_properties = commit_tool["inputSchema"]["properties"]
        .as_object()
        .unwrap();
    assert!(
        schema_properties.contains_key("amend"),
        "pgs_commit input schema should expose optional amend flag"
    );
    assert!(
        schema_properties.contains_key("message"),
        "pgs_commit input schema should expose message"
    );
    assert!(
        schema_properties.contains_key("message_file"),
        "pgs_commit input schema should expose message_file"
    );

    let response = call_tool(
        &mut stdin,
        &mut stdout,
        3,
        "pgs_commit",
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
    assert_eq!(pgs["message"], "feat: add line3\n");
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
fn mcp_commit_tool_amends_head_when_requested() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "old subject");
    let old_head = repo.head().unwrap().peel_to_commit().unwrap();
    let old_parent_id = old_head.parent_id(0).unwrap();

    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();
    initialize_session(&mut stdin, &mut stdout);

    let response = call_tool(
        &mut stdin,
        &mut stdout,
        3,
        "pgs_commit",
        &json!({
            "repo_path": dir.path().display().to_string(),
            "message": "new subject\n\nBody from MCP.",
            "amend": true
        }),
    );

    shutdown_child(child);

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 3);
    assert_eq!(response["result"]["isError"], false);
    let pgs = &response["result"]["structuredContent"]["pgs"];
    assert_eq!(pgs["message"], "new subject\n\nBody from MCP.\n");
    assert_eq!(pgs["files_changed"], 1);

    let amended = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(
        amended.message_raw().unwrap(),
        "new subject\n\nBody from MCP.\n"
    );
    assert_eq!(amended.parent_id(0).unwrap(), old_parent_id);
}

#[test]
fn mcp_commit_tool_returns_no_effect_when_nothing_staged() {
    let (dir, _repo) = setup_repo();

    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();
    initialize_session(&mut stdin, &mut stdout);

    let response = call_tool(
        &mut stdin,
        &mut stdout,
        3,
        "pgs_commit",
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

    let response = call_tool(
        &mut stdin,
        &mut stdout,
        3,
        "pgs_commit",
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

#[test]
fn mcp_commit_tool_amend_blank_message_rejected_preserving_head_message() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "old subject");
    let head_before = repo.head().unwrap().peel_to_commit().unwrap().id();

    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();
    initialize_session(&mut stdin, &mut stdout);

    let response = call_tool(
        &mut stdin,
        &mut stdout,
        3,
        "pgs_commit",
        &json!({
            "repo_path": dir.path().display().to_string(),
            "message": "   ",
            "amend": true
        }),
    );

    shutdown_child(child);

    assert!(response.get("result").is_none(), "amend should be rejected");

    let head_after = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head_before, head_after.id());
    assert_eq!(head_after.message().unwrap(), "old subject");
}

/// The CLI and MCP front ends must agree: a blank message is refused on both,
/// and neither may leave a new commit behind.
#[test]
fn mcp_and_cli_blank_commit_message_agree_on_rejection() {
    let (cli_dir, cli_repo) = setup_repo();
    commit_file(&cli_repo, cli_dir.path(), "a.txt", "one\n", "seed");
    write_file(cli_dir.path(), "a.txt", "two\n");
    run_pgs(cli_dir.path(), &["stage", "a.txt"]).success();
    let cli_head_before = cli_repo.head().unwrap().peel_to_commit().unwrap().id();
    run_pgs(cli_dir.path(), &["commit", "-m", "  "]).code(2);
    let cli_head_after = cli_repo.head().unwrap().peel_to_commit().unwrap().id();

    let (mcp_dir, mcp_repo) = setup_repo();
    commit_file(&mcp_repo, mcp_dir.path(), "a.txt", "one\n", "seed");
    write_file(mcp_dir.path(), "a.txt", "two\n");
    run_pgs(mcp_dir.path(), &["stage", "a.txt"]).success();
    let mcp_head_before = mcp_repo.head().unwrap().peel_to_commit().unwrap().id();

    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();
    initialize_session(&mut stdin, &mut stdout);
    let response = call_tool(
        &mut stdin,
        &mut stdout,
        3,
        "pgs_commit",
        &json!({
            "repo_path": mcp_dir.path().display().to_string(),
            "message": "  "
        }),
    );
    shutdown_child(child);

    let mcp_head_after = mcp_repo.head().unwrap().peel_to_commit().unwrap().id();

    assert!(response.get("result").is_none(), "MCP should refuse");
    assert_eq!(cli_head_before, cli_head_after, "CLI created a commit");
    assert_eq!(mcp_head_before, mcp_head_after, "MCP created a commit");
}

// --- message_file ---

/// Spawn a server, issue one `pgs_commit` call, and shut down.
fn call_commit_tool(arguments: &serde_json::Value) -> serde_json::Value {
    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();
    initialize_session(&mut stdin, &mut stdout);
    let response = call_tool(&mut stdin, &mut stdout, 3, "pgs_commit", arguments);
    shutdown_child(child);
    response
}

#[test]
fn mcp_commit_tool_message_file_commits_message_from_file() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "a.txt", "one\n", "seed");
    write_file(dir.path(), "a.txt", "two\n");
    run_pgs(dir.path(), &["stage", "a.txt"]).success();

    let msg_path = dir.path().join("msg.txt");
    std::fs::write(&msg_path, "feat: from mcp file\r\n\n\nBody.  \n\n").unwrap();

    let response = call_commit_tool(&json!({
        "repo_path": dir.path().display().to_string(),
        "message_file": msg_path.display().to_string()
    }));

    assert_eq!(response["result"]["isError"], false);
    let pgs = &response["result"]["structuredContent"]["pgs"];
    assert_eq!(pgs["message"], "feat: from mcp file\n\nBody.\n");
    assert_eq!(
        repo.head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .message_raw()
            .unwrap(),
        "feat: from mcp file\n\nBody.\n"
    );
}

/// An MCP request has no stdin to read from, so `-` must be refused outright
/// rather than blocking the server on a stream that will never produce data.
#[test]
fn mcp_commit_tool_message_file_dash_rejected() {
    let (dir, _repo) = setup_repo();

    let response = call_commit_tool(&json!({
        "repo_path": dir.path().display().to_string(),
        "message_file": "-"
    }));

    assert!(response.get("result").is_none(), "`-` must be rejected");
}

#[test]
fn mcp_commit_tool_message_and_message_file_together_rejected() {
    let (dir, _repo) = setup_repo();
    let msg_path = dir.path().join("msg.txt");
    std::fs::write(&msg_path, "feat: from file\n").unwrap();

    let response = call_commit_tool(&json!({
        "repo_path": dir.path().display().to_string(),
        "message": "feat: inline",
        "message_file": msg_path.display().to_string()
    }));

    assert!(response.get("result").is_none(), "both sources must fail");
}

#[test]
fn mcp_commit_tool_without_message_or_message_file_rejected() {
    let (dir, _repo) = setup_repo();

    let response = call_commit_tool(&json!({
        "repo_path": dir.path().display().to_string()
    }));

    assert!(response.get("result").is_none(), "no source must fail");
}

/// CLI and MCP must agree: an unreadable message file is a user error on both
/// front ends, and neither may leave a commit behind.
#[test]
fn mcp_and_cli_missing_message_file_agree_on_user_error() {
    let (cli_dir, cli_repo) = setup_repo();
    commit_file(&cli_repo, cli_dir.path(), "a.txt", "one\n", "seed");
    write_file(cli_dir.path(), "a.txt", "two\n");
    run_pgs(cli_dir.path(), &["stage", "a.txt"]).success();
    let cli_head_before = cli_repo.head().unwrap().peel_to_commit().unwrap().id();
    let cli_missing = cli_dir.path().join("nope.txt");
    run_pgs(
        cli_dir.path(),
        &["commit", "-F", cli_missing.to_str().unwrap()],
    )
    .code(2);
    let cli_head_after = cli_repo.head().unwrap().peel_to_commit().unwrap().id();

    let (mcp_dir, mcp_repo) = setup_repo();
    commit_file(&mcp_repo, mcp_dir.path(), "a.txt", "one\n", "seed");
    write_file(mcp_dir.path(), "a.txt", "two\n");
    run_pgs(mcp_dir.path(), &["stage", "a.txt"]).success();
    let mcp_head_before = mcp_repo.head().unwrap().peel_to_commit().unwrap().id();

    let response = call_commit_tool(&json!({
        "repo_path": mcp_dir.path().display().to_string(),
        "message_file": mcp_dir.path().join("nope.txt").display().to_string()
    }));
    let mcp_head_after = mcp_repo.head().unwrap().peel_to_commit().unwrap().id();

    let pgs_error = &response["result"]["structuredContent"]["pgs_error"];
    assert_eq!(pgs_error["kind"], "user");
    assert_eq!(pgs_error["code"], "input_file_unreadable");
    assert_eq!(pgs_error["exit_code"], 2);
    assert_eq!(cli_head_before, cli_head_after, "CLI created a commit");
    assert_eq!(mcp_head_before, mcp_head_after, "MCP created a commit");
}
