/// Git module: repository access, diff engine, staging, and unstaging.
pub mod diff;
pub mod repo;
pub mod staging;
pub mod unstaging;

use git2::Repository;

use crate::error::PgsError;

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
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn read_head_mode(repo: &Repository, file_path: &str) -> Result<u32, PgsError> {
    let head = repo.head()?;
    let tree = head.peel_to_tree()?;
    let entry = tree.get_path(std::path::Path::new(file_path))?;
    Ok(entry.filemode() as u32)
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
}
