mod common;

use common::{commit_file, run_agstage, setup_repo, write_file};

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
    assert_eq!(stage_json["status"], "Ok");

    // 3. Status — verify staged
    let status_output = run_agstage(dir.path(), &["status"]).success();
    let status_stdout = String::from_utf8(status_output.get_output().stdout.clone()).unwrap();
    let status_json: serde_json::Value = serde_json::from_str(&status_stdout).unwrap();

    let staged = status_json["staged_files"].as_array().unwrap();
    assert_eq!(staged.len(), 1);
    assert_eq!(staged[0]["path"], "feature.rs");

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
    let final_staged = final_json["staged_files"].as_array().unwrap();
    assert!(
        final_staged.is_empty(),
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
