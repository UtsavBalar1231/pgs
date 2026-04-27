mod common;

use common::{commit_file, setup_repo, write_file};

use pgs::cmd::mcp_adapter::{
    McpCommandRequest, McpScanRequest, McpStageRequest, McpTypedOutput, execute,
};

#[test]
fn mcp_adapter_scan_reuses_typed_command_output() {
    let (dir, repo) = setup_repo();
    commit_file(
        &repo,
        dir.path(),
        "hello.txt",
        "line1\nline2\n",
        "add hello",
    );
    write_file(dir.path(), "hello.txt", "line1\nline2\nline3\n");

    let request = McpCommandRequest::Scan(McpScanRequest {
        repo_path: dir.path().display().to_string(),
        context: 3,
        files: vec![],
        full: false,
    });

    let output = execute(request).expect("scan should succeed");

    match output {
        McpTypedOutput::Scan(scan) => {
            assert_eq!(scan.version, "v1");
            assert_eq!(scan.command.as_str(), "scan");
            assert_eq!(format!("{:?}", scan.detail), "Compact");
            assert_eq!(scan.files.len(), 1);
            assert_eq!(scan.files[0].path, "hello.txt");
            assert_eq!(scan.summary.total_files, 1);
            assert_eq!(scan.summary.total_hunks, 1);
        }
        other => panic!("expected typed scan output, got: {other:?}"),
    }
}

#[test]
fn mcp_adapter_preserves_pgs_error_metadata() {
    let (dir, _repo) = setup_repo();

    let request = McpCommandRequest::Scan(McpScanRequest {
        repo_path: dir.path().display().to_string(),
        context: 3,
        files: vec![],
        full: false,
    });

    let error = execute(request).expect_err("empty repo scan should return an error");

    assert_eq!(error.code, "no_changes");
    assert_eq!(error.exit_code, 1);
    assert_eq!(error.source.code(), error.code);
    assert_eq!(error.source.exit_code(), error.exit_code);
    assert!(matches!(error.source, pgs::error::PgsError::NoChanges));
}

/// Stage a symlink via the MCP adapter and verify the index blob contains the
/// link-target string. Proves the MCP surface goes through the same fixed
/// `read_workdir_for_blob` helper as the CLI.
#[cfg(unix)]
#[test]
fn stage_via_mcp_adapter_on_symlink_produces_correct_blob() {
    use std::os::unix::fs::symlink;
    use std::path::Path;

    let (dir, repo) = setup_repo();

    // Commit a target file so the workdir is not empty.
    commit_file(
        &repo,
        dir.path(),
        "target.bin",
        &"B".repeat(512),
        "add target.bin",
    );

    // Create untracked symlink in workdir.
    symlink("target.bin", dir.path().join("link")).expect("symlink");

    let request = McpCommandRequest::Stage(McpStageRequest {
        repo_path: dir.path().display().to_string(),
        selections: vec!["link".into()],
        exclude: vec![],
        dry_run: false,
        context: 3,
    });

    let output = execute(request).expect("stage via MCP should succeed");

    match output {
        McpTypedOutput::Operation(op) => {
            assert_eq!(op.items.len(), 1, "expected one staged item");
            assert_eq!(
                op.items[0].lines_affected, 10,
                "lines_affected must equal len(\"target.bin\")"
            );
        }
        other => panic!("expected Operation output, got: {other:?}"),
    }

    // Verify the blob in the index directly via git2.
    let repo2 = git2::Repository::open(dir.path()).expect("open repo");
    let index = repo2.index().expect("index");
    let entry = index
        .get_path(Path::new("link"), 0)
        .expect("link must be in index after MCP stage");

    let blob = repo2.find_blob(entry.id).expect("find blob");
    assert_eq!(
        blob.content(),
        b"target.bin",
        "MCP stage must write link-target string as blob, not referent bytes"
    );
    assert_eq!(
        entry.mode, 0o120_000,
        "MCP stage must set symlink mode 0o120000"
    );
}
