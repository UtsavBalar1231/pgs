#![allow(deprecated, dead_code)]

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use git2::Repository;
use tempfile::TempDir;

pub const MCP_PROTOCOL_VERSION_BASELINE: &str = pgs::mcp::PROTOCOL_VERSION_BASELINE;

/// Create a test repo with git identity and initial commit so HEAD exists.
pub fn setup_repo() -> (TempDir, Repository) {
    let dir = TempDir::new().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    {
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test").unwrap();
        config.set_str("user.email", "test@test.com").unwrap();
    }

    // Create initial empty commit so HEAD exists
    {
        let sig = repo.signature().unwrap();
        let tree_oid = {
            let mut index = repo.index().unwrap();
            index.write_tree().unwrap()
        };
        let tree = repo.find_tree(tree_oid).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
            .unwrap();
    }

    (dir, repo)
}

/// Write a file to the working directory, creating parent dirs as needed.
pub fn write_file(dir: &Path, rel_path: &str, content: &str) {
    let full = dir.join(rel_path);
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(full, content).unwrap();
}

/// Commit a file: write to working dir, add to index, create commit.
pub fn commit_file(repo: &Repository, dir: &Path, rel_path: &str, content: &str, message: &str) {
    write_file(dir, rel_path, content);
    let mut index = repo.index().unwrap();
    index.add_path(Path::new(rel_path)).unwrap();
    index.write().unwrap();
    let tree_oid = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_oid).unwrap();
    let sig = repo.signature().unwrap();
    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let parents: Vec<&git2::Commit<'_>> = parent.iter().collect();
    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
        .unwrap();
}

/// Build and run pgs with `--repo` pointed at the test repo.
pub fn run_pgs(dir: &Path, args: &[&str]) -> assert_cmd::assert::Assert {
    Command::cargo_bin("pgs")
        .unwrap()
        .arg("--json")
        .arg("--repo")
        .arg(dir.to_str().unwrap())
        .args(args)
        .assert()
}

pub fn run_pgs_raw(dir: &Path, args: &[&str]) -> assert_cmd::assert::Assert {
    Command::cargo_bin("pgs")
        .unwrap()
        .arg("--repo")
        .arg(dir.to_str().unwrap())
        .args(args)
        .assert()
}
