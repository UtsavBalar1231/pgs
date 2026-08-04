//! Partial staging/unstaging inside a contiguous replace-run (adjacent
//! deletion+addition lines with no context between them).
//!
//! A line range selects new-side lines only. Every deletion in the run must be
//! paired with the addition that replaces it, or unselected original content is
//! silently dropped from the staged blob.

mod common;

use std::path::Path;

use common::{commit_file, run_pgs, setup_repo, write_file};

/// Read the staged blob through a freshly-opened repository handle.
///
/// `pgs` runs out of process here, so the `Repository` returned by `setup_repo`
/// holds a stale in-memory index snapshot.
fn staged_blob(dir: &Path, path: &str) -> String {
    let repo = git2::Repository::open(dir).expect("reopen repo");
    let index = repo.index().expect("open index");
    let entry = index
        .get_path(Path::new(path), 0)
        .expect("file should have an index entry");
    let blob = repo.find_blob(entry.id).expect("find blob");
    String::from_utf8(blob.content().to_vec()).expect("utf-8 blob")
}

/// commit: `keep1/AAA_old/BBB_old/CCC_old/keep2`, workdir: same with `_new`.
fn setup_three_line_replace_run() -> tempfile::TempDir {
    let (dir, repo) = setup_repo();
    commit_file(
        &repo,
        dir.path(),
        "f.txt",
        "keep1\nAAA_old\nBBB_old\nCCC_old\nkeep2\n",
        "init",
    );
    write_file(
        dir.path(),
        "f.txt",
        "keep1\nAAA_new\nBBB_new\nCCC_new\nkeep2\n",
    );
    drop(repo);
    dir
}

#[test]
fn stage_two_of_three_lines_in_replace_run_preserves_unselected_original() {
    let dir = setup_three_line_replace_run();

    run_pgs(dir.path(), &["stage", "f.txt:2-3"]).success();

    assert_eq!(
        staged_blob(dir.path(), "f.txt"),
        "keep1\nAAA_new\nBBB_new\nCCC_old\nkeep2\n"
    );
}

#[test]
fn stage_first_line_of_replace_run_preserves_remaining_originals() {
    let dir = setup_three_line_replace_run();

    run_pgs(dir.path(), &["stage", "f.txt:2-2"]).success();

    assert_eq!(
        staged_blob(dir.path(), "f.txt"),
        "keep1\nAAA_new\nBBB_old\nCCC_old\nkeep2\n"
    );
}

#[test]
fn stage_middle_line_of_replace_run_preserves_surrounding_originals() {
    let dir = setup_three_line_replace_run();

    run_pgs(dir.path(), &["stage", "f.txt:3-3"]).success();

    assert_eq!(
        staged_blob(dir.path(), "f.txt"),
        "keep1\nAAA_old\nBBB_new\nCCC_old\nkeep2\n"
    );
}

#[test]
fn stage_last_line_of_replace_run_preserves_preceding_originals() {
    let dir = setup_three_line_replace_run();

    run_pgs(dir.path(), &["stage", "f.txt:4-4"]).success();

    assert_eq!(
        staged_blob(dir.path(), "f.txt"),
        "keep1\nAAA_old\nBBB_old\nCCC_new\nkeep2\n"
    );
}

#[test]
fn stage_whole_file_across_replace_run_matches_workdir() {
    let dir = setup_three_line_replace_run();

    run_pgs(dir.path(), &["stage", "f.txt:2-4"]).success();

    assert_eq!(
        staged_blob(dir.path(), "f.txt"),
        "keep1\nAAA_new\nBBB_new\nCCC_new\nkeep2\n"
    );
}

#[test]
fn stage_ragged_run_with_more_deletions_than_additions_applies_net_removal() {
    let (dir, repo) = setup_repo();
    commit_file(
        &repo,
        dir.path(),
        "f.txt",
        "keep1\nA\nB\nC\nkeep2\n",
        "init",
    );
    write_file(dir.path(), "f.txt", "keep1\nZ\nkeep2\n");

    run_pgs(dir.path(), &["stage", "f.txt:2-2"]).success();

    assert_eq!(staged_blob(dir.path(), "f.txt"), "keep1\nZ\nkeep2\n");
}

#[test]
fn stage_ragged_run_with_more_additions_than_deletions_pairs_first_addition() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "f.txt", "keep1\nA\nkeep2\n", "init");
    write_file(dir.path(), "f.txt", "keep1\nX\nY\nZ\nkeep2\n");

    run_pgs(dir.path(), &["stage", "f.txt:4-4"]).success();

    // Only the surplus addition Z is selected; its unpaired original A survives.
    assert_eq!(staged_blob(dir.path(), "f.txt"), "keep1\nA\nZ\nkeep2\n");
}

#[test]
fn stage_ragged_run_selecting_paired_addition_replaces_original() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "f.txt", "keep1\nA\nkeep2\n", "init");
    write_file(dir.path(), "f.txt", "keep1\nX\nY\nZ\nkeep2\n");

    run_pgs(dir.path(), &["stage", "f.txt:2-2"]).success();

    assert_eq!(staged_blob(dir.path(), "f.txt"), "keep1\nX\nkeep2\n");
}

#[test]
fn stage_non_adjacent_changed_lines_stages_only_the_selected_one() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "f.txt", "l1\nl2\nl3\nl4\nl5\n", "init");
    write_file(dir.path(), "f.txt", "l1\nl2_mod\nl3\nl4_mod\nl5\n");

    run_pgs(dir.path(), &["stage", "f.txt:2-2"]).success();

    assert_eq!(staged_blob(dir.path(), "f.txt"), "l1\nl2_mod\nl3\nl4\nl5\n");
}

#[test]
fn unstage_two_of_three_lines_in_replace_run_keeps_unselected_staged_line() {
    let dir = setup_three_line_replace_run();
    run_pgs(dir.path(), &["stage", "f.txt"]).success();

    run_pgs(dir.path(), &["unstage", "f.txt:2-3"]).success();

    assert_eq!(
        staged_blob(dir.path(), "f.txt"),
        "keep1\nAAA_old\nBBB_old\nCCC_new\nkeep2\n"
    );
}

#[test]
fn unstage_first_line_of_replace_run_keeps_remaining_staged_lines() {
    let dir = setup_three_line_replace_run();
    run_pgs(dir.path(), &["stage", "f.txt"]).success();

    run_pgs(dir.path(), &["unstage", "f.txt:2-2"]).success();

    assert_eq!(
        staged_blob(dir.path(), "f.txt"),
        "keep1\nAAA_old\nBBB_new\nCCC_new\nkeep2\n"
    );
}

#[test]
fn unstage_last_line_of_replace_run_keeps_preceding_staged_lines() {
    let dir = setup_three_line_replace_run();
    run_pgs(dir.path(), &["stage", "f.txt"]).success();

    run_pgs(dir.path(), &["unstage", "f.txt:4-4"]).success();

    assert_eq!(
        staged_blob(dir.path(), "f.txt"),
        "keep1\nAAA_new\nBBB_new\nCCC_old\nkeep2\n"
    );
}
