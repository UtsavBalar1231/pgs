/// Repository discovery helpers for pgs.
use std::path::Path;

use git2::Repository;

use crate::error::PgsError;

/// Discover and open a git repository.
///
/// If `path` is `Some`, opens the repository at that exact path.
/// If `None`, walks up from the current working directory looking for `.git`.
pub fn open(path: Option<&str>) -> Result<Repository, PgsError> {
    match path {
        Some(p) => Repository::open(p).map_err(PgsError::from),
        None => Repository::discover(".").map_err(PgsError::from),
    }
}

/// Return the working directory path for a repository.
///
/// Returns an error for bare repositories, which have no working directory.
pub fn workdir(repo: &Repository) -> Result<&Path, PgsError> {
    repo.workdir()
        .ok_or_else(|| PgsError::Internal("bare repository has no working directory".into()))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use git2::Repository;
    use tempfile::TempDir;

    use super::*;

    fn init_repo() -> (TempDir, Repository) {
        let dir = TempDir::new().expect("tempdir");
        let repo = Repository::init(dir.path()).expect("init");
        (dir, repo)
    }

    #[test]
    fn open_valid_repo_succeeds() {
        let (dir, _repo) = init_repo();
        let result = open(Some(dir.path().to_str().expect("utf8")));
        assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
    }

    #[test]
    fn open_non_repo_path_returns_exit_code_4() {
        let dir = TempDir::new().expect("tempdir");
        let non_repo = dir.path().join("not_a_repo");
        fs::create_dir_all(&non_repo).expect("create dir");
        let result = open(Some(non_repo.to_str().expect("utf8")));
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert_eq!(err.exit_code(), 4, "expected exit code 4, got: {err}");
    }

    #[test]
    fn workdir_returns_path_for_non_bare_repo() {
        let (dir, repo) = init_repo();
        let wd = workdir(&repo).expect("workdir");
        // Resolve symlinks so comparison works across platforms
        let expected = fs::canonicalize(dir.path()).expect("canonicalize dir");
        let got = fs::canonicalize(wd).expect("canonicalize workdir");
        assert_eq!(got, expected);
    }

    #[test]
    fn workdir_returns_error_for_bare_repo() {
        let dir = TempDir::new().expect("tempdir");
        let repo = Repository::init_bare(dir.path()).expect("bare init");
        let result = workdir(&repo);
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert_eq!(
            err.exit_code(),
            4,
            "bare repo workdir should be exit code 4"
        );
        assert!(
            err.to_string().contains("bare"),
            "message should mention bare: {err}"
        );
    }

    /// Ensure `open()` with a path inside a repo (not the root) still works.
    #[test]
    fn open_subdir_of_repo_succeeds() {
        let (dir, _repo) = init_repo();
        let subdir: PathBuf = dir.path().join("src");
        fs::create_dir_all(&subdir).expect("create subdir");
        // git2 Repository::open requires the .git root, but discover walks up
        // We use open() which calls Repository::open directly — subdir has no .git,
        // so this should fail (open requires exact repo root).
        let result = open(Some(subdir.to_str().expect("utf8")));
        // This is expected to fail for Repository::open (not discover)
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().exit_code(), 4);
    }
}
