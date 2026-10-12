//! Per-repo mutation serialization for the `pgs-mcp` server.
//!
//! Mutating tools share one index per repository, so the server runs them one
//! at a time per repository. Each stage is a read-modify-write of the whole
//! index: two running at once would each write an index missing the other's
//! entry. These tests dispatch overlapping `tools/call` requests without waiting
//! for any response and assert no update was lost.
//!
//! Cancellation while queued is covered by unit tests in `src/mcp/runtime.rs`,
//! where the lock can be held deterministically.

mod common;

use common::{
    collect_responses, commit_file, open_session, send_tool_call, setup_repo, shutdown_child,
    spawn_mcp_stdio, write_file, write_json_line,
};
use serde_json::json;

const CONCURRENT_STAGES: u64 = 8;

#[test]
fn mcp_concurrent_stages_on_one_repo_keep_every_update() {
    let (dir, repo) = setup_repo();
    let paths: Vec<String> = (0..CONCURRENT_STAGES)
        .map(|n| format!("file{n}.txt"))
        .collect();
    for path in &paths {
        commit_file(&repo, dir.path(), path, "one\n", "add file");
        write_file(dir.path(), path, "one\ntwo\n");
    }

    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();
    open_session(&mut stdin, &mut stdout);

    let repo_path = dir.path().display().to_string();
    for (id, path) in (2..).zip(&paths) {
        send_tool_call(
            &mut stdin,
            id,
            "pgs_stage",
            &json!({ "repo_path": repo_path, "selections": [path] }),
        );
    }

    let responses = collect_responses(&mut stdout, paths.len());
    shutdown_child(child);

    for (id, response) in &responses {
        assert_eq!(
            response["result"]["structuredContent"]["outcome"], "ok",
            "request {id}: {response}"
        );
    }

    let index = git2::Repository::open(dir.path())
        .expect("reopen repo")
        .index()
        .expect("read index");
    let head_tree = repo.head().unwrap().peel_to_tree().unwrap();
    for path in &paths {
        let entry = index
            .get_path(std::path::Path::new(path), 0)
            .expect("staged entry");
        let head_entry = head_tree.get_name(path).expect("committed entry");
        assert_ne!(
            entry.id,
            head_entry.id(),
            "{path} lost its staged update to a concurrent stage"
        );
    }
}

// rmcp rejects some requests before `call_tool` runs. Such a request must not
// leave anything behind that blocks later mutations on the repository.
#[test]
fn mcp_mutation_rejected_before_dispatch_does_not_block_the_lane() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "f.txt", "a\n", "add f");
    write_file(dir.path(), "f.txt", "a\nb\n");
    let arguments = json!({
        "repo_path": dir.path().display().to_string(),
        "selections": ["f.txt"],
        "dry_run": true
    });

    let (child, mut stdin, mut stdout) = spawn_mcp_stdio();
    write_json_line(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "pgs_stage",
                "arguments": arguments,
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": "2024-11-05",
                    "io.modelcontextprotocol/clientCapabilities": {}
                }
            }
        }),
    );
    let rejected = collect_responses(&mut stdout, 1);
    assert_eq!(rejected[&1]["error"]["code"], -32022);

    send_tool_call(&mut stdin, 2, "pgs_stage", &arguments);
    let accepted = collect_responses(&mut stdout, 1);
    shutdown_child(child);

    assert_eq!(accepted[&2]["result"]["isError"], false, "{accepted:?}");
}
