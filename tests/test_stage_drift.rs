/// Tests for agent-perceived drift detection via `--expect PATH=SHA`.
///
/// The fix: `pgs stage --expect path=SHA0` compares the agent's captured
/// checksum (SHA0 from a prior scan) against the fresh `scan.file_checksum`
/// computed microseconds before staging. A mismatch → `StaleScan` (exit 3) with
/// zero index mutation.
mod common;

use common::{commit_file, run_pgs, setup_repo, write_file};

// ── helpers ──────────────────────────────────────────────────────────────────

/// Run `pgs scan --full --json` and return the file-level `checksum` for the given path.
fn scan_checksum(dir: &std::path::Path, rel_path: &str) -> String {
    let out = run_pgs(dir, &["scan", "--full"]).success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    json["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["path"].as_str() == Some(rel_path))
        .unwrap_or_else(|| panic!("file '{rel_path}' not found in scan output"))["checksum"]
        .as_str()
        .unwrap_or_else(|| panic!("no checksum for '{rel_path}' — binary or checksum skipped?"))
        .to_owned()
}

/// Return the current index OID for a file (used to verify no index mutation).
fn index_oid(repo: &git2::Repository, path: &str) -> Option<git2::Oid> {
    let index = repo.index().expect("open index");
    index.get_path(std::path::Path::new(path), 0).map(|e| e.id)
}

/// Count entries in `.git/pgs/backups/` (returns 0 if the directory does not exist).
///
/// Used to assert that a stale-scan abort fires before `create_backup`.
fn backup_count(git_dir: &std::path::Path) -> usize {
    let backup_dir = git_dir.join("pgs").join("backups");
    std::fs::read_dir(&backup_dir).map_or(0, |entries| entries.flatten().count())
}

// ── core E2E drift test ───────────────────────────────────────────────────────

/// An agent scans at T0, the file changes at T1, and the agent stages at T2
/// with the T0 checksum. pgs must detect the drift, return `StaleScan` (exit 3),
/// and leave the index byte-identical.
#[test]
fn stage_with_stale_expected_checksum_returns_exit3_and_leaves_index_unchanged() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "src/lib.rs", "fn foo() {}\n", "add lib");

    // Introduce an unstaged change so there is something to scan.
    write_file(dir.path(), "src/lib.rs", "fn foo() {}\nfn bar() {}\n");

    // T0: agent scans and captures the checksum.
    let sha0 = scan_checksum(dir.path(), "src/lib.rs");
    assert!(!sha0.is_empty(), "scan must produce a checksum");

    // Capture the index OID before staging (should stay unchanged after drift).
    let oid_before = index_oid(&repo, "src/lib.rs");

    // T1: file changes on disk between the agent's scan and its stage call.
    write_file(
        dir.path(),
        "src/lib.rs",
        "fn foo() {}\nfn bar() {}\nfn baz() {}\n",
    );

    // T2: agent stages with the T0 checksum → drift detected → exit 3.
    let expect_arg = format!("src/lib.rs={sha0}");
    let backups_before = backup_count(repo.path());
    let stale_result = run_pgs(
        dir.path(),
        &["stage", "src/lib.rs", "--expect", &expect_arg],
    )
    .failure()
    .code(3);

    // Error code must be "stale_scan" — exit 3 is shared by IndexLocked etc.
    let err_json: serde_json::Value = serde_json::from_slice(&stale_result.get_output().stdout)
        .expect("stale-scan error must emit JSON on stdout");
    assert_eq!(
        err_json["code"], "stale_scan",
        "stale abort must emit code 'stale_scan', got: {:?}",
        err_json["code"]
    );

    // No backup must be created — the stale-scan abort fires before create_backup.
    assert_eq!(
        backup_count(repo.path()),
        backups_before,
        "stale abort must not create a new backup"
    );

    // Index must be completely untouched.
    let oid_after = index_oid(&repo, "src/lib.rs");
    assert_eq!(
        oid_before, oid_after,
        "index OID must not change when StaleScan fires"
    );
}

/// After detecting drift and re-scanning, staging with the fresh checksum
/// must succeed.
#[test]
fn stage_with_fresh_expected_checksum_after_rescan_succeeds() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "app.rs", "fn main() {}\n", "add main");

    // Introduce a change.
    write_file(dir.path(), "app.rs", "fn main() {}\nfn helper() {}\n");

    // Agent re-scans and captures the fresh checksum.
    let fresh_sha = scan_checksum(dir.path(), "app.rs");

    // Stage with the correct (fresh) checksum.
    let expect_arg = format!("app.rs={fresh_sha}");
    let out = run_pgs(dir.path(), &["stage", "app.rs", "--expect", &expect_arg]).success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["status"], "ok");
}

/// Without `--expect`, staging still works as before (backward compat).
#[test]
fn stage_without_expect_works_unchanged() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "a.rs", "fn a() {}\n", "add a");
    write_file(dir.path(), "a.rs", "fn a() {}\nfn b() {}\n");

    let out = run_pgs(dir.path(), &["stage", "a.rs"]).success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["status"], "ok");
}

// ── edge cases ────────────────────────────────────────────────────────────────

/// Duplicate --expect entries for the same path are a user error (exit 2).
#[test]
fn stage_with_duplicate_expect_paths_returns_exit2() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "dup.rs", "fn x() {}\n", "add dup");
    write_file(dir.path(), "dup.rs", "fn x() {}\nfn y() {}\n");

    run_pgs(
        dir.path(),
        &[
            "stage",
            "dup.rs",
            "--expect",
            "dup.rs=aaa",
            "--expect",
            "dup.rs=bbb",
        ],
    )
    .failure()
    .code(2);
}

/// --expect naming a path that is not in the staging selection is a user error (exit 2).
#[test]
fn stage_with_expect_path_not_in_selection_returns_exit2() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "real.rs", "fn r() {}\n", "add real");
    write_file(dir.path(), "real.rs", "fn r() {}\nfn s() {}\n");

    // Stage real.rs but --expect on a different path not in the selection.
    run_pgs(
        dir.path(),
        &["stage", "real.rs", "--expect", "other.rs=deadbeef"],
    )
    .failure()
    .code(2);
}

/// --expect with invalid format (no `=`) is a user error (exit 2).
#[test]
fn stage_with_malformed_expect_returns_exit2() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "m.rs", "fn m() {}\n", "add m");
    write_file(dir.path(), "m.rs", "fn m() {}\nfn n() {}\n");

    run_pgs(dir.path(), &["stage", "m.rs", "--expect", "noequalssign"])
        .failure()
        .code(2);
}

/// line-range staging with `--expect` on a drifted file correctly fires `StaleScan`.
#[test]
fn stage_line_range_with_stale_expect_returns_exit3() {
    let (dir, repo) = setup_repo();
    commit_file(
        &repo,
        dir.path(),
        "lines.rs",
        "fn a() {}\nfn b() {}\nfn c() {}\n",
        "add lines",
    );
    write_file(
        dir.path(),
        "lines.rs",
        "fn a() {}\nfn b_new() {}\nfn c() {}\n",
    );

    // Agent captures the SHA at scan time.
    let sha0 = scan_checksum(dir.path(), "lines.rs");

    // Capture index state before the (expected-to-abort) stage call.
    let oid_before = index_oid(&repo, "lines.rs");
    let backups_before = backup_count(repo.path());

    // File drifts between scan and stage.
    write_file(
        dir.path(),
        "lines.rs",
        "fn a() {}\nfn b_newer() {}\nfn c() {}\n",
    );

    let expect_arg = format!("lines.rs={sha0}");
    let stale_result = run_pgs(
        dir.path(),
        &["stage", "lines.rs:2-2", "--expect", &expect_arg],
    )
    .failure()
    .code(3);

    // Error code must be "stale_scan" — exit 3 is shared by IndexLocked etc.
    let err_json: serde_json::Value = serde_json::from_slice(&stale_result.get_output().stdout)
        .expect("stale-scan error must emit JSON on stdout");
    assert_eq!(
        err_json["code"], "stale_scan",
        "line-range stale abort must emit code 'stale_scan', got: {:?}",
        err_json["code"]
    );

    // No backup must be created — the stale-scan abort fires before create_backup.
    assert_eq!(
        backup_count(repo.path()),
        backups_before,
        "line-range stale abort must not create a new backup"
    );

    // Index must be byte-identical to its pre-abort state.
    let oid_after = index_oid(&repo, "lines.rs");
    assert_eq!(
        oid_before, oid_after,
        "index OID must not change when line-range StaleScan fires"
    );
}
