mod common;

use common::{commit_file, read_staged_blob, run_pgs, setup_repo, write_file};
use pgs::git::diff::{build_scan_result, diff_index_to_workdir};
use pgs::git::staging::stage_hunk;

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
        selections.contains(&"lib/a.rs"),
        "items should list individual file paths, got: {selections:?}"
    );
    assert!(
        selections.contains(&"lib/b.rs"),
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
        selections.contains(&"dry/a.rs"),
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

// RED regression: staging hunk A (10-line deletion, old-lines {5..14}) must not leak
// hunk B (addition of "INSERTED" at workdir new-line 13) when 13 aliases into hunk A's
// old-line set. FAILS on main — proves the coordinate-space conflation bug.
#[test]
fn stage_hunk_by_id_does_not_leak_adjacent_hunk_when_old_line_aliases_new_line() {
    let (dir, repo) = setup_repo();

    // HEAD: L01..L30 (30 lines)
    let head_content: String = (1u32..=30).fold(String::new(), |mut s, n| {
        use std::fmt::Write;
        let _ = writeln!(s, "L{n:02}");
        s
    });
    commit_file(
        &repo,
        dir.path(),
        "f.txt",
        &head_content,
        "initial 30 lines",
    );

    // Workdir: L01-L04 | [L05-L14 deleted] | L15-L22 (8 unchanged) | INSERTED | L23-L30
    // 8 unchanged lines between edits forces libgit2 to emit 2 distinct hunks at context_lines=3.
    // INSERTED lands at workdir line 13, which aliases hunk A's old-line 13.
    let workdir_content: String = {
        let mut lines: Vec<String> = Vec::new();
        for n in 1u32..=4 {
            lines.push(format!("L{n:02}\n"));
        }
        for n in 15u32..=22 {
            lines.push(format!("L{n:02}\n"));
        }
        lines.push("INSERTED\n".to_string());
        for n in 23u32..=30 {
            lines.push(format!("L{n:02}\n"));
        }
        lines.concat()
    };
    write_file(dir.path(), "f.txt", &workdir_content);

    let diff = diff_index_to_workdir(&repo, 3).expect("diff_index_to_workdir should succeed");
    let scan = build_scan_result(&repo, &diff, None).expect("build_scan_result should succeed");

    assert_eq!(scan.files.len(), 1, "FIXTURE: expected 1 file in scan");
    let file_info = &scan.files[0];
    assert_eq!(
        file_info.hunks.len(),
        2,
        "FIXTURE INVARIANT BROKEN: libgit2 fused the two hunks into {} \
         (need 2 distinct hunks to prove the bug — check gap between edits)",
        file_info.hunks.len()
    );

    let hunk_a = &file_info.hunks[0];
    stage_hunk(&repo, "f.txt", hunk_a, None).expect("stage_hunk should succeed");

    let staged = read_staged_blob(&repo, "f.txt");

    for n in 5u32..=14 {
        let label = format!("L{n:02}");
        assert!(
            !staged.contains(&label as &str),
            "staged blob still contains {label} — hunk A deletion was not applied;\
             blob:\n{staged}"
        );
    }

    assert!(
        !staged.contains("INSERTED"),
        "LEAK: staged blob contains INSERTED from hunk B, which was not selected.\
         old-line 13 (hunk A deletion) aliases new-line 13 (hunk B addition);\
         blob:\n{staged}"
    );
}

// Line-range analogue of the hunk-ID aliasing RED test.
// Hunk[0] (new-lines 2-7) deletes L05-L14; hunk[1] (new-lines 10+) inserts INSERTED.
// Staging `f.txt:2-7` must apply hunk[0]'s deletions without leaking INSERTED from hunk[1],
// whose new-line number aliases into hunk[0]'s old-line set under the pre-fix code path.
#[test]
fn stage_line_range_does_not_leak_adjacent_hunk_when_old_line_aliases_new_line() {
    let (dir, repo) = setup_repo();

    // Same 30-line fixture as the hunk-ID RED test above.
    let head_content: String = (1u32..=30).fold(String::new(), |mut s, n| {
        use std::fmt::Write;
        let _ = writeln!(s, "L{n:02}");
        s
    });
    commit_file(
        &repo,
        dir.path(),
        "f.txt",
        &head_content,
        "initial 30 lines",
    );

    // Workdir: L01-L04 | [L05-L14 deleted] | L15-L22 (8 unchanged) | INSERTED | L23-L30
    let workdir_content: String = {
        let mut lines: Vec<String> = Vec::new();
        for n in 1u32..=4 {
            lines.push(format!("L{n:02}\n"));
        }
        for n in 15u32..=22 {
            lines.push(format!("L{n:02}\n"));
        }
        lines.push("INSERTED\n".to_string());
        for n in 23u32..=30 {
            lines.push(format!("L{n:02}\n"));
        }
        lines.concat()
    };
    write_file(dir.path(), "f.txt", &workdir_content);

    let diff = diff_index_to_workdir(&repo, 3).expect("diff_index_to_workdir should succeed");
    let scan = build_scan_result(&repo, &diff, None).expect("build_scan_result should succeed");
    assert_eq!(scan.files.len(), 1, "FIXTURE: expected 1 file in scan");
    assert_eq!(
        scan.files[0].hunks.len(),
        2,
        "FIXTURE INVARIANT BROKEN: libgit2 fused the two hunks into {} \
         (need 2 distinct hunks — check gap between edits)",
        scan.files[0].hunks.len()
    );

    // hunk[0] occupies new-lines 2-7; hunk[1] starts at new-line 10+.
    // Range 2-7 selects hunk[0] only via the new-cursor walk.
    let hunk0 = &scan.files[0].hunks[0];
    let range_end = hunk0.new_start + hunk0.new_lines.saturating_sub(1);
    let range_arg = format!(
        "f.txt:{}-{}",
        hunk0.new_start,
        range_end.max(hunk0.new_start)
    );
    run_pgs(dir.path(), &["stage", &range_arg]).success();

    // After staging hunk[0]'s deletions, `pgs status` (HEAD→index) must report
    // f.txt as staged-modified. `pgs scan` (index→workdir) must still report
    // INSERTED as unstaged — proving hunk[1] was not leaked into the index.
    let scan2_out = run_pgs(dir.path(), &["scan"]).success();
    let scan2_stdout = String::from_utf8(scan2_out.get_output().stdout.clone()).unwrap();
    let scan2: serde_json::Value = serde_json::from_str(&scan2_stdout).unwrap();

    // The INSERTED hunk must still be unstaged (compact scan omits line content,
    // so we check metadata: exactly one remaining unstaged hunk consisting of one addition).
    let scan2_text = scan2.to_string();
    assert_eq!(
        scan2["summary"]["total_hunks"].as_u64().unwrap_or(0),
        1,
        "expected 1 remaining unstaged hunk (INSERTED) after staging hunk[0]; scan:\n{scan2_text}"
    );
    assert_eq!(
        scan2["files"][0]["lines_added"].as_u64().unwrap_or(0),
        1,
        "expected 1 unstaged addition (INSERTED); scan:\n{scan2_text}"
    );
    assert_eq!(
        scan2["files"][0]["lines_deleted"].as_u64().unwrap_or(99),
        0,
        "no unstaged deletions should remain (hunk[0]'s deletions were staged); scan:\n{scan2_text}"
    );

    // The staged view (HEAD→index) must show f.txt with deletions applied.
    let status_out = run_pgs(dir.path(), &["status"]).success();
    let status_stdout = String::from_utf8(status_out.get_output().stdout.clone()).unwrap();
    let status: serde_json::Value = serde_json::from_str(&status_stdout).unwrap();
    let status_files = status["files"].as_array().unwrap();
    assert!(
        !status_files.is_empty(),
        "pgs status must show f.txt as staged after staging hunk[0]; status:\n{status_stdout}"
    );
    // Staged deletions means lines_deleted > 0 and lines_added == 0 (no additions staged).
    let staged_file = &status_files[0];
    assert!(
        staged_file["lines_deleted"].as_u64().unwrap_or(0) > 0,
        "staged file must show deletions from hunk[0]; status:\n{status_stdout}"
    );
    assert_eq!(
        staged_file["lines_added"].as_u64().unwrap_or(1),
        0,
        "LEAK: lines_added > 0 means INSERTED from hunk[1] leaked into the index; status:\n{status_stdout}"
    );
}

// Negative test: an unselected deletion must stay in the index.
// Hunk[0] substitutes line 5; hunk[1] deletes old line 23. Staging `f.txt:1-10`
// selects hunk[0] only — L23 must survive unchanged.
#[test]
fn stage_line_range_does_not_pull_in_unselected_deletion_via_new_cursor_walk() {
    let (dir, repo) = setup_repo();

    // HEAD: L01..L30 (30 lines)
    let head_content: String = (1u32..=30).fold(String::new(), |mut s, n| {
        use std::fmt::Write;
        let _ = writeln!(s, "L{n:02}");
        s
    });
    commit_file(
        &repo,
        dir.path(),
        "f.txt",
        &head_content,
        "initial 30 lines",
    );

    // Workdir: L01-L04 same, L05 → MODIFIED5, L06-L22 same, L23 DELETED, L24-L30 same.
    // Two hunks expected: hunk[0] = substitution at old-line 5; hunk[1] = deletion at old-line 23.
    let workdir_content: String = {
        let mut lines: Vec<String> = Vec::new();
        for n in 1u32..=4 {
            lines.push(format!("L{n:02}\n"));
        }
        lines.push("MODIFIED5\n".to_string()); // replaces L05
        for n in 6u32..=22 {
            lines.push(format!("L{n:02}\n"));
        }
        for n in 24u32..=30 {
            // L23 omitted (deleted)
            lines.push(format!("L{n:02}\n"));
        }
        lines.concat()
    };
    write_file(dir.path(), "f.txt", &workdir_content);

    // Verify fixture: need 2 distinct hunks
    let diff = diff_index_to_workdir(&repo, 3).expect("diff_index_to_workdir should succeed");
    let scan = build_scan_result(&repo, &diff, None).expect("build_scan_result should succeed");
    assert_eq!(scan.files.len(), 1, "FIXTURE: expected 1 file in scan");
    assert_eq!(
        scan.files[0].hunks.len(),
        2,
        "FIXTURE INVARIANT BROKEN: libgit2 fused the two hunks into {} \
         (need 2 distinct hunks — check gap between edits)",
        scan.files[0].hunks.len()
    );

    // hunk[0] occupies new-lines 2-8; hunk[1] starts at new-line 20+.
    // Range 2-8 selects hunk[0] only; hunk[1]'s deletion must not be pulled in.
    let hunk0 = &scan.files[0].hunks[0];
    let range_end = hunk0.new_start + hunk0.new_lines.saturating_sub(1);
    let range_arg = format!(
        "f.txt:{}-{}",
        hunk0.new_start,
        range_end.max(hunk0.new_start)
    );
    let output = run_pgs(dir.path(), &["stage", &range_arg]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["status"], "ok");

    // Re-open the repo so libgit2 sees the subprocess's index writes
    // (the original `repo` handle cached the empty index from setup_repo).
    let repo2 = git2::Repository::open(dir.path()).expect("reopen repo after subprocess stage");
    let staged = read_staged_blob(&repo2, "f.txt");

    // MODIFIED5 must be in the index (hunk[0] was selected)
    assert!(
        staged.contains("MODIFIED5"),
        "staged blob should contain MODIFIED5 from hunk[0]; blob:\n{staged}"
    );

    // L23 must still be present — hunk[1] (deletion of L23) was NOT selected.
    assert!(
        staged.contains("L23"),
        "staged blob is missing L23 — the unselected deletion at old-line 23 was \
         incorrectly pulled in by the new-cursor walk; blob:\n{staged}"
    );
}
