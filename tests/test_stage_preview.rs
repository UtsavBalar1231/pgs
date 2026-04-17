//! RED tests for `pgs stage --dry-run --explain` exact-content preview (A1).
//!
//! These assert the `OperationPreview` contract established in TODO 17 (A1-SCHEMA)
//! and will flip to GREEN once TODO 19 (A1-GREEN) wires up the preview logic in
//! `src/cmd/stage.rs`. Each test documents its expected RED failure mode.

mod common;

use std::fmt::Write as _;

use common::{commit_file, run_pgs, run_pgs_raw, setup_repo, write_file};

/// RED expected: `previews` absent — `stage --dry-run --explain` still returns
/// the count-only envelope until TODO 19 wires preview emission.
#[test]
fn dry_run_explain_shows_exact_line_content() {
    let (dir, repo) = setup_repo();
    commit_file(
        &repo,
        dir.path(),
        "src/main.rs",
        "fn main() {\n    println!(\"one\");\n    println!(\"two\");\n}\n",
        "seed",
    );
    write_file(
        dir.path(),
        "src/main.rs",
        "fn main() {\n    println!(\"one\");\n    println!(\"two\");\n    println!(\"three\");\n    println!(\"four\");\n    println!(\"five\");\n}\n",
    );

    let output = run_pgs(
        dir.path(),
        &["stage", "src/main.rs:4-6", "--dry-run", "--explain"],
    )
    .success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["status"], "dry_run");
    let previews = json["previews"].as_array().expect("previews must be array");
    assert_eq!(previews.len(), 1, "exactly one file previewed");
    let preview = &previews[0];
    assert_eq!(preview["file_path"], "src/main.rs");
    let lines = preview["preview_lines"]
        .as_array()
        .expect("preview_lines must be array");
    assert_eq!(lines.len(), 3, "three resolved lines");
    let contents: Vec<&str> = lines
        .iter()
        .map(|l| l["content"].as_str().unwrap())
        .collect();
    assert_eq!(
        contents,
        vec![
            "    println!(\"three\");",
            "    println!(\"four\");",
            "    println!(\"five\");",
        ]
    );
}

/// RED expected: `previews[0].preview_lines.len()` is 0 (field missing) or
/// equal to the full addition count (limit not applied).
#[test]
fn dry_run_explain_respects_limit_per_file() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "big.txt", "seed\n", "seed");

    let mut body = String::from("seed\n");
    for i in 0..250 {
        writeln!(body, "new line {i}").unwrap();
    }
    write_file(dir.path(), "big.txt", &body);

    let output = run_pgs(
        dir.path(),
        &[
            "stage",
            "big.txt:2-251",
            "--dry-run",
            "--explain",
            "--limit",
            "100",
        ],
    )
    .success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let preview = &json["previews"].as_array().unwrap()[0];
    assert_eq!(preview["preview_lines"].as_array().unwrap().len(), 100);
    assert_eq!(preview["truncated"], serde_json::Value::Bool(true));
    assert_eq!(preview["limit_applied"], 100);
}

/// RED expected: aggregation across files instead of per-file scoping.
#[test]
fn dry_run_explain_limit_is_per_file_not_aggregate() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "a.txt", "seed\n", "seed");
    commit_file(&repo, dir.path(), "b.txt", "seed\n", "seed");

    let mut body = String::from("seed\n");
    for i in 0..150 {
        writeln!(body, "line {i}").unwrap();
    }
    write_file(dir.path(), "a.txt", &body);
    write_file(dir.path(), "b.txt", &body);

    let output = run_pgs(
        dir.path(),
        &[
            "stage",
            "a.txt:2-151",
            "b.txt:2-151",
            "--dry-run",
            "--explain",
            "--limit",
            "200",
        ],
    )
    .success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let previews = json["previews"].as_array().unwrap();
    assert_eq!(previews.len(), 2, "two per-file entries");
    for p in previews {
        assert_eq!(p["preview_lines"].as_array().unwrap().len(), 150);
        assert_eq!(p["truncated"], serde_json::Value::Bool(false));
        assert_eq!(p["limit_applied"], 200);
    }
}

/// RED expected: `--limit 0` truncates to 0 or 200 instead of returning all.
#[test]
fn dry_run_explain_limit_zero_is_unlimited() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "all.txt", "seed\n", "seed");

    let mut body = String::from("seed\n");
    for i in 0..500 {
        writeln!(body, "l{i}").unwrap();
    }
    write_file(dir.path(), "all.txt", &body);

    let output = run_pgs(
        dir.path(),
        &[
            "stage",
            "all.txt:2-501",
            "--dry-run",
            "--explain",
            "--limit",
            "0",
        ],
    )
    .success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let preview = &json["previews"].as_array().unwrap()[0];
    assert_eq!(preview["preview_lines"].as_array().unwrap().len(), 500);
    assert_eq!(preview["truncated"], serde_json::Value::Bool(false));
    assert_eq!(preview["limit_applied"], 0);
}

/// GREEN on scaffold, RED pre-scaffold: the `ExplainWithoutDryRun` guard
/// added in TODO 17 surfaces the canonical error code and message.
#[test]
fn explain_without_dry_run_is_user_error() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "seed");
    write_file(dir.path(), "hello.txt", "line1\nline2\n");

    let output = run_pgs(dir.path(), &["stage", "hello.txt", "--explain"]).code(2);
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["code"], "explain_without_dry_run");
    let message = json["message"].as_str().unwrap();
    assert!(
        message.contains("--explain requires --dry-run"),
        "message was: {message}"
    );
    assert_eq!(json["exit_code"], 2);
}

/// RED expected: index ends up mutated once the dry-run path starts applying
/// changes incorrectly. Currently satisfied by the count-only dry-run path.
#[test]
fn dry_run_explain_preview_does_not_mutate_index() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "src/main.rs", "fn main() {}\n", "seed");
    write_file(
        dir.path(),
        "src/main.rs",
        "fn main() { println!(\"hello\"); }\n",
    );

    let pre_status = run_pgs(dir.path(), &["status"]).success();
    let pre_stdout = String::from_utf8(pre_status.get_output().stdout.clone()).unwrap();
    let pre_json: serde_json::Value = serde_json::from_str(&pre_stdout).unwrap();
    assert_eq!(pre_json["summary"]["total_files"], 0);

    run_pgs(
        dir.path(),
        &["stage", "src/main.rs", "--dry-run", "--explain"],
    )
    .success();

    let post_status = run_pgs(dir.path(), &["status"]).success();
    let post_stdout = String::from_utf8(post_status.get_output().stdout.clone()).unwrap();
    let post_json: serde_json::Value = serde_json::from_str(&post_stdout).unwrap();
    assert_eq!(
        post_json["summary"]["total_files"], pre_json["summary"]["total_files"],
        "dry-run --explain must not mutate the index"
    );
}

/// Text markers: A1-RENDER emits `stage.preview.begin`, one
/// `stage.preview.line` per row, then `stage.preview.end` between the
/// existing `stage.begin`/`stage.end` envelope.
#[test]
fn dry_run_explain_emits_stage_preview_text_markers() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "seed");
    write_file(dir.path(), "hello.txt", "line1\nline2\nline3\n");

    let output = run_pgs_raw(
        dir.path(),
        &["stage", "hello.txt:2-3", "--dry-run", "--explain"],
    )
    .success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    assert!(
        stdout.contains("@@pgs:v1 stage.preview.begin "),
        "expected stage.preview.begin marker: {stdout}"
    );
    assert!(
        stdout.contains("@@pgs:v1 stage.preview.line "),
        "expected at least one stage.preview.line marker: {stdout}"
    );
    assert!(
        stdout.contains("@@pgs:v1 stage.preview.end "),
        "expected stage.preview.end marker: {stdout}"
    );
}

/// RED expected: binary file path either errors out or leaks bytes into
/// `preview_lines`. TODO 19 must short-circuit with `reason: "binary"`.
#[test]
fn dry_run_explain_on_binary_file_returns_empty_preview_with_reason() {
    use git2::IndexAddOption;

    let (dir, repo) = setup_repo();
    let seed_bytes: Vec<u8> = (0u8..32u8).collect();
    std::fs::write(dir.path().join("image.bin"), &seed_bytes).unwrap();

    {
        let mut index = repo.index().unwrap();
        index
            .add_all(["image.bin"], IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        let sig = repo.signature().unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "seed bin", &tree, &[&head])
            .unwrap();
    }

    let modified_bytes: Vec<u8> = (0u8..64u8).map(|b| b ^ 0x5A_u8).collect();
    std::fs::write(dir.path().join("image.bin"), &modified_bytes).unwrap();

    let output = run_pgs(
        dir.path(),
        &["stage", "image.bin", "--dry-run", "--explain"],
    )
    .success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let preview = &json["previews"].as_array().unwrap()[0];
    assert_eq!(preview["file_path"], "image.bin");
    assert_eq!(preview["preview_lines"].as_array().unwrap().len(), 0);
    assert_eq!(preview["truncated"], serde_json::Value::Bool(false));
    assert_eq!(preview["reason"], "binary");
}
