mod common;

use common::{commit_file, run_agstage, run_agstage_raw, setup_repo, write_file};

fn parse_marker(line: &str) -> (&str, serde_json::Value) {
    let mut parts = line.splitn(3, ' ');
    assert_eq!(parts.next(), Some("@@agstage:v1"));
    let kind = parts.next().unwrap();
    let payload = serde_json::from_str(parts.next().unwrap()).unwrap();
    (kind, payload)
}

#[test]
fn scan_stage_status_commit_workflow() {
    let (dir, repo) = setup_repo();
    commit_file(
        &repo,
        dir.path(),
        "feature.rs",
        "fn main() {\n}\n",
        "initial feature",
    );
    write_file(
        dir.path(),
        "feature.rs",
        "fn main() {\n    println!(\"hello\");\n}\n",
    );

    // 1. Scan — should find changes
    let scan_output = run_agstage(dir.path(), &["scan"]).success();
    let scan_stdout = String::from_utf8(scan_output.get_output().stdout.clone()).unwrap();
    let scan_json: serde_json::Value = serde_json::from_str(&scan_stdout).unwrap();

    let files = scan_json["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["path"], "feature.rs");
    assert!(scan_json["summary"]["total_hunks"].as_u64().unwrap() > 0);

    // 2. Stage — stage the file
    let stage_output = run_agstage(dir.path(), &["stage", "feature.rs"]).success();
    let stage_stdout = String::from_utf8(stage_output.get_output().stdout.clone()).unwrap();
    let stage_json: serde_json::Value = serde_json::from_str(&stage_stdout).unwrap();
    assert_eq!(stage_json["status"], "ok");

    // 3. Status — verify staged
    let status_output = run_agstage(dir.path(), &["status"]).success();
    let status_stdout = String::from_utf8(status_output.get_output().stdout.clone()).unwrap();
    let status_json: serde_json::Value = serde_json::from_str(&status_stdout).unwrap();

    let files = status_json["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["path"], "feature.rs");

    // 4. Commit — create commit
    let commit_output = run_agstage(dir.path(), &["commit", "-m", "feat: add println"]).success();
    let commit_stdout = String::from_utf8(commit_output.get_output().stdout.clone()).unwrap();
    let commit_json: serde_json::Value = serde_json::from_str(&commit_stdout).unwrap();
    assert!(commit_json["commit_hash"].is_string());
    assert_eq!(commit_json["message"], "feat: add println");

    // 5. After commit, status should show nothing staged
    let final_status = run_agstage(dir.path(), &["status"]).success();
    let final_stdout = String::from_utf8(final_status.get_output().stdout.clone()).unwrap();
    let final_json: serde_json::Value = serde_json::from_str(&final_stdout).unwrap();
    let final_files = final_json["files"].as_array().unwrap();
    assert!(
        final_files.is_empty(),
        "after commit, nothing should be staged"
    );

    // 6. After commit, scan should show no unstaged changes for that file
    let final_scan = run_agstage(dir.path(), &["scan"]);
    let final_code = final_scan.get_output().status.code().unwrap();
    // Either exit 0 with empty files or exit 1 (no changes)
    assert!(
        final_code == 0 || final_code == 1,
        "expected exit 0 or 1 after full commit, got {final_code}"
    );
}

#[test]
fn text_mode_scan_stage_status_commit_workflow() {
    let (dir, repo) = setup_repo();
    commit_file(
        &repo,
        dir.path(),
        "feature.rs",
        "fn main() {\n}\n",
        "initial feature",
    );
    write_file(
        dir.path(),
        "feature.rs",
        "fn main() {\n    println!(\"hello\");\n}\n",
    );

    let scan_output = run_agstage_raw(dir.path(), &["scan"]).success();
    let scan_stdout = String::from_utf8(scan_output.get_output().stdout.clone()).unwrap();
    let scan_lines: Vec<&str> = scan_stdout.lines().collect();
    let (scan_begin_kind, scan_begin_payload) = parse_marker(scan_lines[0]);
    assert_eq!(scan_begin_kind, "scan.begin");
    assert_eq!(scan_begin_payload["command"], "scan");
    let (scan_end_kind, scan_end_payload) = parse_marker(scan_lines[scan_lines.len() - 1]);
    assert_eq!(scan_end_kind, "scan.end");
    assert_eq!(scan_end_payload["command"], "scan");

    let stage_output = run_agstage_raw(dir.path(), &["stage", "feature.rs"]).success();
    let stage_stdout = String::from_utf8(stage_output.get_output().stdout.clone()).unwrap();
    let stage_lines: Vec<&str> = stage_stdout.lines().collect();
    let (stage_begin_kind, stage_begin_payload) = parse_marker(stage_lines[0]);
    assert_eq!(stage_begin_kind, "stage.begin");
    assert_eq!(stage_begin_payload["command"], "stage");
    assert_eq!(stage_begin_payload["status"], "ok");

    let status_output = run_agstage_raw(dir.path(), &["status"]).success();
    let status_stdout = String::from_utf8(status_output.get_output().stdout.clone()).unwrap();
    let status_lines: Vec<&str> = status_stdout.lines().collect();
    let (status_begin_kind, status_begin_payload) = parse_marker(status_lines[0]);
    assert_eq!(status_begin_kind, "status.begin");
    assert_eq!(status_begin_payload["command"], "status");
    assert_eq!(status_begin_payload["items"], 1);

    let commit_output =
        run_agstage_raw(dir.path(), &["commit", "-m", "feat: add println"]).success();
    let commit_stdout = String::from_utf8(commit_output.get_output().stdout.clone()).unwrap();
    let commit_lines: Vec<&str> = commit_stdout.lines().collect();
    assert_eq!(commit_lines.len(), 1);
    let (commit_kind, commit_payload) = parse_marker(commit_lines[0]);
    assert_eq!(commit_kind, "commit.result");
    assert_eq!(commit_payload["command"], "commit");
    assert_eq!(commit_payload["message"], "feat: add println");

    let final_status = run_agstage_raw(dir.path(), &["status"]).success();
    let final_stdout = String::from_utf8(final_status.get_output().stdout.clone()).unwrap();
    let final_lines: Vec<&str> = final_stdout.lines().collect();
    let (final_begin_kind, final_begin_payload) = parse_marker(final_lines[0]);
    assert_eq!(final_begin_kind, "status.begin");
    assert_eq!(final_begin_payload["items"], 0);

    let final_scan = run_agstage_raw(dir.path(), &["scan"]).code(1);
    let final_scan_stdout = String::from_utf8(final_scan.get_output().stdout.clone()).unwrap();
    let final_scan_lines: Vec<&str> = final_scan_stdout.lines().collect();
    let (error_kind, error_payload) = parse_marker(final_scan_lines[0]);
    assert_eq!(error_kind, "error");
    assert_eq!(error_payload["command"], "scan");
    assert_eq!(error_payload["phase"], "runtime");
    assert_eq!(error_payload["code"], "no_changes");
    assert_eq!(error_payload["exit_code"], 1);
}

#[test]
fn stage_unstage_is_idempotent() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "original\n", "add hello");
    write_file(dir.path(), "hello.txt", "original\nmodified\n");

    // Scan before staging
    let scan_before = run_agstage(dir.path(), &["scan", "--full"]).success();
    let before_stdout = String::from_utf8(scan_before.get_output().stdout.clone()).unwrap();
    let before_json: serde_json::Value = serde_json::from_str(&before_stdout).unwrap();

    let hunks_before = before_json["files"][0]["hunks"].as_array().unwrap().len();
    assert!(hunks_before > 0, "expected hunks before staging");

    // Stage
    run_agstage(dir.path(), &["stage", "hello.txt"]).success();

    // Unstage
    run_agstage(dir.path(), &["unstage", "hello.txt"]).success();

    // Scan after stage+unstage — should show the same changes as before
    let scan_after = run_agstage(dir.path(), &["scan", "--full"]).success();
    let after_stdout = String::from_utf8(scan_after.get_output().stdout.clone()).unwrap();
    let after_json: serde_json::Value = serde_json::from_str(&after_stdout).unwrap();

    let files_after = after_json["files"].as_array().unwrap();
    assert_eq!(files_after.len(), 1, "should still have one changed file");
    assert_eq!(files_after[0]["path"], "hello.txt");

    let hunks_after = files_after[0]["hunks"].as_array().unwrap().len();
    assert_eq!(
        hunks_before, hunks_after,
        "hunk count should be the same after stage+unstage round-trip"
    );
}

#[test]
fn scan_stage_commit_untracked_file_workflow() {
    let (dir, repo) = setup_repo();
    commit_file(
        &repo,
        dir.path(),
        "existing.rs",
        "fn existing() {}\n",
        "initial",
    );

    // Write a brand-new untracked file
    write_file(
        dir.path(),
        "new_feature.rs",
        "fn new_feature() {\n    println!(\"hello\");\n}\n",
    );

    // 1. Scan — should detect the untracked file as Added
    let scan_output = run_agstage(dir.path(), &["scan"]).success();
    let scan_stdout = String::from_utf8(scan_output.get_output().stdout.clone()).unwrap();
    let scan_json: serde_json::Value = serde_json::from_str(&scan_stdout).unwrap();

    let files = scan_json["files"].as_array().unwrap();
    let new_file = files
        .iter()
        .find(|f| f["path"] == "new_feature.rs")
        .expect("untracked file should appear in scan");
    assert_eq!(new_file["status"]["type"], "Added");

    // 2. Stage by path
    let stage_output = run_agstage(dir.path(), &["stage", "new_feature.rs"]).success();
    let stage_stdout = String::from_utf8(stage_output.get_output().stdout.clone()).unwrap();
    let stage_json: serde_json::Value = serde_json::from_str(&stage_stdout).unwrap();
    assert_eq!(stage_json["status"], "ok");

    // 3. Status — verify staged as Added
    let status_output = run_agstage(dir.path(), &["status"]).success();
    let status_stdout = String::from_utf8(status_output.get_output().stdout.clone()).unwrap();
    let status_json: serde_json::Value = serde_json::from_str(&status_stdout).unwrap();

    let files = status_json["files"].as_array().unwrap();
    let staged_file = files
        .iter()
        .find(|f| f["path"] == "new_feature.rs")
        .expect("new_feature.rs should be staged");
    assert_eq!(staged_file["status"]["type"], "Added");

    // 4. Commit
    let commit_output =
        run_agstage(dir.path(), &["commit", "-m", "feat: add new_feature"]).success();
    let commit_stdout = String::from_utf8(commit_output.get_output().stdout.clone()).unwrap();
    let commit_json: serde_json::Value = serde_json::from_str(&commit_stdout).unwrap();
    assert!(commit_json["commit_hash"].is_string());
    assert_eq!(commit_json["message"], "feat: add new_feature");

    // 5. Scan — should show no changes (exit code 1)
    run_agstage(dir.path(), &["scan"]).code(1);
}
