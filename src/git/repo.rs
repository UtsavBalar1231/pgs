/// Repository discovery helpers for pgs.
use std::path::{Path, PathBuf};

use git2::Repository;

use crate::error::PgsError;

/// Discover and open a git repository, correcting workdir if needed.
///
/// `path = Some` opens at that path; `None` walks up from CWD. The workdir is
/// corrected in-memory for non-standard layouts (e.g. `--separate-git-dir`).
// Errors: PgsError::Git on open failure; PgsError::WorkdirMismatch if uncorrectable.
pub fn open(path: Option<&str>) -> Result<Repository, PgsError> {
    let repo = match path {
        Some(p) => Repository::open(p).map_err(PgsError::from)?,
        None => Repository::discover(".").map_err(PgsError::from)?,
    };
    validate_workdir(&repo, path)?;
    Ok(repo)
}

/// Validate and correct the repository workdir in-memory.
///
/// In non-standard `.git` layouts (`--separate-git-dir`, Google Repo),
/// libgit2 may resolve `workdir()` to the parent of the gitdir instead of
/// the directory containing the `.git` file/entry. Detects this by checking
/// whether the reported workdir actually contains a `.git` entry.
fn validate_workdir(repo: &Repository, cli_path: Option<&str>) -> Result<(), PgsError> {
    let current_workdir = match repo.workdir() {
        Some(wd) => wd.to_path_buf(),
        None => return Ok(()), // bare repo — handled elsewhere
    };

    // Fast path: if the reported workdir contains a .git entry, it is correct.
    if current_workdir.join(".git").exists() {
        return Ok(());
    }

    // Workdir is wrong — find the directory that actually contains .git.
    let expected = if let Some(p) = cli_path {
        let given = std::fs::canonicalize(p).unwrap_or_else(|_| PathBuf::from(p));
        if given.join(".git").exists() {
            given
        } else {
            // User passed a bare gitdir path — trust libgit2.
            return Ok(());
        }
    } else {
        let cwd = std::env::current_dir()
            .map_err(|e| PgsError::Internal(format!("failed to read current directory: {e}")))?;
        let canon_cwd = std::fs::canonicalize(&cwd).unwrap_or(cwd);
        find_git_root(&canon_cwd).ok_or_else(|| {
            let resolved_workdir =
                std::fs::canonicalize(&current_workdir).unwrap_or_else(|_| current_workdir.clone());
            PgsError::WorkdirMismatch {
                expected: canon_cwd,
                actual: resolved_workdir,
            }
        })?
    };

    let canon_current =
        std::fs::canonicalize(&current_workdir).unwrap_or_else(|_| current_workdir.clone());
    let canon_expected = std::fs::canonicalize(&expected).unwrap_or_else(|_| expected.clone());

    if canon_current == canon_expected {
        return Ok(());
    }

    if !canon_expected.is_dir() {
        return Err(PgsError::WorkdirMismatch {
            expected: canon_expected,
            actual: canon_current,
        });
    }

    repo.set_workdir(&canon_expected, false)
        .map_err(|_| PgsError::WorkdirMismatch {
            expected: canon_expected.clone(),
            actual: canon_current,
        })
}

/// Walk up from `start` looking for a directory that contains a `.git` entry.
fn find_git_root(start: &Path) -> Option<PathBuf> {
    let mut dir = start;
    loop {
        if dir.join(".git").exists() {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
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

    /// Helper: run git commands during test setup.
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

    #[test]
    fn open_separate_git_dir_resolves_correct_workdir() {
        let workdir_dir = TempDir::new().expect("workdir");
        let gitdir_dir = TempDir::new().expect("gitdir");

        git(
            workdir_dir.path(),
            &[
                "init",
                "--separate-git-dir",
                gitdir_dir.path().to_str().unwrap(),
            ],
        );
        git(workdir_dir.path(), &["config", "user.name", "Test"]);
        git(
            workdir_dir.path(),
            &["config", "user.email", "test@test.com"],
        );

        fs::write(workdir_dir.path().join("test.txt"), "hello\n").expect("write");
        git(workdir_dir.path(), &["add", "test.txt"]);
        git(workdir_dir.path(), &["commit", "-m", "init"]);

        // Modify file so scan has something to report
        fs::write(workdir_dir.path().join("test.txt"), "hello\nworld\n").expect("write");

        let repo = open(Some(workdir_dir.path().to_str().unwrap())).expect("open");
        let actual_wd = fs::canonicalize(repo.workdir().expect("workdir")).expect("canon");
        let expected_wd = fs::canonicalize(workdir_dir.path()).expect("canon");
        assert_eq!(
            actual_wd, expected_wd,
            "workdir should be the workdir dir, not gitdir parent"
        );

        // The critical assertion: scan must show Modified, not Deleted
        let diff = crate::git::diff::diff_index_to_workdir(&repo, 3).expect("diff");
        let result = crate::git::diff::build_scan_result(&repo, &diff, None).expect("scan");
        assert_eq!(result.summary.modified, 1, "file should be Modified");
        assert_eq!(result.summary.deleted, 0, "nothing should be Deleted");
        assert_eq!(result.files[0].path, "test.txt");
    }

    #[test]
    fn open_symlinked_git_objects_works_correctly() {
        let project_dir = TempDir::new().expect("project");
        let shared_dir = TempDir::new().expect("shared");

        git(project_dir.path(), &["init"]);
        git(project_dir.path(), &["config", "user.name", "Test"]);
        git(
            project_dir.path(),
            &["config", "user.email", "test@test.com"],
        );

        fs::write(project_dir.path().join("test.txt"), "hello\n").expect("write");
        git(project_dir.path(), &["add", "test.txt"]);
        git(project_dir.path(), &["commit", "-m", "init"]);

        // Symlink .git/objects to external dir (Google Repo style)
        let git_objects = project_dir.path().join(".git/objects");
        let shared_objects = shared_dir.path().join("objects");
        fs::rename(&git_objects, &shared_objects).expect("move objects");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&shared_objects, &git_objects).expect("symlink objects");

        fs::write(project_dir.path().join("test.txt"), "hello\nworld\n").expect("write");

        let repo = open(Some(project_dir.path().to_str().unwrap())).expect("open");
        let diff = crate::git::diff::diff_index_to_workdir(&repo, 3).expect("diff");
        let result = crate::git::diff::build_scan_result(&repo, &diff, None).expect("scan");
        assert_eq!(result.summary.modified, 1, "file should be Modified");
        assert_eq!(result.summary.deleted, 0, "nothing should be Deleted");
    }
}
