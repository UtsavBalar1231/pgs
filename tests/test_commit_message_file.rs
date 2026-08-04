//! `pgs commit -F <path>` — external-file commit messages and the shared
//! `--cleanup=whitespace` message normalization applied to every message source.

mod common;

use std::fs;

use common::{commit_file, run_pgs, run_pgs_stdin, setup_repo, write_file};

/// Repo with one staged change, ready to commit.
fn repo_with_staged_change() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "add hello");
    write_file(dir.path(), "hello.txt", "line1\nline2\n");
    run_pgs(dir.path(), &["stage", "hello.txt"]).success();
    (dir, repo)
}

fn head_message(repo: &git2::Repository) -> String {
    repo.head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .message_raw()
        .unwrap()
        .to_owned()
}

fn json_of(output: &assert_cmd::assert::Assert) -> serde_json::Value {
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    serde_json::from_str(&stdout).unwrap()
}

#[test]
fn commit_message_file_commits_message_from_file() {
    let (dir, repo) = repo_with_staged_change();
    let msg_path = dir.path().join("msg.txt");
    fs::write(&msg_path, "feat: from file\n\nBody paragraph.\n").unwrap();

    let output = run_pgs(dir.path(), &["commit", "-F", msg_path.to_str().unwrap()]).success();

    let json = json_of(&output);
    assert_eq!(json["message"], "feat: from file\n\nBody paragraph.\n");
    assert_eq!(head_message(&repo), "feat: from file\n\nBody paragraph.\n");
}

#[test]
fn commit_message_file_long_flag_commits_message_from_file() {
    let (dir, repo) = repo_with_staged_change();
    let msg_path = dir.path().join("msg.txt");
    fs::write(&msg_path, "feat: long flag\n").unwrap();

    run_pgs(
        dir.path(),
        &["commit", "--message-file", msg_path.to_str().unwrap()],
    )
    .success();

    assert_eq!(head_message(&repo), "feat: long flag\n");
}

#[test]
fn commit_message_file_with_amend_rewrites_head_message() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "old subject");
    let old_head = repo.head().unwrap().peel_to_commit().unwrap().id();

    let msg_path = dir.path().join("msg.txt");
    fs::write(&msg_path, "new subject\n\nAmended body.\n").unwrap();

    run_pgs(
        dir.path(),
        &["commit", "--amend", "-F", msg_path.to_str().unwrap()],
    )
    .success();

    let amended = repo.head().unwrap().peel_to_commit().unwrap();
    assert_ne!(amended.id(), old_head);
    assert_eq!(
        amended.message_raw().unwrap(),
        "new subject\n\nAmended body.\n"
    );
}

#[test]
fn commit_message_file_missing_path_returns_exit_code_2() {
    let (dir, repo) = repo_with_staged_change();
    let head_before = repo.head().unwrap().peel_to_commit().unwrap().id();

    let missing = dir.path().join("does-not-exist.txt");
    let output = run_pgs(dir.path(), &["commit", "-F", missing.to_str().unwrap()]).code(2);

    let json = json_of(&output);
    assert_eq!(json["exit_code"], 2);
    assert_eq!(json["code"], "input_file_unreadable");
    assert!(
        json["message"]
            .as_str()
            .unwrap()
            .contains("does-not-exist.txt"),
        "message must name the path: {}",
        json["message"]
    );

    assert_eq!(
        head_before,
        repo.head().unwrap().peel_to_commit().unwrap().id()
    );
}

#[test]
fn commit_message_file_directory_path_returns_exit_code_2() {
    let (dir, _repo) = repo_with_staged_change();
    let subdir = dir.path().join("a-directory");
    fs::create_dir(&subdir).unwrap();

    let output = run_pgs(dir.path(), &["commit", "-F", subdir.to_str().unwrap()]).code(2);
    assert_eq!(json_of(&output)["code"], "input_file_unreadable");
}

#[test]
#[cfg(unix)]
fn commit_message_file_unreadable_permissions_returns_exit_code_2() {
    use std::os::unix::fs::PermissionsExt as _;

    let (dir, _repo) = repo_with_staged_change();
    let msg_path = dir.path().join("msg.txt");
    fs::write(&msg_path, "feat: secret\n").unwrap();
    fs::set_permissions(&msg_path, fs::Permissions::from_mode(0o000)).unwrap();

    // Running as root bypasses the permission bits entirely.
    if fs::read_to_string(&msg_path).is_ok() {
        return;
    }

    let output = run_pgs(dir.path(), &["commit", "-F", msg_path.to_str().unwrap()]).code(2);
    assert_eq!(json_of(&output)["code"], "input_file_unreadable");
}

#[test]
fn commit_message_file_non_utf8_returns_exit_code_2() {
    let (dir, _repo) = repo_with_staged_change();
    let msg_path = dir.path().join("msg.bin");
    fs::write(&msg_path, [0xffu8, 0xfe, 0x00, 0x41]).unwrap();

    let output = run_pgs(dir.path(), &["commit", "-F", msg_path.to_str().unwrap()]).code(2);
    assert_eq!(json_of(&output)["code"], "input_file_unreadable");
}

#[test]
fn commit_message_file_empty_returns_empty_commit_message_exit_2() {
    let (dir, repo) = repo_with_staged_change();
    let head_before = repo.head().unwrap().peel_to_commit().unwrap().id();

    let msg_path = dir.path().join("msg.txt");
    fs::write(&msg_path, "").unwrap();

    let output = run_pgs(dir.path(), &["commit", "-F", msg_path.to_str().unwrap()]).code(2);
    assert_eq!(json_of(&output)["code"], "empty_commit_message");
    assert_eq!(
        head_before,
        repo.head().unwrap().peel_to_commit().unwrap().id()
    );
}

#[test]
fn commit_message_file_whitespace_only_amend_preserves_head_message() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "old subject");
    let head_before = repo.head().unwrap().peel_to_commit().unwrap().id();

    let msg_path = dir.path().join("msg.txt");
    fs::write(&msg_path, "  \n\t\n\r\n   \n").unwrap();

    let output = run_pgs(
        dir.path(),
        &["commit", "--amend", "-F", msg_path.to_str().unwrap()],
    )
    .code(2);
    assert_eq!(json_of(&output)["code"], "empty_commit_message");

    let head_after = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head_after.id(), head_before);
    assert_eq!(head_after.message().unwrap(), "old subject");
}

#[test]
fn commit_message_and_message_file_together_returns_exit_code_2() {
    let (dir, _repo) = repo_with_staged_change();
    let msg_path = dir.path().join("msg.txt");
    fs::write(&msg_path, "feat: from file\n").unwrap();

    run_pgs(
        dir.path(),
        &[
            "commit",
            "-m",
            "feat: inline",
            "-F",
            msg_path.to_str().unwrap(),
        ],
    )
    .code(2);
}

#[test]
fn commit_without_message_or_message_file_returns_exit_code_2() {
    let (dir, _repo) = repo_with_staged_change();
    run_pgs(dir.path(), &["commit"]).code(2);
}

#[test]
fn commit_message_file_dash_reads_message_from_stdin() {
    let (dir, repo) = repo_with_staged_change();

    run_pgs_stdin(
        dir.path(),
        &["commit", "-F", "-"],
        "feat: from stdin\n\nStdin body.\n",
    )
    .success();

    assert_eq!(head_message(&repo), "feat: from stdin\n\nStdin body.\n");
}

#[test]
fn commit_message_file_dash_empty_stdin_returns_exit_code_2() {
    let (dir, repo) = repo_with_staged_change();
    let head_before = repo.head().unwrap().peel_to_commit().unwrap().id();

    let output = run_pgs_stdin(dir.path(), &["commit", "-F", "-"], "").code(2);
    assert_eq!(json_of(&output)["code"], "empty_commit_message");
    assert_eq!(
        head_before,
        repo.head().unwrap().peel_to_commit().unwrap().id()
    );
}

// --- Normalization, observed end to end through the CLI ---

/// Every message source is normalized with git's `--cleanup=whitespace` rules.
fn assert_commit_message_normalizes(raw: &str, expected: &str) {
    let (dir, repo) = repo_with_staged_change();
    let msg_path = dir.path().join("msg.txt");
    fs::write(&msg_path, raw).unwrap();

    run_pgs(dir.path(), &["commit", "-F", msg_path.to_str().unwrap()]).success();

    assert_eq!(head_message(&repo), expected, "raw input: {raw:?}");
}

#[test]
fn commit_message_file_crlf_normalized_to_lf() {
    assert_commit_message_normalizes("subject\r\nbody\r\n", "subject\nbody\n");
}

#[test]
fn commit_message_file_trailing_whitespace_stripped_per_line() {
    assert_commit_message_normalizes("subject   \t\nbody  \n", "subject\nbody\n");
}

#[test]
fn commit_message_file_leading_blank_lines_removed() {
    assert_commit_message_normalizes("\n\nsubject\nbody\n", "subject\nbody\n");
}

#[test]
fn commit_message_file_consecutive_blank_lines_collapsed() {
    assert_commit_message_normalizes("subject\n\n\n\nbody\n", "subject\n\nbody\n");
}

#[test]
fn commit_message_file_missing_trailing_newline_gets_exactly_one() {
    assert_commit_message_normalizes("subject\nbody", "subject\nbody\n");
}

#[test]
fn commit_message_file_trailing_blank_lines_removed() {
    assert_commit_message_normalizes("subject\nbody\n\n\n\n", "subject\nbody\n");
}

#[test]
fn commit_message_file_comment_lines_preserved() {
    assert_commit_message_normalizes(
        "subject\n# a comment\nbody\n",
        "subject\n# a comment\nbody\n",
    );
}

#[test]
fn commit_message_file_leading_indentation_preserved() {
    assert_commit_message_normalizes("subject\n    indented\n", "subject\n    indented\n");
}

#[test]
fn commit_inline_message_is_normalized_like_a_message_file() {
    let (dir, repo) = repo_with_staged_change();
    run_pgs(dir.path(), &["commit", "-m", "subject\r\n\n\nbody  "]).success();
    assert_eq!(head_message(&repo), "subject\n\nbody\n");
}

/// The `--plan` loader shares the message-file error path: a missing plan path is
/// a user typo (exit 2), not an internal IO failure (exit 4).
#[test]
fn plan_check_missing_plan_path_returns_exit_code_2() {
    let (dir, _repo) = setup_repo();
    let missing = dir.path().join("no-such-plan.json");
    let output = run_pgs(
        dir.path(),
        &["plan-check", "--plan", missing.to_str().unwrap()],
    )
    .code(2);
    assert_eq!(json_of(&output)["code"], "input_file_unreadable");
}

#[test]
fn plan_diff_missing_plan_path_returns_exit_code_2() {
    let (dir, _repo) = setup_repo();
    let missing = dir.path().join("no-such-plan.json");
    let output = run_pgs(
        dir.path(),
        &["plan-diff", "--plan", missing.to_str().unwrap()],
    )
    .code(2);
    assert_eq!(json_of(&output)["code"], "input_file_unreadable");
}
