mod common;

use std::fs;

use tempfile::TempDir;

fn git(dir: &std::path::Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git command");
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
}

fn setup_separate_git_dir() -> (TempDir, TempDir) {
    let workdir = TempDir::new().unwrap();
    let gitdir = TempDir::new().unwrap();

    git(
        workdir.path(),
        &[
            "init",
            "--separate-git-dir",
            gitdir.path().to_str().unwrap(),
        ],
    );
    git(workdir.path(), &["config", "user.name", "Test"]);
    git(workdir.path(), &["config", "user.email", "test@test.com"]);

    fs::write(workdir.path().join("test.txt"), "hello\n").unwrap();
    git(workdir.path(), &["add", "test.txt"]);
    git(workdir.path(), &["commit", "-m", "init"]);

    fs::write(workdir.path().join("test.txt"), "hello\nworld\n").unwrap();

    (workdir, gitdir)
}

#[test]
fn scan_separate_git_dir_shows_modified_not_deleted() {
    let (workdir, _gitdir) = setup_separate_git_dir();

    let assert = common::run_pgs(workdir.path(), &["scan"]);
    let output = assert.success().get_output().stdout.clone();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();

    let files = json["files"].as_array().unwrap();
    assert_eq!(files.len(), 1, "should have exactly 1 file");
    assert_eq!(files[0]["path"], "test.txt");
    assert_eq!(files[0]["status"]["type"], "Modified");

    let summary = &json["summary"];
    assert_eq!(summary["modified"], 1);
    assert_eq!(summary["deleted"], 0);
}

#[test]
fn stage_with_separate_git_dir_succeeds() {
    let (workdir, _gitdir) = setup_separate_git_dir();

    common::run_pgs(workdir.path(), &["stage", "test.txt"]).success();

    let assert = common::run_pgs(workdir.path(), &["status"]);
    let output = assert.success().get_output().stdout.clone();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();

    let files = json["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["path"], "test.txt");
    assert_eq!(files[0]["status"]["type"], "Modified");
}

#[test]
fn scan_separate_git_dir_with_relative_gitdir_path() {
    // Create a parent dir containing both workdir/ and real-gitdir/ as siblings
    let parent = TempDir::new().unwrap();
    let workdir_path = parent.path().join("workdir");
    let gitdir_path = parent.path().join("real-gitdir");
    fs::create_dir_all(&workdir_path).unwrap();

    git(
        &workdir_path,
        &["init", "--separate-git-dir", gitdir_path.to_str().unwrap()],
    );
    git(&workdir_path, &["config", "user.name", "Test"]);
    git(&workdir_path, &["config", "user.email", "test@test.com"]);

    // Rewrite .git file to use a relative path
    let dot_git = workdir_path.join(".git");
    fs::write(&dot_git, "gitdir: ../real-gitdir\n").unwrap();

    fs::write(workdir_path.join("file.rs"), "fn main() {}\n").unwrap();
    git(&workdir_path, &["add", "file.rs"]);
    git(&workdir_path, &["commit", "-m", "init"]);
    fs::write(workdir_path.join("file.rs"), "fn main() { todo!() }\n").unwrap();

    let assert = common::run_pgs(&workdir_path, &["scan"]);
    let output = assert.success().get_output().stdout.clone();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();

    assert_eq!(json["summary"]["modified"], 1);
    assert_eq!(json["summary"]["deleted"], 0);
}
