/// Git module: repository access, diff engine, staging, and unstaging.
pub mod diff;
pub mod repo;
pub mod staging;
pub mod unstaging;

use git2::Repository;

use crate::error::PgsError;

/// Index mode of a gitlink: the commit a submodule is recorded at.
pub const GITLINK_MODE: u32 = 0o160_000;

/// The commit a submodule's checkout is on, which is what staging its path
/// records, as `git add <submodule>` does.
///
/// # Errors
///
/// `Git` when the path is not a repository (including a gitfile that points
/// nowhere) or its HEAD has no commit yet.
pub fn submodule_head(repo: &Repository, file_path: &str) -> Result<git2::Oid, PgsError> {
    let checkout = Repository::open(repo::workdir(repo)?.join(file_path))?;
    Ok(checkout.head()?.peel_to_commit()?.id())
}

/// The commit HEAD records for `file_path` when that path is a gitlink.
///
/// `None` when there is no HEAD, the path is absent from it, or it is not a
/// gitlink.
pub fn head_gitlink(repo: &Repository, file_path: &str) -> Option<git2::Oid> {
    let tree = repo.head().ok()?.peel_to_tree().ok()?;
    let entry = tree.get_path(std::path::Path::new(file_path)).ok()?;
    (u32::try_from(entry.filemode()).ok() == Some(GITLINK_MODE)).then(|| entry.id())
}

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
/// - `PgsError::UnsupportedEntry` when the workdir entry is not a regular file
///   or a symlink (e.g., a submodule checkout, a socket, a fifo).
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
    Err(PgsError::UnsupportedEntry { path: full })
}

/// Read a workdir path in the form `git add` would store it in the index.
///
/// Regular files pass through the repository's built-in clean filters (`core.autocrlf`,
/// `text`/`eol`, `ident`), so a CRLF checkout of an LF file stages as LF.
/// Symlink targets are never filtered. The drift checksum keeps hashing the raw
/// bytes from [`read_workdir_for_blob`], because it fingerprints the file on disk.
///
/// # Errors
/// Everything [`read_workdir_for_blob`] returns; `PgsError::ExternalFilter` when
/// the path needs a filter driver libgit2 cannot run (git-lfs, git-crypt); and
/// `PgsError::Git` when a built-in filter fails (e.g. `core.safecrlf=true` on
/// endings that cannot round-trip).
pub fn read_workdir_for_index(repo: &Repository, rel_path: &str) -> Result<WorkdirBlob, PgsError> {
    let workdir = repo::workdir(repo)?;
    let mut blob = read_workdir_for_blob(workdir, rel_path)?;
    if blob.file_type == WorkdirFileType::Regular {
        reject_external_filter(repo, rel_path)?;
        // libgit2 exposes its filters only through blob creation. The object
        // written is the one a whole-file stage writes anyway.
        let mut writer = repo.blob_writer(Some(std::path::Path::new(rel_path)))?;
        std::io::Write::write_all(&mut writer, &blob.bytes)
            .map_err(|e| PgsError::io(workdir.join(rel_path), e))?;
        let oid = writer.commit()?;
        blob.bytes = repo.find_blob(oid)?.content().to_vec();
    }
    Ok(blob)
}

/// Refuse a path whose `filter` attribute names a configured external driver.
///
/// A named filter with no `clean` or `process` command configured is skipped by
/// `git add` as well, so it is not refused.
fn reject_external_filter(repo: &Repository, rel_path: &str) -> Result<(), PgsError> {
    let attr = repo.get_attr_bytes(
        std::path::Path::new(rel_path),
        "filter",
        git2::AttrCheckFlags::default(),
    )?;
    let git2::AttrValue::String(driver) = git2::AttrValue::from_bytes(attr) else {
        return Ok(());
    };
    let config = repo.config()?;
    let configured = ["clean", "process"].iter().any(|command| {
        config
            .get_string(&format!("filter.{driver}.{command}"))
            .is_ok()
    });
    if configured {
        return Err(PgsError::ExternalFilter {
            path: rel_path.to_owned(),
            driver: driver.to_owned(),
        });
    }
    Ok(())
}

/// Build an `IndexEntry` for a file, preserving existing mode/flags from the index.
///
/// If the file is not yet in the index (new file), defaults to mode `0o100644`.
/// Pass `mode_override` to apply a different mode (e.g. for `chmod +x` staging).
///
/// Stat data is left zero, size included, as `git update-index --cacheinfo`
/// does. A nonzero size that differs from the file on disk makes git and
/// libgit2 report the file modified without comparing content, and the staged
/// blob's size differs from the disk size after a partial stage or a clean
/// filter, so a zero size is the only value that never lies.
pub fn build_index_entry(
    index: &git2::Index,
    file_path: &str,
    oid: git2::Oid,
    mode_override: Option<u32>,
) -> git2::IndexEntry {
    let (mode, flags, flags_extended) = match index.get_path(std::path::Path::new(file_path), 0) {
        // Writing real content resolves a `git add -N` placeholder.
        Some(e) => (
            mode_override.unwrap_or(e.mode),
            e.flags,
            e.flags_extended & !git2::IndexEntryExtendedFlag::INTENT_TO_ADD.bits(),
        ),
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
        file_size: 0,
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

/// `None` when there are no `\n` chars (cannot classify; skip the CRLF guard).
/// `Some(true)` when more than half of endings are `\r\n`, `Some(false)` otherwise.
#[allow(clippy::naive_bytecount)] // bytecount dep is not worth it for this guard
pub(crate) fn is_predominantly_crlf(text: &str) -> Option<bool> {
    let bytes = text.as_bytes();
    let lf_count = bytes.iter().filter(|&&b| b == b'\n').count();
    if lf_count == 0 {
        return None;
    }
    let crlf_count = bytes.windows(2).filter(|w| *w == b"\r\n").count();
    Some(crlf_count * 2 > lf_count)
}

/// Pair each deletion of a replace run with the addition that replaces it.
///
/// The partner is the first addition with identical content, else the addition
/// at the deletion's own index, else the run's last addition. A run with no
/// additions is a pure deletion, and every entry is `None`. Line selection and
/// blob assembly both use this rule, so a kept original is always emitted in
/// the slot of the addition its selection was decided by.
pub(crate) fn replace_run_partners(deletions: &[&str], additions: &[&str]) -> Vec<Option<usize>> {
    let last = additions.len().checked_sub(1);
    deletions
        .iter()
        .enumerate()
        .map(|(i, deleted)| {
            let positional = last.map(|last| i.min(last));
            additions
                .iter()
                .position(|added| added == deleted)
                .or(positional)
        })
        .collect()
}

/// Emit one buffered replace run into `out` and drain both buffers.
///
/// Each element is `(token, emit)`. Addition slots are walked in order, and a
/// kept deletion follows the addition it pairs with under
/// [`replace_run_partners`]; a pure-deletion run emits its kept lines in order.
pub(crate) fn flush_replace_run<'a>(
    deletes: &mut Vec<(&'a str, bool)>,
    inserts: &mut Vec<(&'a str, bool)>,
    out: &mut Vec<&'a str>,
) {
    let deleted_content: Vec<&str> = deletes.iter().map(|&(t, _)| line_content(t)).collect();
    let added_content: Vec<&str> = inserts.iter().map(|&(t, _)| line_content(t)).collect();
    let partners = replace_run_partners(&deleted_content, &added_content);
    let kept_deletes = |slot: Option<usize>| {
        deletes
            .iter()
            .zip(&partners)
            .filter(move |&(&(_, keep), &partner)| keep && partner == slot)
            .map(|(&(token, _), _)| token)
    };

    if inserts.is_empty() {
        out.extend(kept_deletes(None));
    }
    for (slot, &(token, keep)) in inserts.iter().enumerate() {
        if keep {
            out.push(token);
        }
        out.extend(kept_deletes(Some(slot)));
    }
    deletes.clear();
    inserts.clear();
}

/// A diff token's content without its line terminator, as scan reports it.
pub(crate) fn line_content(token: &str) -> &str {
    token.trim_end_matches('\n').trim_end_matches('\r')
}

/// Post-condition on an assembled blob: no interior element may be unterminated.
///
/// `elements` are diff tokens in emission order, each still carrying its own
/// terminator, so a token without a trailing newline can only be a file's last
/// line. One sitting anywhere else means the selection would silently merge two
/// lines, and making it representable would require terminating a line the
/// selection never named.
///
/// Checking the assembled result rather than the input shape means the guard
/// cannot false-positive: any selection that reproduces a well-formed blob
/// passes it.
///
/// # Errors
///
/// `PgsError::UnterminatedInteriorLine` when an interior element lacks a
/// trailing newline.
pub(crate) fn reject_interior_unterminated<'a>(
    file_path: &str,
    elements: impl DoubleEndedIterator<Item = &'a str>,
) -> Result<(), PgsError> {
    if elements.rev().skip(1).any(|value| !value.ends_with('\n')) {
        return Err(PgsError::UnterminatedInteriorLine {
            path: file_path.to_owned(),
        });
    }
    Ok(())
}

/// Coordinate-space–safe line selection for partial staging and unstaging.
///
/// Git diffs operate in two distinct coordinate spaces: the HEAD/index side
/// (old) and the workdir side (new). These spaces diverge the moment any line
/// is added or deleted — a line number valid in one space is meaningless in the
/// other.  Historically, callers passed a single `HashSet<u32>` and used it for
/// both `similar::ChangeTag::Delete` ops (old-side) and
/// `similar::ChangeTag::Insert` ops (new-side) interchangeably, which is the
/// coordinate-space conflation bug this type prevents.
///
/// `old_lines` gates `similar::ChangeTag::Delete` ops by the HEAD/index-side
/// 1-based line number.
///
/// `new_lines` gates `similar::ChangeTag::Insert` ops by the workdir-side
/// 1-based line number.
///
/// Never pass the same `HashSet` (or the same range) to both fields: the two
/// spaces are independent and mixing them silently stages the wrong lines.
#[derive(Default, Debug)]
pub struct LineSelection {
    /// Gates `similar::ChangeTag::Delete` ops by HEAD/index-side 1-based line number.
    pub old_lines: std::collections::HashSet<u32>,
    /// Gates `similar::ChangeTag::Insert` ops by workdir-side 1-based line number.
    pub new_lines: std::collections::HashSet<u32>,
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
            // Runner-wide config (core.autocrlf=true on Windows) must not
            // rewrite the line endings these fixtures pin.
            config
                .set_bool("core.autocrlf", false)
                .expect("set autocrlf");
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
        let oid = git2::Oid::ZERO_SHA1;
        let entry = build_index_entry(&index, "new_file.rs", oid, None);
        assert_eq!(entry.mode, 0o100_644);
        assert_eq!(entry.flags, 0);
        assert_eq!(entry.file_size, 0);
        assert_eq!(entry.path, b"new_file.rs");
    }

    #[test]
    fn build_index_entry_existing_file_preserves_mode() {
        let (_dir, repo) = setup_repo_with_commit("content");
        let index = repo.index().expect("index");
        let oid = git2::Oid::ZERO_SHA1;
        let entry = build_index_entry(&index, "file.txt", oid, None);
        assert_eq!(entry.mode, 0o100_644);
    }

    #[test]
    fn build_index_entry_mode_override_applies() {
        let (_dir, repo) = setup_repo_with_commit("content");
        let index = repo.index().expect("index");
        let oid = git2::Oid::ZERO_SHA1;
        let entry = build_index_entry(&index, "file.txt", oid, Some(0o100_755));
        assert_eq!(entry.mode, 0o100_755);
    }

    #[test]
    fn build_index_entry_mode_override_new_file() {
        let (_dir, repo) = setup_repo_with_commit("content");
        let index = repo.index().expect("index");
        let oid = git2::Oid::ZERO_SHA1;
        let entry = build_index_entry(&index, "new_file.rs", oid, Some(0o100_755));
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
    fn read_workdir_for_blob_directory_returns_unsupported_entry() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        std::fs::create_dir(dir.path().join("subdir")).expect("mkdir");
        let result = read_workdir_for_blob(dir.path(), "subdir");
        let err = result.expect_err("expected error for directory");
        assert!(
            matches!(err, crate::error::PgsError::UnsupportedEntry { .. }),
            "expected UnsupportedEntry, got: {err}"
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
