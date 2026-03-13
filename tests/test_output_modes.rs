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
fn output_mode_defaults_to_text() {
    let (dir, _repo) = setup_repo();

    let output = run_agstage_raw(dir.path(), &["scan"]).code(1);
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();

    assert_eq!(
        lines.len(),
        1,
        "runtime error should be a single marker: {stdout}"
    );
    let (kind, payload) = parse_marker(lines[0]);
    assert_eq!(kind, "error");
    assert_eq!(payload["version"], "v1");
    assert_eq!(payload["command"], "scan");
    assert_eq!(payload["phase"], "runtime");
    assert_eq!(payload["code"], "no_changes");
    assert_eq!(payload["exit_code"], 1);
    assert_eq!(payload["message"], "no changes detected in working tree");
    assert!(
        serde_json::from_str::<serde_json::Value>(&stdout).is_err(),
        "default output should not be JSON: {stdout}"
    );
}

#[test]
fn output_mode_json_alias_selects_json() {
    let (dir, _repo) = setup_repo();

    let alias_output = run_agstage_raw(dir.path(), &["--json", "scan"]).code(1);
    let alias_stdout = String::from_utf8(alias_output.get_output().stdout.clone()).unwrap();
    let alias_json: serde_json::Value = serde_json::from_str(&alias_stdout).unwrap();
    assert_eq!(alias_json["version"], "v1");
    assert_eq!(alias_json["command"], "scan");
    assert_eq!(alias_json["phase"], "runtime");
    assert_eq!(alias_json["code"], "no_changes");
    assert_eq!(alias_json["message"], "no changes detected in working tree");
    assert_eq!(alias_json["exit_code"], 1);

    let redundant_output =
        run_agstage_raw(dir.path(), &["--json", "--output", "json", "scan"]).code(1);
    let redundant_stdout = String::from_utf8(redundant_output.get_output().stdout.clone()).unwrap();
    let redundant_json: serde_json::Value = serde_json::from_str(&redundant_stdout).unwrap();
    assert_eq!(redundant_json["version"], "v1");
    assert_eq!(redundant_json["command"], "scan");
    assert_eq!(redundant_json["phase"], "runtime");
    assert_eq!(redundant_json["code"], "no_changes");
    assert_eq!(
        redundant_json["message"],
        serde_json::Value::String("no changes detected in working tree".into())
    );
    assert_eq!(redundant_json["exit_code"], 1);
}

#[test]
fn output_mode_conflicting_flags_return_user_error() {
    let (dir, _repo) = setup_repo();

    let output = run_agstage_raw(dir.path(), &["--json", "--output", "text", "scan"]).code(2);
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();
    let (kind, payload) = parse_marker(lines[0]);

    assert_eq!(kind, "error");
    assert_eq!(payload["version"], "v1");
    assert_eq!(payload["command"], "cli");
    assert_eq!(payload["phase"], "parse");
    assert_eq!(payload["code"], "argument_conflict");
    assert_eq!(payload["exit_code"], 2);
    let message = payload["message"].as_str().unwrap();
    assert!(message.contains("--json conflicts with --output text"));
    assert!(
        serde_json::from_str::<serde_json::Value>(&stdout).is_err(),
        "conflicting flags should not resolve to JSON output: {stdout}"
    );
}

#[test]
fn json_parse_error_uses_cli_error_contract() {
    let (dir, _repo) = setup_repo();

    let output = run_agstage_raw(dir.path(), &["--output", "json", "--definitely-invalid"]).code(2);
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["version"], "v1");
    assert_eq!(json["command"], "cli");
    assert_eq!(json["phase"], "parse");
    assert_eq!(json["code"], "unknown_argument");
    assert_eq!(json["exit_code"], 2);
    let message = json["message"].as_str().unwrap();
    assert!(
        message.contains("--definitely-invalid"),
        "message was: {message}"
    );
}

#[test]
fn text_runtime_error_has_stable_error_marker() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "add hello");
    write_file(dir.path(), "hello.txt", "line1\nline2\n");

    let output = run_agstage_raw(dir.path(), &["stage", "deadbeef0000"]).code(2);
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();

    assert_eq!(
        lines.len(),
        1,
        "runtime error should be single marker: {stdout}"
    );
    let (kind, payload) = parse_marker(lines[0]);
    assert_eq!(kind, "error");
    assert_eq!(payload["version"], "v1");
    assert_eq!(payload["command"], "stage");
    assert_eq!(payload["phase"], "runtime");
    assert_eq!(payload["code"], "unknown_hunk_id");
    assert_eq!(payload["exit_code"], 2);
    let message = payload["message"].as_str().unwrap();
    assert!(
        message.contains("unknown hunk ID"),
        "message should describe the runtime error: {message}"
    );
}

#[test]
fn text_default_status_uses_v1_markers() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "add hello");
    write_file(dir.path(), "hello.txt", "line1\nline2\n");
    run_agstage(dir.path(), &["stage", "hello.txt"]).success();

    let output = run_agstage_raw(dir.path(), &["status"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();

    assert_eq!(
        lines.len(),
        4,
        "status text output should be marker-only: {stdout}"
    );
    assert!(lines.iter().all(|line| line.starts_with("@@agstage:v1 ")));

    let (begin_kind, begin_payload) = parse_marker(lines[0]);
    assert_eq!(begin_kind, "status.begin");
    assert_eq!(begin_payload["command"], "status");
    assert_eq!(begin_payload["items"], 1);

    let (file_kind, file_payload) = parse_marker(lines[1]);
    assert_eq!(file_kind, "status.file");
    assert_eq!(file_payload["path"], "hello.txt");
    assert_eq!(file_payload["status"]["type"], "Modified");
    assert!(file_payload["lines_added"].as_u64().unwrap() > 0);

    let (summary_kind, summary_payload) = parse_marker(lines[2]);
    assert_eq!(summary_kind, "summary");
    assert_eq!(summary_payload["command"], "status");
    assert_eq!(summary_payload["total_files"], 1);
    assert!(summary_payload["total_additions"].as_u64().unwrap() > 0);

    let (end_kind, end_payload) = parse_marker(lines[3]);
    assert_eq!(end_kind, "status.end");
    assert_eq!(end_payload["command"], "status");
    assert_eq!(end_payload["items"], 1);

    assert!(
        serde_json::from_str::<serde_json::Value>(&stdout).is_err(),
        "marker output must not be a raw JSON document: {stdout}"
    );
}

#[test]
fn json_mode_status_uses_new_contract() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "add hello");
    write_file(dir.path(), "hello.txt", "line1\nline2\n");
    run_agstage(dir.path(), &["stage", "hello.txt"]).success();

    let output = run_agstage(dir.path(), &["status"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["version"], "v1");
    assert_eq!(json["command"], "status");
    assert!(json.get("staged_files").is_none());

    let files = json["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["path"], "hello.txt");
    assert_eq!(files[0]["status"]["type"], "Modified");
    assert!(files[0]["lines_added"].as_u64().unwrap() > 0);

    assert_eq!(json["summary"]["total_files"], 1);
    assert!(json["summary"]["total_additions"].as_u64().unwrap() > 0);
}

#[test]
fn text_default_commit_uses_single_result_marker() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "add hello");
    write_file(dir.path(), "hello.txt", "line1\nline2\n");
    run_agstage(dir.path(), &["stage", "hello.txt"]).success();

    let output = run_agstage_raw(dir.path(), &["commit", "-m", "feat: add line2"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();

    assert_eq!(
        lines.len(),
        1,
        "commit text output should be one marker: {stdout}"
    );

    let (kind, payload) = parse_marker(lines[0]);
    assert_eq!(kind, "commit.result");
    assert_eq!(payload["version"], "v1");
    assert_eq!(payload["command"], "commit");
    assert_eq!(payload["message"], "feat: add line2");
    assert!(payload["author"].as_str().unwrap().contains("Test"));
    assert_eq!(payload["files_changed"], 1);
    assert_eq!(payload["insertions"], 1);
    assert_eq!(payload["deletions"], 0);

    let hash = payload["commit_hash"].as_str().unwrap();
    assert_eq!(hash.len(), 40, "commit hash should be 40 hex characters");

    assert!(
        serde_json::from_str::<serde_json::Value>(&stdout).is_err(),
        "marker output must not be a raw JSON document: {stdout}"
    );
}

#[test]
fn json_mode_commit_uses_new_contract() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "add hello");
    write_file(dir.path(), "hello.txt", "line1\nline2\n");
    run_agstage(dir.path(), &["stage", "hello.txt"]).success();

    let output = run_agstage(dir.path(), &["commit", "-m", "feat: add line2"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["version"], "v1");
    assert_eq!(json["command"], "commit");
    assert_eq!(json["message"], "feat: add line2");
    assert!(json["author"].as_str().unwrap().contains("Test"));
    assert_eq!(json["files_changed"], 1);
    assert_eq!(json["insertions"], 1);
    assert_eq!(json["deletions"], 0);

    let hash = json["commit_hash"].as_str().unwrap();
    assert_eq!(hash.len(), 40, "commit hash should be 40 hex characters");
}

#[test]
fn text_default_stage_uses_operation_markers() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "add hello");
    write_file(dir.path(), "hello.txt", "line1\nline2\n");

    let output = run_agstage_raw(dir.path(), &["stage", "hello.txt"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();

    assert_eq!(
        lines.len(),
        3,
        "stage text output should be marker-only: {stdout}"
    );
    assert!(lines.iter().all(|line| line.starts_with("@@agstage:v1 ")));

    let (begin_kind, begin_payload) = parse_marker(lines[0]);
    assert_eq!(begin_kind, "stage.begin");
    assert_eq!(begin_payload["command"], "stage");
    assert_eq!(begin_payload["status"], "ok");
    assert_eq!(begin_payload["items"], 1);
    assert!(begin_payload["backup_id"].is_string());

    let (item_kind, item_payload) = parse_marker(lines[1]);
    assert_eq!(item_kind, "item");
    assert_eq!(item_payload["selection"], "hello.txt");
    assert!(item_payload["lines_affected"].as_u64().unwrap() > 0);
    assert!(item_payload.get("lines_staged").is_none());

    let (end_kind, end_payload) = parse_marker(lines[2]);
    assert_eq!(end_kind, "stage.end");
    assert_eq!(end_payload["command"], "stage");
    assert_eq!(end_payload["status"], "ok");
    assert_eq!(end_payload["items"], 1);
    assert_eq!(end_payload["backup_id"], begin_payload["backup_id"]);

    assert!(
        serde_json::from_str::<serde_json::Value>(&stdout).is_err(),
        "marker output must not be a raw JSON document: {stdout}"
    );
}

#[test]
fn json_mode_stage_uses_new_contract() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "add hello");
    write_file(dir.path(), "hello.txt", "line1\nline2\n");

    let output = run_agstage(dir.path(), &["stage", "hello.txt"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["version"], "v1");
    assert_eq!(json["command"], "stage");
    assert_eq!(json["status"], "ok");
    assert!(json.get("succeeded").is_none());
    assert!(json.get("failed").is_none());

    let items = json["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["selection"], "hello.txt");
    assert!(items[0]["lines_affected"].as_u64().unwrap() > 0);
    assert!(items[0].get("lines_staged").is_none());

    assert_eq!(json["warnings"], serde_json::Value::Array(vec![]));
    assert!(json["backup_id"].is_string());
}

#[test]
fn text_default_unstage_uses_operation_markers() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "add hello");
    write_file(dir.path(), "hello.txt", "line1\nline2\n");
    run_agstage(dir.path(), &["stage", "hello.txt"]).success();

    let output = run_agstage_raw(dir.path(), &["unstage", "hello.txt"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();

    assert_eq!(
        lines.len(),
        3,
        "unstage text output should be marker-only: {stdout}"
    );
    assert!(lines.iter().all(|line| line.starts_with("@@agstage:v1 ")));

    let (begin_kind, begin_payload) = parse_marker(lines[0]);
    assert_eq!(begin_kind, "unstage.begin");
    assert_eq!(begin_payload["command"], "unstage");
    assert_eq!(begin_payload["status"], "ok");
    assert_eq!(begin_payload["items"], 1);
    assert!(begin_payload["backup_id"].is_string());

    let (item_kind, item_payload) = parse_marker(lines[1]);
    assert_eq!(item_kind, "item");
    assert_eq!(item_payload["selection"], "hello.txt");
    assert!(item_payload["lines_affected"].as_u64().unwrap() > 0);
    assert!(item_payload.get("lines_staged").is_none());

    let (end_kind, end_payload) = parse_marker(lines[2]);
    assert_eq!(end_kind, "unstage.end");
    assert_eq!(end_payload["command"], "unstage");
    assert_eq!(end_payload["status"], "ok");
    assert_eq!(end_payload["items"], 1);
    assert_eq!(end_payload["backup_id"], begin_payload["backup_id"]);

    assert!(
        serde_json::from_str::<serde_json::Value>(&stdout).is_err(),
        "marker output must not be a raw JSON document: {stdout}"
    );
}

#[test]
fn json_mode_unstage_uses_new_contract() {
    let (dir, repo) = setup_repo();
    commit_file(&repo, dir.path(), "hello.txt", "line1\n", "add hello");
    write_file(dir.path(), "hello.txt", "line1\nline2\n");
    run_agstage(dir.path(), &["stage", "hello.txt"]).success();

    let output = run_agstage(dir.path(), &["unstage", "--dry-run", "hello.txt"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["version"], "v1");
    assert_eq!(json["command"], "unstage");
    assert_eq!(json["status"], "dry_run");
    assert!(json.get("succeeded").is_none());
    assert!(json.get("failed").is_none());

    let items = json["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["selection"], "hello.txt");
    assert!(items[0]["lines_affected"].as_u64().unwrap() > 0);
    assert!(items[0].get("lines_staged").is_none());

    assert_eq!(json["warnings"], serde_json::Value::Array(vec![]));
    assert_eq!(json["backup_id"], serde_json::Value::Null);
}

#[test]
fn text_default_scan_has_v1_markers() {
    let (dir, repo) = setup_repo();
    commit_file(
        &repo,
        dir.path(),
        "hello.txt",
        "line1\nline2\n",
        "add hello",
    );
    write_file(dir.path(), "hello.txt", "line1\nline2\nline3\n");

    let output = run_agstage_raw(dir.path(), &["scan"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();

    assert_eq!(
        lines.len(),
        5,
        "compact scan should be marker-only: {stdout}"
    );
    assert!(lines.iter().all(|line| line.starts_with("@@agstage:v1 ")));

    let (begin_kind, begin_payload) = parse_marker(lines[0]);
    assert_eq!(begin_kind, "scan.begin");
    assert_eq!(begin_payload["command"], "scan");
    assert_eq!(begin_payload["detail"], "compact");
    assert_eq!(begin_payload["items"], 1);

    let (file_kind, file_payload) = parse_marker(lines[1]);
    assert_eq!(file_kind, "file");
    assert_eq!(file_payload["path"], "hello.txt");
    assert_eq!(file_payload["binary"], false);
    assert_eq!(file_payload["hunks_count"], 1);
    assert!(file_payload["lines_added"].as_u64().unwrap() > 0);
    assert!(file_payload.get("checksum").is_none());

    let (hunk_kind, hunk_payload) = parse_marker(lines[2]);
    assert_eq!(hunk_kind, "hunk");
    assert_eq!(hunk_payload["path"], "hello.txt");
    assert!(hunk_payload["id"].is_string());
    assert!(hunk_payload.get("checksum").is_none());

    let (summary_kind, summary_payload) = parse_marker(lines[3]);
    assert_eq!(summary_kind, "summary");
    assert_eq!(summary_payload["command"], "scan");
    assert_eq!(summary_payload["detail"], "compact");
    assert_eq!(summary_payload["total_files"], 1);
    assert_eq!(summary_payload["total_hunks"], 1);
    assert_eq!(summary_payload["modified"], 1);

    let (end_kind, end_payload) = parse_marker(lines[4]);
    assert_eq!(end_kind, "scan.end");
    assert_eq!(end_payload["command"], "scan");
    assert_eq!(end_payload["detail"], "compact");
    assert_eq!(end_payload["items"], 1);
}

#[test]
fn text_full_scan_frames_diff_body_with_markers() {
    let (dir, repo) = setup_repo();
    commit_file(
        &repo,
        dir.path(),
        "hello.txt",
        "line1\nline2\n",
        "add hello",
    );
    write_file(dir.path(), "hello.txt", "line1\nline2\nline3\n");

    let output = run_agstage_raw(dir.path(), &["scan", "--full"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();
    let hunk_end_index = lines
        .iter()
        .position(|line| line.starts_with("@@agstage:v1 hunk.end "))
        .unwrap();
    let file_end_index = lines
        .iter()
        .position(|line| line.starts_with("@@agstage:v1 file.end "))
        .unwrap();

    let (begin_kind, begin_payload) = parse_marker(lines[0]);
    assert_eq!(begin_kind, "scan.begin");
    assert_eq!(begin_payload["detail"], "full");

    let (file_begin_kind, file_begin_payload) = parse_marker(lines[1]);
    assert_eq!(file_begin_kind, "file.begin");
    assert_eq!(file_begin_payload["path"], "hello.txt");
    assert_eq!(file_begin_payload["binary"], false);
    assert!(file_begin_payload["checksum"].is_string());

    let (hunk_begin_kind, hunk_begin_payload) = parse_marker(lines[2]);
    assert_eq!(hunk_begin_kind, "hunk.begin");
    assert_eq!(hunk_begin_payload["path"], "hello.txt");
    assert!(hunk_begin_payload["id"].is_string());
    assert!(hunk_begin_payload["checksum"].is_string());

    assert!(hunk_end_index > 3, "expected raw diff body lines: {stdout}");
    for raw_line in &lines[3..hunk_end_index] {
        assert!(!raw_line.starts_with("@@agstage:v1 "));
        assert!(
            raw_line.starts_with(' ') || raw_line.starts_with('+') || raw_line.starts_with('-'),
            "unexpected raw diff line: {raw_line}"
        );
    }
    assert!(
        lines[3..hunk_end_index]
            .iter()
            .any(|line| *line == "+line3")
    );

    let (hunk_end_kind, hunk_end_payload) = parse_marker(lines[hunk_end_index]);
    assert_eq!(hunk_end_kind, "hunk.end");
    assert_eq!(hunk_end_payload["path"], "hello.txt");
    assert_eq!(hunk_end_payload["id"], hunk_begin_payload["id"]);
    assert_eq!(file_end_index, hunk_end_index + 1);

    let (file_end_kind, file_end_payload) = parse_marker(lines[file_end_index]);
    assert_eq!(file_end_kind, "file.end");
    assert_eq!(file_end_payload["path"], "hello.txt");

    let (summary_kind, summary_payload) = parse_marker(lines[file_end_index + 1]);
    assert_eq!(summary_kind, "summary");
    assert_eq!(summary_payload["detail"], "full");
    assert_eq!(summary_payload["total_files"], 1);

    let (end_kind, end_payload) = parse_marker(lines[file_end_index + 2]);
    assert_eq!(end_kind, "scan.end");
    assert_eq!(end_payload["detail"], "full");
    assert_eq!(end_payload["items"], 1);

    let non_marker_indices: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| (!line.starts_with("@@agstage:v1 ")).then_some(index))
        .collect();
    assert!(
        non_marker_indices
            .iter()
            .all(|index| *index > 2 && *index < hunk_end_index),
        "raw diff lines must stay inside hunk frames: {stdout}"
    );
}

#[test]
fn json_mode_scan_uses_new_contract() {
    let (dir, repo) = setup_repo();
    commit_file(
        &repo,
        dir.path(),
        "hello.txt",
        "line1\nline2\n",
        "add hello",
    );
    write_file(dir.path(), "hello.txt", "line1\nline2\nline3\n");

    let output = run_agstage(dir.path(), &["scan"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["version"], "v1");
    assert_eq!(json["command"], "scan");
    assert_eq!(json["detail"], "compact");

    let file = &json["files"][0];
    assert_eq!(file["path"], "hello.txt");
    assert_eq!(file["binary"], false);
    assert!(file.get("is_binary").is_none());
    assert!(file.get("checksum").is_none());

    let hunk = &file["hunks"][0];
    assert!(hunk["id"].is_string());
    assert!(hunk.get("hunk_id").is_none());
    assert!(hunk.get("lines").is_none());
    assert!(hunk.get("checksum").is_none());

    assert_eq!(json["summary"]["total_files"], 1);
    assert_eq!(json["summary"]["total_hunks"], 1);
    assert_eq!(json["summary"]["modified"], 1);
}
