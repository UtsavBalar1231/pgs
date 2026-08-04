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

// Pure-deletion runs (deleted lines with no additions) have no new-side line
// number of their own; they occupy the gap between the surviving new-side lines
// on either side. A line range selects such a run only when it covers the
// survivor on both sides of the gap. A gap at the start or end of the file has
// only one side, so the range must cover the adjacent survivor plus one further
// line on that side — unless no further line exists, which is the one case where
// covering the single adjacent survivor suffices.

/// Build a fixture and return its directory: commit `base`, then write `work`.
fn setup_pure_deletion(base: &str, work: &str) -> tempfile::TempDir {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "f.txt", base, "init");
    write_file(dir.path(), "f.txt", work);
    drop(repo);
    dir
}

#[test]
fn stage_line_range_covering_one_side_of_interior_deletion_gap_is_rejected() {
    let dir = setup_pure_deletion("a\nD1\nD2\nb\nc\n", "a\nb\nc\n");

    // The gap sits between new lines 1 (`a`) and 2 (`b`). Range 2-2 covers only
    // the trailing survivor, so it names nothing that changed.
    run_pgs(dir.path(), &["stage", "f.txt:2-2"]).code(1);

    assert_eq!(staged_blob(dir.path(), "f.txt"), "a\nD1\nD2\nb\nc\n");
}

#[test]
fn stage_line_range_covering_both_sides_of_interior_deletion_gap_applies_deletion() {
    let dir = setup_pure_deletion("a\nD1\nD2\nb\nc\n", "a\nb\nc\n");

    run_pgs(dir.path(), &["stage", "f.txt:1-2"]).success();

    assert_eq!(staged_blob(dir.path(), "f.txt"), "a\nb\nc\n");
}

#[test]
fn stage_line_range_covering_one_side_of_deletion_gap_before_addition_run_is_rejected() {
    let dir = setup_pure_deletion("a\no1\no2\nb\n", "a\nb\nn1\n");

    // Two independent runs: a pure deletion in the gap between new lines 1 and
    // 2, and an addition at new line 3. Range 2-2 touches neither.
    run_pgs(dir.path(), &["stage", "f.txt:2-2"]).code(1);

    assert_eq!(staged_blob(dir.path(), "f.txt"), "a\no1\no2\nb\n");
}

#[test]
fn stage_full_new_line_range_applies_trailing_deletion() {
    let dir = setup_pure_deletion("a\nb\nD1\nD2\n", "a\nb\n");

    // The gap is at EOF: the range covers its neighbour (new line 2) and the
    // further line 1 behind it.
    run_pgs(dir.path(), &["stage", "f.txt:1-2"]).success();

    assert_eq!(staged_blob(dir.path(), "f.txt"), "a\nb\n");
}

#[test]
fn stage_line_range_naming_only_boundary_neighbour_of_leading_deletion_gap_is_rejected() {
    let dir = setup_pure_deletion("D1\nD2\na\nb\n", "a\nb\n");

    // The gap is at BOF; new line 1 (`a`) is its only neighbour. Naming just
    // that unchanged line must not apply the deletion above it.
    run_pgs(dir.path(), &["stage", "f.txt:1-1"]).code(1);

    assert_eq!(staged_blob(dir.path(), "f.txt"), "D1\nD2\na\nb\n");
}

#[test]
fn stage_line_range_naming_only_boundary_neighbour_of_trailing_deletion_gap_is_rejected() {
    let dir = setup_pure_deletion("a\nb\nD1\nD2\n", "a\nb\n");

    // The gap is at EOF; new line 2 (`b`) is its only neighbour.
    run_pgs(dir.path(), &["stage", "f.txt:2-2"]).code(1);

    assert_eq!(staged_blob(dir.path(), "f.txt"), "a\nb\nD1\nD2\n");
}

#[test]
fn stage_line_range_naming_sole_line_beside_leading_deletion_gap_applies_deletion() {
    let dir = setup_pure_deletion("D1\na\n", "a\n");

    // The BOF gap's one neighbour is also the file's only new-side line, so
    // there is no further line to demand. Without this fallback the deletion
    // would be unreachable and staging every new line would stop reproducing
    // the workdir.
    run_pgs(dir.path(), &["stage", "f.txt:1-1"]).success();

    assert_eq!(staged_blob(dir.path(), "f.txt"), "a\n");
}

#[test]
fn stage_line_range_naming_sole_line_beside_trailing_deletion_gap_applies_deletion() {
    let dir = setup_pure_deletion("a\nD1\n", "a\n");

    run_pgs(dir.path(), &["stage", "f.txt:1-1"]).success();

    assert_eq!(staged_blob(dir.path(), "f.txt"), "a\n");
}

#[test]
fn stage_line_range_with_zero_context_does_not_misread_interior_gap_as_file_boundary() {
    let dir = setup_pure_deletion("a\nD1\nD2\nb\nc\n", "a\nb\nc\n");

    // Whether a gap sits at a file boundary is read off the file's new-side line
    // count, not off whether the hunk emitted trailing context. At `--context 0`
    // an interior run carries no trailing context, and the older inference read
    // that as end of file and let a single-line range apply the deletion.
    run_pgs(dir.path(), &["--context", "0", "stage", "f.txt:1-1"]).code(1);

    assert_eq!(staged_blob(dir.path(), "f.txt"), "a\nD1\nD2\nb\nc\n");
}

#[test]
fn stage_full_new_line_range_reproduces_workdir_across_deletion_shapes() {
    let shapes: [(&str, &str, u32); 5] = [
        ("a\nD1\nD2\nb\nc\n", "a\nb\nc\n", 3),
        ("D1\nD2\na\nb\n", "a\nb\n", 2),
        ("a\nb\nD1\nD2\n", "a\nb\n", 2),
        ("a\no1\no2\nb\n", "a\nb\nn1\n", 3),
        ("k\no1\no2\no3\nk2\n", "k\nn1\nk2\n", 3),
    ];

    for (base, work, new_line_count) in shapes {
        let dir = setup_pure_deletion(base, work);
        let range = format!("f.txt:1-{new_line_count}");

        run_pgs(dir.path(), &["stage", &range]).success();

        assert_eq!(
            staged_blob(dir.path(), "f.txt"),
            work,
            "staging every new line of {base:?} -> {work:?} must reproduce the workdir"
        );
    }
}

#[test]
fn stage_unchanged_interior_lines_leaves_index_untouched_across_deletion_shapes() {
    // Each range names only unchanged lines: no gap fully enclosed, no addition.
    let shapes: [(&str, &str, &str); 3] = [
        ("a\nD1\nD2\nb\nc\n", "a\nb\nc\n", "f.txt:2-3"),
        ("a\no1\no2\nb\n", "a\nb\nn1\n", "f.txt:2-2"),
        ("a\nb\nD1\nD2\nc\nd\n", "a\nb\nc\nd\n", "f.txt:3-4"),
    ];

    for (base, work, range) in shapes {
        let dir = setup_pure_deletion(base, work);

        run_pgs(dir.path(), &["stage", range]).code(1);

        assert_eq!(
            staged_blob(dir.path(), "f.txt"),
            base,
            "selecting only unchanged lines ({range}) must not mutate the index"
        );
    }
}

/// Commit `base`, write `work`, then stage the whole file so HEAD -> index
/// carries the deletion that the unstage line range has to address.
fn setup_staged_pure_deletion(base: &str, work: &str) -> tempfile::TempDir {
    let dir = setup_pure_deletion(base, work);
    run_pgs(dir.path(), &["stage", "f.txt"]).success();
    dir
}

#[test]
fn unstage_line_range_covering_one_side_of_interior_deletion_gap_is_rejected() {
    let dir = setup_staged_pure_deletion("a\nD1\nD2\nb\nc\n", "a\nb\nc\n");

    run_pgs(dir.path(), &["unstage", "f.txt:2-2"]).code(1);

    assert_eq!(staged_blob(dir.path(), "f.txt"), "a\nb\nc\n");
}

#[test]
fn unstage_full_index_line_range_restores_trailing_deletion() {
    let dir = setup_staged_pure_deletion("a\nb\nD1\nD2\n", "a\nb\n");

    run_pgs(dir.path(), &["unstage", "f.txt:1-2"]).success();

    assert_eq!(staged_blob(dir.path(), "f.txt"), "a\nb\nD1\nD2\n");
}

// --- Unterminated last line (no trailing newline in the base) ---
//
// `similar::TextDiff::from_lines` tokenizes each line together with its
// terminator, so a file whose last line carries no `\n` yields a token that is
// unequal to the same text terminated. git models this identically: for base
// `x\nb` against work `x\nY\nb\n` the diff is `-b` / `+Y` / `+b`, where `-b` and
// `+b` are one line regaining its newline and `+Y` is the only real insertion.

/// Commit `base`, then write `work` — neither is newline-normalized.
fn setup_raw(base: &str, work: &str) -> tempfile::TempDir {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "f.txt", base, "init");
    write_file(dir.path(), "f.txt", work);
    drop(repo);
    dir
}

#[test]
fn stage_inserted_line_before_unterminated_last_line_keeps_that_line() {
    // `-b` pairs with `+b`, not with `+Y`, so selecting `Y` alone must not drag
    // the deletion of `b` in behind it.
    let dir = setup_raw("x\nb", "x\nY\nb\n");

    run_pgs(dir.path(), &["stage", "f.txt:2-2"]).success();

    assert_eq!(staged_blob(dir.path(), "f.txt"), "x\nY\nb");
}

#[test]
fn stage_inserted_line_before_terminated_last_line_is_unchanged() {
    let dir = setup_raw("x\nb\n", "x\nY\nb\n");

    run_pgs(dir.path(), &["stage", "f.txt:2-2"]).success();

    assert_eq!(staged_blob(dir.path(), "f.txt"), "x\nY\nb\n");
}

#[test]
fn stage_line_appended_after_unterminated_last_line_is_refused() {
    // Placing `c` after the unterminated `b` requires terminating `b` too — a
    // change the range never names — so the selection is not representable.
    let dir = setup_raw("a\nb", "a\nb\nc\n");

    let assert = run_pgs(dir.path(), &["stage", "f.txt:3-3"]).code(2);
    let output = assert.get_output();
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("unterminated_interior_line"),
        "expected `unterminated_interior_line` error code, got: {combined}"
    );

    assert_eq!(
        staged_blob(dir.path(), "f.txt"),
        "a\nb",
        "a refused selection must leave the index untouched"
    );
}

#[test]
fn stage_line_appended_after_terminated_last_line_is_unchanged() {
    let dir = setup_raw("a\nb\n", "a\nb\nc\n");

    run_pgs(dir.path(), &["stage", "f.txt:3-3"]).success();

    assert_eq!(staged_blob(dir.path(), "f.txt"), "a\nb\nc\n");
}

#[test]
fn stage_whole_hunk_of_unterminated_file_is_byte_exact() {
    let dir = setup_raw("a\nb", "a\nb\nc\n");

    let scan = run_pgs(dir.path(), &["scan", "--full"]).success();
    let scan_json: serde_json::Value =
        serde_json::from_slice(&scan.get_output().stdout).expect("scan json");
    let hunk_id = scan_json["files"][0]["hunks"][0]["id"]
        .as_str()
        .expect("hunk id")
        .to_owned();

    run_pgs(dir.path(), &["stage", &hunk_id]).success();

    assert_eq!(staged_blob(dir.path(), "f.txt"), "a\nb\nc\n");
}

#[test]
fn stage_whole_file_of_unterminated_file_is_byte_exact() {
    let dir = setup_raw("a\nb", "a\nb\nc");

    run_pgs(dir.path(), &["stage", "f.txt"]).success();

    assert_eq!(staged_blob(dir.path(), "f.txt"), "a\nb\nc");
}

#[test]
fn unstage_line_range_on_unterminated_head_restores_exact_head_bytes() {
    // HEAD is `a\nb` with no final newline; the index holds `a\n` after the
    // deletion of `b` was staged. Undoing that deletion must land on HEAD's
    // exact bytes — a phantom trailing newline leaves the file Modified and an
    // "unstage everything, verify clean" loop never converges.
    let dir = setup_raw("a\nb", "a\n");
    run_pgs(dir.path(), &["stage", "f.txt"]).success();
    assert_eq!(staged_blob(dir.path(), "f.txt"), "a\n");

    run_pgs(dir.path(), &["unstage", "f.txt:1-1"]).success();

    assert_eq!(staged_blob(dir.path(), "f.txt"), "a\nb");
}

#[test]
fn stage_line_range_on_emptied_file_reports_selection_empty() {
    let dir = setup_raw("a\nb\n", "");

    let assert = run_pgs(dir.path(), &["stage", "f.txt:1-1"]).code(1);
    let output = assert.get_output();
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("selection_empty"),
        "expected `selection_empty` on an emptied file, got: {combined}"
    );
}
