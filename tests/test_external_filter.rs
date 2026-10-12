//! libgit2 cannot run external filter drivers (`filter.<name>.clean` or
//! `.process`, as git-lfs and git-crypt install). Staging such a path without
//! its driver would store the checkout bytes, so pgs refuses it.

mod common;

use std::path::Path;

use common::{commit_file, read_staged_blob, run_pgs, setup_repo, write_file};
use git2::Repository;
use serde_json::Value;

fn staged(dir: &Path) -> String {
    read_staged_blob(&Repository::open(dir).unwrap(), "f.txt")
}

fn filtered_fixture(configure_driver: bool) -> tempfile::TempDir {
    let (dir, repo) = setup_repo();
    commit_file(
        &repo,
        dir.path(),
        ".gitattributes",
        "*.txt filter=up\n",
        "attrs",
    );
    commit_file(&repo, dir.path(), "f.txt", "a\nb\n", "add f");
    if configure_driver {
        repo.config()
            .unwrap()
            .set_str("filter.up.clean", "tr a-z A-Z")
            .unwrap();
    }
    write_file(dir.path(), "f.txt", "a\nb\nc\n");
    drop(repo);
    dir
}

#[test]
fn stage_path_with_configured_external_filter_is_refused() {
    let dir = filtered_fixture(true);

    for selection in ["f.txt", "f.txt:3-3"] {
        let output = run_pgs(dir.path(), &["stage", selection]).code(2);
        let error: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();

        assert_eq!(error["code"], "external_filter", "{selection}");
        assert!(
            error["message"].as_str().unwrap().contains("up"),
            "{selection}: {error}"
        );
        assert_eq!(staged(dir.path()), "a\nb\n", "{selection}");
    }
}

#[test]
fn stage_path_whose_filter_has_no_driver_stages_normally() {
    let dir = filtered_fixture(false);

    run_pgs(dir.path(), &["stage", "f.txt"]).success();

    assert_eq!(staged(dir.path()), "a\nb\nc\n");
}
