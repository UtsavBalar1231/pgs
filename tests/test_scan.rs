mod common;

use common::{commit_file, run_pgs, setup_repo, write_file};

#[test]
fn scan_empty_repo_returns_exit_code_1() {
    let (dir, _repo) = setup_repo();
    // No changes in working tree — should exit 1.
    run_pgs(dir.path(), &["scan"]).code(1);
}

#[test]
fn scan_modified_file_returns_compact_contract() {
    let (dir, repo) = setup_repo();
    commit_file(
        &repo,
        dir.path(),
        "hello.txt",
        "line1\nline2\n",
        "add hello",
    );
    write_file(dir.path(), "hello.txt", "line1\nline2\nline3\n");

    let output = run_pgs(dir.path(), &["scan"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["version"], "v1");
    assert_eq!(json["command"], "scan");
    assert_eq!(json["detail"], "compact");

    assert!(json["files"].is_array(), "expected files array");
    let files = json["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);

    let file = &files[0];
    assert_eq!(file["path"], "hello.txt");
    assert_eq!(file["status"]["type"], "Modified");
    assert_eq!(file["binary"], false);
    assert!(file.get("is_binary").is_none());
    assert_eq!(file["hunks_count"], 1);
    assert!(file.get("checksum").is_none());

    let hunks = file["hunks"].as_array().unwrap();
    assert!(!hunks.is_empty(), "expected at least one hunk");
    assert!(
        hunks[0].get("lines").is_none(),
        "compact format should not include lines field"
    );
    assert!(hunks[0].get("checksum").is_none());

    assert!(hunks[0]["id"].is_string(), "expected id to be a string");
    assert!(hunks[0].get("hunk_id").is_none());

    assert_eq!(json["summary"]["total_files"], 1);
    assert_eq!(json["summary"]["total_hunks"], 1);
    assert_eq!(json["summary"]["modified"], 1);
}

#[test]
fn scan_full_flag_includes_line_content() {
    let (dir, repo) = setup_repo();
    commit_file(
        &repo,
        dir.path(),
        "hello.txt",
        "line1\nline2\n",
        "add hello",
    );
    write_file(dir.path(), "hello.txt", "line1\nline2\nline3\n");

    let output = run_pgs(dir.path(), &["scan", "--full"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["version"], "v1");
    assert_eq!(json["command"], "scan");
    assert_eq!(json["detail"], "full");

    let files = json["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["binary"], false);

    let hunks = files[0]["hunks"].as_array().unwrap();
    assert!(!hunks.is_empty());
    let lines = hunks[0]["lines"].as_array().unwrap();
    assert!(!lines.is_empty(), "expected lines in full output");
    assert!(hunks[0]["id"].is_string());
    assert!(hunks[0].get("hunk_id").is_none());
    assert!(hunks[0]["checksum"].is_string());

    let first_line = &lines[0];
    assert!(first_line["origin"].is_string());
    assert!(first_line["content"].is_string());

    assert!(
        files[0]["checksum"].is_string(),
        "expected checksum in full output"
    );
}

#[test]
fn scan_file_filter_restricts_output() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "a.txt", "aaa\n", "add a");
    commit_file(&repo, dir.path(), "b.txt", "bbb\n", "add b");
    write_file(dir.path(), "a.txt", "aaa\nmodified\n");
    write_file(dir.path(), "b.txt", "bbb\nmodified\n");

    // Scan only a.txt via positional arg
    let output = run_pgs(dir.path(), &["scan", "a.txt"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let files = json["files"].as_array().unwrap();
    assert_eq!(files.len(), 1, "expected only one file in filtered output");
    assert_eq!(files[0]["path"], "a.txt");
    assert_eq!(json["detail"], "compact");
}

#[test]
fn scan_binary_file_is_flagged() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "data.bin", "text content\n", "add data");

    // Write binary content (contains null bytes)
    let binary_content: Vec<u8> = vec![0x00, 0x01, 0x02, 0xFF, 0xFE, 0x00, 0x03, 0x04];
    let full_path = dir.path().join("data.bin");
    std::fs::write(full_path, binary_content).unwrap();

    let output = run_pgs(dir.path(), &["scan"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let files = json["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["binary"], true, "expected binary to be true");
    assert!(files[0].get("is_binary").is_none());
    let hunks = files[0]["hunks"].as_array().unwrap();
    assert!(hunks.is_empty(), "binary files should have no hunks");
}

#[test]
fn scan_untracked_file_detected_as_added() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "existing.txt", "hello\n", "initial");

    // Write a brand-new file without adding to index
    write_file(dir.path(), "new_file.txt", "brand new content\n");

    // Compact scan
    let output = run_pgs(dir.path(), &["scan"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let files = json["files"].as_array().unwrap();
    let new_file = files
        .iter()
        .find(|f| f["path"] == "new_file.txt")
        .expect("untracked file should appear in scan");
    assert_eq!(new_file["status"]["type"], "Added");
    assert_eq!(new_file["binary"], false);
    assert!(new_file.get("is_binary").is_none());

    let full_output = run_pgs(dir.path(), &["scan", "--full"]).success();
    let full_stdout = String::from_utf8(full_output.get_output().stdout.clone()).unwrap();
    let full_json: serde_json::Value = serde_json::from_str(&full_stdout).unwrap();

    let full_files = full_json["files"].as_array().unwrap();
    let full_new = full_files
        .iter()
        .find(|f| f["path"] == "new_file.txt")
        .expect("untracked file in full scan");
    assert!(full_new["checksum"].is_string());
    let hunks = full_new["hunks"].as_array().unwrap();
    assert!(!hunks.is_empty(), "untracked file should have hunks");
    let lines = hunks[0]["lines"].as_array().unwrap();
    assert!(!lines.is_empty(), "hunks should have lines in full output");
}

#[test]
fn scan_directory_filter_returns_matching_files() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "root.txt", "root\n", "initial");
    write_file(dir.path(), "subdir/file1.txt", "content1\n");
    write_file(dir.path(), "subdir/file2.txt", "content2\n");

    let output = run_pgs(dir.path(), &["scan", "subdir"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let files = json["files"].as_array().unwrap();
    let paths: Vec<&str> = files.iter().map(|f| f["path"].as_str().unwrap()).collect();
    assert!(
        paths.contains(&"subdir/file1.txt"),
        "subdir/file1.txt should be present: {paths:?}"
    );
    assert!(
        paths.contains(&"subdir/file2.txt"),
        "subdir/file2.txt should be present: {paths:?}"
    );
}

#[test]
fn scan_directory_filter_with_trailing_slash() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "root.txt", "root\n", "initial");
    write_file(dir.path(), "subdir/file1.txt", "content1\n");
    write_file(dir.path(), "subdir/file2.txt", "content2\n");

    let output = run_pgs(dir.path(), &["scan", "subdir/"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let files = json["files"].as_array().unwrap();
    let paths: Vec<&str> = files.iter().map(|f| f["path"].as_str().unwrap()).collect();
    assert!(
        paths.contains(&"subdir/file1.txt"),
        "subdir/file1.txt should be present with trailing slash: {paths:?}"
    );
    assert!(
        paths.contains(&"subdir/file2.txt"),
        "subdir/file2.txt should be present with trailing slash: {paths:?}"
    );
}

#[test]
fn scan_exact_file_filter_still_works() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "root.txt", "root\n", "initial");
    write_file(dir.path(), "subdir/file1.txt", "content1\n");
    write_file(dir.path(), "subdir/file2.txt", "content2\n");

    let output = run_pgs(dir.path(), &["scan", "subdir/file1.txt"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let files = json["files"].as_array().unwrap();
    assert_eq!(files.len(), 1, "only one file should match: {files:?}");
    assert_eq!(files[0]["path"], "subdir/file1.txt");
}

#[test]
fn scan_directory_with_no_changes_returns_no_changes() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "dirA/file.txt", "aaa\n", "add dirA");
    commit_file(&repo, dir.path(), "dirB/file.txt", "bbb\n", "add dirB");
    // Modify only dirA
    write_file(dir.path(), "dirA/file.txt", "aaa modified\n");

    // Scan dirB — no changes there
    run_pgs(dir.path(), &["scan", "dirB"]).code(1);
}
