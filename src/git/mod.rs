/// Git module: repository access, diff engine, staging, and unstaging.
pub mod diff;
pub mod repo;
pub mod staging;
pub mod unstaging;

use git2::Repository;

use crate::error::PgsError;

/// Classification of the workdir entry behind a path, for blob construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkdirFileType {
    /// Regular file — bytes are file contents.
    Regular,
    /// Symbolic link — bytes are the raw link target string.
    Symlink,
}

/// Bytes plus classification suitable for feeding `repo.blob()` + index entry construction.
#[derive(Debug, Clone)]
pub struct WorkdirBlob {
    /// Raw bytes: file contents for regular files, link-target string bytes for symlinks.
    pub bytes: Vec<u8>,
    /// Whether the workdir entry is a regular file or a symbolic link.
    pub file_type: WorkdirFileType,
}

/// Read a workdir path and return its bytes plus type classification.
///
/// Regular files return their content. Symlinks return the raw link-target
/// string bytes (Unix uses `OsStrExt::as_bytes` to preserve non-UTF8). The
/// helper never follows symlinks and never canonicalises paths — by design.
///
/// # Errors
/// - `PgsError::Io` for `symlink_metadata`, `read_link`, or `read` failures
///   (including missing paths).
/// - `PgsError::Internal` when the workdir entry is not a regular file or a
///   symlink (e.g., a directory, a socket, a fifo).
pub fn read_workdir_for_blob(
    workdir_root: &std::path::Path,
    rel_path: &str,
) -> Result<WorkdirBlob, PgsError> {
    let full = workdir_root.join(rel_path);
    let meta = std::fs::symlink_metadata(&full).map_err(|e| PgsError::io(&full, e))?;
    let ft = meta.file_type();
    if ft.is_symlink() {
        let target = std::fs::read_link(&full).map_err(|e| PgsError::io(&full, e))?;
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            return Ok(WorkdirBlob {
                bytes: target.as_os_str().as_bytes().to_vec(),
                file_type: WorkdirFileType::Symlink,
            });
        }
        #[cfg(not(unix))]
        {
            let s = target.to_str().ok_or_else(|| {
                PgsError::Internal(format!(
                    "non-UTF8 symlink target on non-Unix platform: {}",
                    full.display()
                ))
            })?;
            return Ok(WorkdirBlob {
                bytes: s.as_bytes().to_vec(),
                file_type: WorkdirFileType::Symlink,
            });
        }
    }
    if ft.is_file() {
        let bytes = std::fs::read(&full).map_err(|e| PgsError::io(&full, e))?;
        return Ok(WorkdirBlob {
            bytes,
            file_type: WorkdirFileType::Regular,
        });
    }
    Err(PgsError::Internal(format!(
        "unsupported workdir file type at {}: not a regular file or symlink",
        full.display()
    )))
}

/// Build an `IndexEntry` for a file, preserving existing mode/flags from the index.
///
/// If the file is not yet in the index (new file), defaults to mode `0o100644`.
/// Pass `mode_override` to apply a different mode (e.g. for `chmod +x` staging).
pub fn build_index_entry(
    index: &git2::Index,
    file_path: &str,
    oid: git2::Oid,
    content_len: u32,
    mode_override: Option<u32>,
) -> git2::IndexEntry {
    let (mode, flags, flags_extended) = match index.get_path(std::path::Path::new(file_path), 0) {
        Some(e) => (mode_override.unwrap_or(e.mode), e.flags, e.flags_extended),
        None => (mode_override.unwrap_or(0o100_644), 0, 0),
    };
    git2::IndexEntry {
        ctime: git2::IndexTime::new(0, 0),
        mtime: git2::IndexTime::new(0, 0),
        dev: 0,
        ino: 0,
        mode,
        uid: 0,
        gid: 0,
        file_size: content_len,
        id: oid,
        flags,
        flags_extended,
        path: file_path.as_bytes().to_vec(),
    }
}

/// Read the file mode from the HEAD tree entry.
///
/// Returns the mode (e.g. `0o100644` or `0o100755`) for the given path in HEAD.
pub fn read_head_mode(repo: &Repository, file_path: &str) -> Result<u32, PgsError> {
    let head = repo.head()?;
    let tree = head.peel_to_tree()?;
    let entry = tree.get_path(std::path::Path::new(file_path))?;
    Ok(u32::try_from(entry.filemode()).unwrap_or(0o100_644))
}

/// Read a blob from HEAD for the given file path.
///
/// Returns the raw byte content of the file as it exists in HEAD.
pub fn read_head_blob(repo: &Repository, file_path: &str) -> Result<Vec<u8>, PgsError> {
    let head = repo.head()?;
    let tree = head.peel_to_tree()?;
    let entry = tree.get_path(std::path::Path::new(file_path))?;
    let object = entry.to_object(repo)?;
    let blob = object.peel_to_blob()?;
    Ok(blob.content().to_vec())
}

/// Read a blob from the current index for the given file path.
///
/// Returns an error if the file is not present in the index.
pub fn read_index_blob(repo: &Repository, file_path: &str) -> Result<Vec<u8>, PgsError> {
    let index = repo.index()?;
    let entry = index
        .get_path(std::path::Path::new(file_path), 0)
        .ok_or_else(|| PgsError::FileNotInDiff {
            path: file_path.to_string(),
        })?;
    let blob = repo.find_blob(entry.id)?;
    Ok(blob.content().to_vec())
}

/// Check if content is binary by scanning for null bytes (matches git's heuristic).
///
/// Scans the first 8000 bytes only, mirroring the libgit2 binary detection strategy.
pub fn content_is_binary(content: &[u8]) -> bool {
    content.iter().take(8000).any(|&b| b == 0)
}

#[cfg(test)]
mod tests {
    use git2::Repository;
    use tempfile::TempDir;

    use super::*;

    fn setup_repo_with_commit(content: &str) -> (TempDir, Repository) {
        let dir = TempDir::new().expect("tempdir");
        let repo = Repository::init(dir.path()).expect("init");
        {
            let mut config = repo.config().expect("config");
            config.set_str("user.name", "Test").expect("set name");
            config
                .set_str("user.email", "test@test.com")
                .expect("set email");
        }
        // Create initial commit with a file
        let file_path = dir.path().join("file.txt");
        std::fs::write(&file_path, content).expect("write file");
        let mut index = repo.index().expect("index");
        index
            .add_path(std::path::Path::new("file.txt"))
            .expect("add");
        index.write().expect("write index");
        let tree_oid = index.write_tree().expect("write tree");
        {
            let tree = repo.find_tree(tree_oid).expect("find tree");
            let sig = repo.signature().expect("sig");
            repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
                .expect("commit");
        }
        (dir, repo)
    }

    #[test]
    fn content_is_binary_detects_null_bytes() {
        let binary = b"hello\x00world";
        assert!(content_is_binary(binary));
    }

    #[test]
    fn content_is_binary_text_returns_false() {
        let text = b"fn main() {\n    println!(\"hello\");\n}\n";
        assert!(!content_is_binary(text));
    }

    #[test]
    fn content_is_binary_only_scans_first_8000_bytes() {
        // Null byte at position 8000 (zero-indexed) is NOT scanned
        let mut data = vec![b'a'; 8000];
        data.push(0);
        assert!(!content_is_binary(&data));
        // Null byte at position 7999 IS scanned
        data[7999] = 0;
        assert!(content_is_binary(&data));
    }

    #[test]
    fn read_head_blob_returns_file_content() {
        let (_dir, repo) = setup_repo_with_commit("hello from HEAD\n");
        let content = read_head_blob(&repo, "file.txt").expect("read_head_blob");
        assert_eq!(content, b"hello from HEAD\n");
    }

    #[test]
    fn read_head_blob_missing_file_returns_error() {
        let (_dir, repo) = setup_repo_with_commit("content");
        let result = read_head_blob(&repo, "nonexistent.txt");
        assert!(result.is_err());
    }

    #[test]
    fn read_index_blob_returns_staged_content() {
        let (dir, repo) = setup_repo_with_commit("original\n");
        // Modify the file and stage it
        std::fs::write(dir.path().join("file.txt"), "modified\n").expect("write");
        let mut index = repo.index().expect("index");
        index
            .add_path(std::path::Path::new("file.txt"))
            .expect("add");
        index.write().expect("write");
        let content = read_index_blob(&repo, "file.txt").expect("read_index_blob");
        assert_eq!(content, b"modified\n");
    }

    #[test]
    fn read_index_blob_missing_file_returns_file_not_in_diff_error() {
        let (_dir, repo) = setup_repo_with_commit("content");
        let result = read_index_blob(&repo, "not_in_index.txt");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.exit_code(), 2, "FileNotInDiff should be exit code 2");
    }

    #[test]
    fn build_index_entry_new_file_uses_default_mode() {
        let (_dir, repo) = setup_repo_with_commit("content");
        let index = repo.index().expect("index");
        let oid = git2::Oid::zero();
        let entry = build_index_entry(&index, "new_file.rs", oid, 42, None);
        assert_eq!(entry.mode, 0o100_644);
        assert_eq!(entry.flags, 0);
        assert_eq!(entry.file_size, 42);
        assert_eq!(entry.path, b"new_file.rs");
    }

    #[test]
    fn build_index_entry_existing_file_preserves_mode() {
        let (_dir, repo) = setup_repo_with_commit("content");
        let index = repo.index().expect("index");
        let oid = git2::Oid::zero();
        let entry = build_index_entry(&index, "file.txt", oid, 10, None);
        assert_eq!(entry.mode, 0o100_644);
    }

    #[test]
    fn build_index_entry_mode_override_applies() {
        let (_dir, repo) = setup_repo_with_commit("content");
        let index = repo.index().expect("index");
        let oid = git2::Oid::zero();
        let entry = build_index_entry(&index, "file.txt", oid, 10, Some(0o100_755));
        assert_eq!(entry.mode, 0o100_755);
    }

    #[test]
    fn build_index_entry_mode_override_new_file() {
        let (_dir, repo) = setup_repo_with_commit("content");
        let index = repo.index().expect("index");
        let oid = git2::Oid::zero();
        let entry = build_index_entry(&index, "new_file.rs", oid, 42, Some(0o100_755));
        assert_eq!(entry.mode, 0o100_755);
    }

    #[test]
    fn read_head_mode_returns_file_mode() {
        let (_dir, repo) = setup_repo_with_commit("content");
        let mode = read_head_mode(&repo, "file.txt").expect("read_head_mode");
        assert_eq!(mode, 0o100_644);
    }

    #[test]
    fn read_head_mode_missing_file_returns_error() {
        let (_dir, repo) = setup_repo_with_commit("content");
        let result = read_head_mode(&repo, "nonexistent.txt");
        assert!(result.is_err());
    }

    #[test]
    fn read_workdir_for_blob_regular_file_returns_file_bytes() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let content = vec![b'x'; 100];
        std::fs::write(dir.path().join("regular.txt"), &content).expect("write");
        let blob = read_workdir_for_blob(dir.path(), "regular.txt").expect("read");
        assert_eq!(blob.bytes, content);
        assert_eq!(blob.file_type, WorkdirFileType::Regular);
    }

    #[cfg(unix)]
    #[test]
    fn read_workdir_for_blob_symlink_returns_target_string_bytes() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        std::fs::write(dir.path().join("target.txt"), b"content").expect("write");
        std::os::unix::fs::symlink("target.txt", dir.path().join("link")).expect("symlink");
        let blob = read_workdir_for_blob(dir.path(), "link").expect("read");
        assert_eq!(blob.bytes, b"target.txt");
        assert_eq!(blob.file_type, WorkdirFileType::Symlink);
    }

    #[cfg(unix)]
    #[test]
    fn read_workdir_for_blob_dangling_symlink_returns_target_string_bytes() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        std::os::unix::fs::symlink("does-not-exist", dir.path().join("dangling")).expect("symlink");
        let blob = read_workdir_for_blob(dir.path(), "dangling").expect("read");
        assert_eq!(blob.bytes, b"does-not-exist");
        assert_eq!(blob.file_type, WorkdirFileType::Symlink);
    }

    #[test]
    fn read_workdir_for_blob_directory_returns_internal_error() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        std::fs::create_dir(dir.path().join("subdir")).expect("mkdir");
        let result = read_workdir_for_blob(dir.path(), "subdir");
        let err = result.expect_err("expected error for directory");
        assert!(
            matches!(err, crate::error::PgsError::Internal(_)),
            "expected Internal, got: {err}"
        );
    }

    #[test]
    fn read_workdir_for_blob_missing_path_returns_io_error() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let result = read_workdir_for_blob(dir.path(), "no-such-file.txt");
        let err = result.expect_err("expected error for missing path");
        assert!(
            matches!(err, crate::error::PgsError::Io { .. }),
            "expected Io, got: {err}"
        );
    }
}
