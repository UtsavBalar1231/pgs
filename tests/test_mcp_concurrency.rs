//! Per-repo mutation serialization for the `pgs-mcp` server.
//!
//! Mutating tools share one index per repository, so the server funnels them
//! through a per-repo lane in request order. These tests dispatch overlapping
//! `tools/call` requests without waiting for the first response and assert the
//! lane held: ordering is enforced by the server, not by the client's pacing.
//!
//! `PGS_MCP_TEST_STAGE_DELAY_MS` widens the window a queued request must wait
//! on. Without it the first mutation usually finishes before the second
//! arrives, and the lane is never exercised.

mod common;

use common::{
    collect_responses, commit_file, initialize_session, run_pgs, send_tool_call, setup_repo,
    shutdown_child, spawn_mcp_stdio_with_env, write_file, write_json_line,
};
use serde_json::{Value, json};

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

    send_tool_call(
        &mut stdin,
        2,
        "pgs_stage",
        &json!({
            "repo_path": dir.path().display().to_string(),
            "selections": ["hello.txt"]
        }),
    );
    send_tool_call(
        &mut stdin,
        3,
        "pgs_commit",
        &json!({
            "repo_path": repo.path().display().to_string(),
            "message": "feat: serialized commit"
        }),
    );

    let responses = collect_responses(&mut stdout, 2);
    shutdown_child(child);

    assert_eq!(
        responses[&2]["result"]["structuredContent"]["outcome"],
        "ok"
    );
    assert_eq!(
        responses[&3]["result"]["structuredContent"]["outcome"],
        "ok"
    );
    assert_eq!(
        responses[&3]["result"]["structuredContent"]["pgs"]["message"],
        "feat: serialized commit"
    );

    // The commit succeeds with content only if it ran after the stage released
    // the lane; had it overtaken the stage, the index would have been empty.
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.summary(), Ok(Some("feat: serialized commit")));
}

#[test]
fn mcp_cancelled_mutation_leaves_index_untouched() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "a.txt", "a1\na2\n", "add a");
    commit_file(&repo, dir.path(), "b.txt", "b1\nb2\n", "add b");
    write_file(dir.path(), "a.txt", "a1\na2\na3\n");
    write_file(dir.path(), "b.txt", "b1\nb2\nb3\n");

    let (child, mut stdin, mut stdout) =
        spawn_mcp_stdio_with_env(&[("PGS_MCP_TEST_STAGE_DELAY_MS", "200")]);
    initialize_session(&mut stdin, &mut stdout);

    let repo_path = dir.path().display().to_string();

    send_tool_call(
        &mut stdin,
        2,
        "pgs_stage",
        &json!({ "repo_path": repo_path, "selections": ["a.txt"] }),
    );
    send_tool_call(
        &mut stdin,
        3,
        "pgs_stage",
        &json!({ "repo_path": repo_path, "selections": ["b.txt"] }),
    );

    // Request 3 is queued behind request 2's delay, so this cancellation lands
    // while it is still waiting on the lane.
    write_json_line(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "notifications/cancelled",
            "params": {
                "requestId": 3,
                "reason": "cancelled by integration test"
            }
        }),
    );

    // A dry run is still a mutating tool, so it queues behind request 3 on the
    // same lane. Its response is the barrier proving request 3 has left the
    // lane -- without it there is no sleep-free way to know the cancellation
    // resolved before the index is inspected. It also doubles as a probe: had
    // request 3 run, b.txt would carry no unstaged diff and this would fail
    // with `file_not_in_diff` instead of reporting `dry_run`.
    send_tool_call(
        &mut stdin,
        4,
        "pgs_stage",
        &json!({ "repo_path": repo_path, "selections": ["b.txt"], "dry_run": true }),
    );

    let responses = collect_responses(&mut stdout, 2);
    shutdown_child(child);

    assert_eq!(
        responses[&2]["result"]["structuredContent"]["outcome"],
        "ok"
    );
    assert_eq!(
        responses[&4]["result"]["structuredContent"]["pgs"]["status"],
        "dry_run"
    );
    // MCP requires the receiver not to answer a cancelled request at all.
    assert!(
        !responses.contains_key(&3),
        "cancelled request must not receive a response: {responses:?}"
    );

    let status_output = run_pgs(dir.path(), &["status"]).success();
    let status_stdout = String::from_utf8(status_output.get_output().stdout.clone()).unwrap();
    let status_json: Value = serde_json::from_str(&status_stdout).unwrap();
    let files = status_json["files"].as_array().unwrap();
    assert_eq!(files.len(), 1, "cancelled stage must not touch the index");
    assert_eq!(files[0]["path"], "a.txt");
}
