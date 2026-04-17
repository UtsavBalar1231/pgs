mod common;

use common::{commit_file, run_pgs, setup_repo, write_file};

/// `pgs overview` fuses scan (unstaged) and status (staged) envelopes.
#[test]
fn overview_merges_unstaged_and_staged_views() {
    let (dir, repo) = setup_repo();

    commit_file(
        &repo,
        dir.path(),
        "hello.txt",
        "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\n",
        "add hello",
    );

    write_file(
        dir.path(),
        "hello.txt",
        "alpha\nbeta\ngamma\ndelta\nepsilon\nline6\nline7\nline8\nline9\nline10\nline11\nline12\n",
    );

    run_pgs(dir.path(), &["stage", "hello.txt:1-5"]).success();

    write_file(
        dir.path(),
        "hello.txt",
        "alpha\nbeta\ngamma\ndelta\nepsilon\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\n",
    );

    let output = run_pgs(dir.path(), &["overview"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["version"], "v1");
    assert_eq!(json["command"], "overview");

    // unstaged section: should mirror the ScanOutput envelope.
    let unstaged = json
        .get("unstaged")
        .expect("overview envelope must include an `unstaged` section");
    assert_eq!(unstaged["version"], "v1");
    assert_eq!(unstaged["command"], "scan");
    let unstaged_files = unstaged["files"]
        .as_array()
        .expect("unstaged.files must be an array");
    assert_eq!(
        unstaged_files.len(),
        1,
        "expected one file with unstaged changes (tail additions)"
    );
    assert_eq!(unstaged_files[0]["path"], "hello.txt");

    // staged section: should mirror the StatusOutput envelope.
    let staged = json
        .get("staged")
        .expect("overview envelope must include a `staged` section");
    assert_eq!(staged["version"], "v1");
    assert_eq!(staged["command"], "status");
    let staged_files = staged["files"]
        .as_array()
        .expect("staged.files must be an array");
    assert_eq!(
        staged_files.len(),
        1,
        "expected one file with staged changes (lines 1-5)"
    );
    assert_eq!(staged_files[0]["path"], "hello.txt");
    assert_eq!(staged_files[0]["status"]["type"], "Modified");
}
