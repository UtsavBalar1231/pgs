mod common;

use common::{
    call_tool, commit_file, initialize_session, setup_repo, shutdown_child, spawn_mcp_stdio,
    write_file,
};
use serde_json::json;
#[test]
fn mcp_scan_tool_compact_matches_cli_contract() {
    let (dir, repo) = setup_repo();
    commit_file(
        &repo,
        dir.path(),
        "hello.txt",
        "line1\nline2\n",
        "add hello",
    );
    write_file(dir.path(), "hello.txt", "line1\nline2\nline3\n");

    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();
    initialize_session(&mut stdin, &mut stdout);

    let response = call_tool(
        &mut stdin,
        &mut stdout,
        3,
        "pgs_scan",
        &json!({
            "repo_path": dir.path().display().to_string()
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
    assert_eq!(pgs["command"], "scan");
    assert_eq!(pgs["detail"], "compact");
    assert_eq!(pgs["summary"]["total_files"], 1);
    assert_eq!(pgs["summary"]["total_hunks"], 1);

    let files = pgs["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["path"], "hello.txt");
    // Compact scan now includes file_checksum so agents can use --expect without --full.
    assert!(files[0]["checksum"].as_str().is_some_and(|s| !s.is_empty()));

    let hunks = files[0]["hunks"].as_array().unwrap();
    assert_eq!(hunks.len(), 1);
    assert!(hunks[0].get("checksum").is_none());
    assert!(hunks[0].get("lines").is_none());

    let content = result["content"].as_array().unwrap();
    let summary = content[0]["text"].as_str().unwrap();
    assert!(summary.contains("Found 1 unstaged file(s) across 1 hunk(s)."));
}

#[test]
fn mcp_scan_tool_full_matches_cli_contract() {
    let (dir, repo) = setup_repo();
    commit_file(
        &repo,
        dir.path(),
        "hello.txt",
        "line1\nline2\n",
        "add hello",
    );
    write_file(dir.path(), "hello.txt", "line1\nline2\nline3\n");

    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();
    initialize_session(&mut stdin, &mut stdout);

    let response = call_tool(
        &mut stdin,
        &mut stdout,
        3,
        "pgs_scan",
        &json!({
            "repo_path": dir.path().display().to_string(),
            "full": true
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
    assert_eq!(pgs["command"], "scan");
    assert_eq!(pgs["detail"], "full");

    let files = pgs["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["path"], "hello.txt");
    assert!(files[0]["checksum"].is_string());

    let hunks = files[0]["hunks"].as_array().unwrap();
    assert_eq!(hunks.len(), 1);
    assert!(hunks[0]["checksum"].is_string());
    let lines = hunks[0]["lines"].as_array().unwrap();
    assert!(!lines.is_empty());
    assert!(lines[0]["origin"].is_string());
    assert!(lines[0]["content"].is_string());

    let content = result["content"].as_array().unwrap();
    let summary = content[0]["text"].as_str().unwrap();
    assert!(summary.contains("Found 1 unstaged file(s) across 1 hunk(s)."));
}

#[test]
fn mcp_scan_tool_returns_no_effect_for_empty_repo() {
    let (dir, _repo) = setup_repo();

    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();
    initialize_session(&mut stdin, &mut stdout);

    let response = call_tool(
        &mut stdin,
        &mut stdout,
        3,
        "pgs_scan",
        &json!({
            "repo_path": dir.path().display().to_string()
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

    let content = result["content"].as_array().unwrap();
    let summary = content[0]["text"].as_str().unwrap();
    assert!(summary.contains("no changes"));
}
