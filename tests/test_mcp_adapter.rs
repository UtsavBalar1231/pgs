mod common;

use common::{commit_file, setup_repo, write_file};

use pgs::cmd::mcp_adapter::{McpCommandRequest, McpScanRequest, McpTypedOutput, execute};

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
