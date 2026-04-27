mod common;

use common::{commit_file, run_pgs, setup_repo};

/// End-to-end test: stage a symlink via the CLI and verify the blob in the
/// index contains the link-target string, not the referent's bytes.
#[cfg(unix)]
#[test]
fn stage_command_e2e_on_symlink_produces_correct_blob() {
    use std::os::unix::fs::symlink;
    use std::path::Path;

    let (dir, repo) = setup_repo();

    // Commit a "fat" target file so that the buggy path (reading file bytes)
    // would produce a blob much larger than the 10-byte target string.
    commit_file(
        &repo,
        dir.path(),
        "target.bin",
        &"A".repeat(1024),
        "add target.bin",
    );

    // Create the symlink in the workdir (not committed — it's an untracked addition).
    symlink("target.bin", dir.path().join("link")).expect("symlink");

    // Stage via CLI.
    let output = run_pgs(dir.path(), &["stage", "link"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    // The CLI envelope must report lines_affected == 10 (len("target.bin")).
    let items = json["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1, "expected one staged item");
    assert_eq!(
        items[0]["lines_affected"].as_u64().unwrap(),
        10,
        "lines_affected must equal the length of the link-target string"
    );

    // Read the blob from the index via git2 — no subprocess calls.
    let repo2 = git2::Repository::open(dir.path()).expect("open repo");
    let index = repo2.index().expect("index");
    let entry = index
        .get_path(Path::new("link"), 0)
        .expect("link must be in index");

    let blob = repo2.find_blob(entry.id).expect("find blob");
    assert_eq!(
        blob.content(),
        b"target.bin",
        "blob content must be the link-target string, not the referent's bytes"
    );

    assert_eq!(
        entry.mode, 0o120_000,
        "index entry mode must be 0o120000 for symlink"
    );
}
