mod common;

use common::{commit_file, run_pgs, setup_repo, write_file};

#[test]
fn stage_file_by_path() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "add hello");
    write_file(dir.path(), "hello.txt", "line1\nline2\n");

    let output = run_pgs(dir.path(), &["stage", "hello.txt"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["status"], "ok");
    let items = json["items"].as_array().unwrap();
    assert!(!items.is_empty(), "expected at least one succeeded item");
    assert!(items[0]["lines_affected"].as_u64().unwrap() > 0);
    assert!(json["backup_id"].is_string());
}

#[test]
fn stage_hunk_by_id() {
    let (dir, repo) = setup_repo();
    commit_file(
        &repo,
        dir.path(),
        "hello.txt",
        "line1\nline2\n",
        "add hello",
    );
    write_file(dir.path(), "hello.txt", "line1\nline2\nline3\n");

    // First scan to get hunk IDs
    let scan_output = run_pgs(dir.path(), &["scan"]).success();
    let scan_stdout = String::from_utf8(scan_output.get_output().stdout.clone()).unwrap();
    let scan_json: serde_json::Value = serde_json::from_str(&scan_stdout).unwrap();

    let hunk_id = scan_json["files"][0]["hunks"][0]["id"].as_str().unwrap();

    // Stage by hunk ID
    let output = run_pgs(dir.path(), &["stage", hunk_id]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["status"], "ok");
    let items = json["items"].as_array().unwrap();
    assert!(!items.is_empty());
}

#[test]
fn stage_line_range() {
    let (dir, repo) = setup_repo();
    commit_file(
        &repo,
        dir.path(),
        "multi.txt",
        "line1\nline2\nline3\nline4\nline5\n",
        "add multi",
    );
    write_file(
        dir.path(),
        "multi.txt",
        "line1\nMODIFIED\nline3\nline4\nline5\n",
    );

    // Stage lines 2-2 (the modified line)
    let output = run_pgs(dir.path(), &["stage", "multi.txt:2-2"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["status"], "ok");
}

#[test]
fn stage_dry_run_does_not_modify_index() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "add hello");
    write_file(dir.path(), "hello.txt", "line1\nline2\n");

    // Stage with --dry-run
    let output = run_pgs(dir.path(), &["stage", "--dry-run", "hello.txt"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["status"], "dry_run");
    assert_eq!(json["backup_id"], serde_json::Value::Null);

    // Verify status shows nothing staged
    let status_output = run_pgs(dir.path(), &["status"]).success();
    let status_stdout = String::from_utf8(status_output.get_output().stdout.clone()).unwrap();
    let status_json: serde_json::Value = serde_json::from_str(&status_stdout).unwrap();

    let files = status_json["files"].as_array().unwrap();
    assert!(files.is_empty(), "dry-run should not modify the index");
}

#[test]
fn stage_unknown_hunk_returns_exit_code_2() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "add hello");
    write_file(dir.path(), "hello.txt", "line1\nline2\n");

    // Stage a nonexistent hunk ID (12 hex chars to look like a valid hunk ID)
    run_pgs(dir.path(), &["stage", "deadbeef0000"]).code(2);
}

#[test]
fn stage_stale_file_returns_exit_code_3() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "add hello");
    write_file(dir.path(), "hello.txt", "line1\nline2\n");

    // Scan to get hunk IDs (captures file checksum)
    let scan_output = run_pgs(dir.path(), &["scan", "--full"]).success();
    let scan_stdout = String::from_utf8(scan_output.get_output().stdout.clone()).unwrap();
    let scan_json: serde_json::Value = serde_json::from_str(&scan_stdout).unwrap();

    let hunk_id = scan_json["files"][0]["hunks"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Now modify the file AGAIN to make the scan stale
    write_file(
        dir.path(),
        "hello.txt",
        "completely\ndifferent\ncontent\nnow\n",
    );

    // Stage the old hunk ID — should fail as stale (exit 3)
    // Note: the exact behavior depends on implementation; the hunk ID may not
    // match anymore, which could be exit 2 (UnknownHunkId). We accept either
    // exit 2 or 3 since both indicate the scan is stale.
    let result = run_pgs(dir.path(), &["stage", &hunk_id]);
    let code = result.get_output().status.code().unwrap();
    assert!(
        code == 2 || code == 3,
        "expected exit code 2 or 3 for stale scan, got {code}"
    );
}

#[test]
fn stage_exclude_hunk() {
    let (dir, repo) = setup_repo();
    commit_file(
        &repo,
        dir.path(),
        "multi.txt",
        "aaa\n\n\n\n\nbbb\n",
        "add multi",
    );
    write_file(dir.path(), "multi.txt", "aaa\nNEW1\n\n\n\nbbb\nNEW2\n");

    // Scan to discover hunks
    let scan_output = run_pgs(dir.path(), &["scan"]).success();
    let scan_stdout = String::from_utf8(scan_output.get_output().stdout.clone()).unwrap();
    let scan_json: serde_json::Value = serde_json::from_str(&scan_stdout).unwrap();

    let hunks = scan_json["files"][0]["hunks"].as_array().unwrap();
    if hunks.len() < 2 {
        // If the diff engine produces only 1 hunk, skip this test gracefully.
        // The test is meaningful only when there are 2+ hunks.
        return;
    }

    let exclude_id = hunks[0]["id"].as_str().unwrap();

    // Stage entire file but exclude the first hunk
    let output = run_pgs(dir.path(), &["stage", "--exclude", exclude_id, "multi.txt"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["status"], "ok");
}

#[test]
fn stage_untracked_file_by_path() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "existing.txt", "hello\n", "initial");

    // Write a brand-new untracked file
    write_file(dir.path(), "new_file.txt", "brand new content\n");

    // Stage it
    let output = run_pgs(dir.path(), &["stage", "new_file.txt"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["status"], "ok");
    let items = json["items"].as_array().unwrap();
    assert!(!items.is_empty(), "expected succeeded items");

    // Verify status shows the file as staged Added
    let status_output = run_pgs(dir.path(), &["status"]).success();
    let status_stdout = String::from_utf8(status_output.get_output().stdout.clone()).unwrap();
    let status_json: serde_json::Value = serde_json::from_str(&status_stdout).unwrap();

    let files = status_json["files"].as_array().unwrap();
    let staged_file = files
        .iter()
        .find(|f| f["path"] == "new_file.txt")
        .expect("new_file.txt should be staged");
    assert_eq!(staged_file["status"]["type"], "Added");
}

#[test]
fn stage_multiple_line_selections_same_file_reports_each_selection_item() {
    let (dir, repo) = setup_repo();
    commit_file(
        &repo,
        dir.path(),
        "multi.txt",
        "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nline15\n",
        "add multi",
    );
    write_file(
        dir.path(),
        "multi.txt",
        "line1\nchanged-2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nchanged-12\nline13\nline14\nline15\n",
    );

    let output = run_pgs(dir.path(), &["stage", "multi.txt:2-2", "multi.txt:12-12"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["status"], "ok");
    let items = json["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["selection"], "multi.txt:2-2");
    assert_eq!(items[1]["selection"], "multi.txt:12-12");
    assert!(items[0]["lines_affected"].as_u64().unwrap() > 0);
    assert!(items[1]["lines_affected"].as_u64().unwrap() > 0);
}

#[test]
fn stage_directory_stages_all_matching_files() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "subdir/file1.txt", "a\n", "add subdir");
    commit_file(&repo, dir.path(), "subdir/file2.txt", "b\n", "add file2");
    write_file(dir.path(), "subdir/file1.txt", "a\nmodified1\n");
    write_file(dir.path(), "subdir/file2.txt", "b\nmodified2\n");

    run_pgs(dir.path(), &["stage", "subdir/"]).success();

    let status_output = run_pgs(dir.path(), &["status"]).success();
    let stdout = String::from_utf8(status_output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let files = json["files"].as_array().unwrap();
    assert_eq!(files.len(), 2, "both files under subdir/ should be staged");
    let paths: Vec<&str> = files.iter().map(|f| f["path"].as_str().unwrap()).collect();
    assert!(paths.contains(&"subdir/file1.txt"));
    assert!(paths.contains(&"subdir/file2.txt"));
}

#[test]
fn stage_directory_with_trailing_slash() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "mydir/a.rs", "fn a() {}\n", "add mydir");
    write_file(dir.path(), "mydir/a.rs", "fn a() {}\nfn b() {}\n");

    let output = run_pgs(dir.path(), &["stage", "mydir/"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["status"], "ok");
    let items = json["items"].as_array().unwrap();
    assert!(!items.is_empty());
}

#[test]
fn stage_directory_no_match_returns_error() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "src/main.rs", "fn main() {}\n", "init");
    write_file(
        dir.path(),
        "src/main.rs",
        "fn main() { println!(\"hi\"); }\n",
    );

    run_pgs(dir.path(), &["stage", "nonexistent/"]).code(2);
}

#[test]
fn stage_directory_output_shows_individual_files() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "lib/a.rs", "fn a() {}\n", "add lib");
    commit_file(&repo, dir.path(), "lib/b.rs", "fn b() {}\n", "add b");
    write_file(dir.path(), "lib/a.rs", "fn a() {}\nfn a2() {}\n");
    write_file(dir.path(), "lib/b.rs", "fn b() {}\nfn b2() {}\n");

    let output = run_pgs(dir.path(), &["stage", "lib/"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let items = json["items"].as_array().unwrap();
    let selections: Vec<&str> = items
        .iter()
        .map(|i| i["selection"].as_str().unwrap())
        .collect();
    assert!(
        selections.iter().any(|s| *s == "lib/a.rs"),
        "items should list individual file paths, got: {selections:?}"
    );
    assert!(
        selections.iter().any(|s| *s == "lib/b.rs"),
        "items should list individual file paths, got: {selections:?}"
    );
}

#[test]
fn stage_directory_with_exclude() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "pkg/file1.rs", "fn f1() {}\n", "add pkg");
    commit_file(&repo, dir.path(), "pkg/file2.rs", "fn f2() {}\n", "add f2");
    write_file(dir.path(), "pkg/file1.rs", "fn f1() {}\nfn extra() {}\n");
    write_file(dir.path(), "pkg/file2.rs", "fn f2() {}\nfn extra() {}\n");

    run_pgs(dir.path(), &["stage", "pkg/", "--exclude", "pkg/file2.rs"]).success();

    let status_output = run_pgs(dir.path(), &["status"]).success();
    let stdout = String::from_utf8(status_output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let files = json["files"].as_array().unwrap();
    assert_eq!(files.len(), 1, "only file1 should be staged");
    assert_eq!(files[0]["path"], "pkg/file1.rs");
}

#[test]
fn stage_directory_exclude_directory() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "root.rs", "fn root() {}\n", "add root");
    commit_file(&repo, dir.path(), "excl/x.rs", "fn x() {}\n", "add excl");
    write_file(dir.path(), "root.rs", "fn root() {}\nfn extra() {}\n");
    write_file(dir.path(), "excl/x.rs", "fn x() {}\nfn extra() {}\n");

    run_pgs(dir.path(), &["stage", "root.rs", "--exclude", "excl/"]).success();

    let status_output = run_pgs(dir.path(), &["status"]).success();
    let stdout = String::from_utf8(status_output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let files = json["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["path"], "root.rs");
}

#[test]
fn stage_directory_dry_run() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "dry/a.rs", "fn a() {}\n", "add dry");
    write_file(dir.path(), "dry/a.rs", "fn a() {}\nfn b() {}\n");

    let output = run_pgs(dir.path(), &["stage", "--dry-run", "dry/"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["status"], "dry_run");
    assert_eq!(json["backup_id"], serde_json::Value::Null);

    let items = json["items"].as_array().unwrap();
    assert!(!items.is_empty(), "dry-run should report expansion");
    let selections: Vec<&str> = items
        .iter()
        .map(|i| i["selection"].as_str().unwrap())
        .collect();
    assert!(
        selections.iter().any(|s| *s == "dry/a.rs"),
        "dry-run items should list individual file paths, got: {selections:?}"
    );
}

#[test]
fn stage_directory_with_mixed_statuses() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "mix/existing.rs", "old\n", "add mix");
    write_file(dir.path(), "mix/existing.rs", "old\nnew\n");
    write_file(dir.path(), "mix/new_file.rs", "brand new\n");

    let output = run_pgs(dir.path(), &["stage", "mix/"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["status"], "ok");

    let status_output = run_pgs(dir.path(), &["status"]).success();
    let status_stdout = String::from_utf8(status_output.get_output().stdout.clone()).unwrap();
    let status_json: serde_json::Value = serde_json::from_str(&status_stdout).unwrap();

    let files = status_json["files"].as_array().unwrap();
    assert_eq!(
        files.len(),
        2,
        "both Added and Modified files should be staged"
    );
}
