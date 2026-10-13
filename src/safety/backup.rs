use crate::error::PgsError;
use crate::safety::lock;
use chrono::Utc;
use git2::Repository;
use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};

/// How many backups `.git/pgs/backups/` keeps, the one just taken included.
///
/// Every stage and unstage takes a backup, so without a cap the directory grows with each
/// operation. A backup matters only until its operation completes or its rollback is
/// reported, so a short history is enough to recover by hand.
pub const BACKUPS_RETAINED: usize = 50;

/// Distinguishes backups taken within the same microsecond by one process.
static BACKUP_SEQ: AtomicU32 = AtomicU32::new(0);

/// Build an id that sorts in creation order: `backup-<UTC time>-<micros>-<pid>-<seq>`.
///
/// The process id keeps concurrent pgs processes apart; the per-process sequence keeps two
/// backups in one microsecond apart and in order.
fn new_backup_id() -> String {
    let now = Utc::now();
    let seq = BACKUP_SEQ.fetch_add(1, Ordering::Relaxed);
    format!(
        "backup-{}-{:06}-{:x}-{seq:04x}",
        now.format("%Y%m%dT%H%M%S"),
        now.timestamp_subsec_micros(),
        std::process::id(),
    )
}

/// Create a backup of the current git index.
///
/// Writes the raw index file to `.git/pgs/backups/<id>.index` and returns the id.
///
/// # Errors
///
/// Returns an error if the backup directory cannot be created, the index cannot be read,
/// or writing the backup file fails.
pub fn create_backup(repo: &Repository) -> Result<String, PgsError> {
    let backup_dir = repo.path().join("pgs").join("backups");
    fs::create_dir_all(&backup_dir).map_err(|e| PgsError::io(&backup_dir, e))?;

    let index_path = repo.path().join("index");
    // A repository gets its index file on the first add; until then the
    // snapshot is empty, and restoring it removes the file again.
    let index_content = match fs::read(&index_path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(PgsError::io(&index_path, e)),
    };

    let backup_id = new_backup_id();
    let backup_file = backup_dir.join(format!("{backup_id}.index"));
    fs::write(&backup_file, &index_content).map_err(|e| PgsError::io(&backup_file, e))?;

    // Pruning is housekeeping: the new backup is already safe on disk, and failing the
    // staging operation over an old file that would not delete would be worse than
    // keeping it until the next run.
    let _ = prune_backups(&backup_dir, &backup_id);

    Ok(backup_id)
}

/// Delete all but the newest [`BACKUPS_RETAINED`] backups in `backup_dir`.
///
/// Ids sort by creation time, so the newest are the greatest names. `keep_id` is never
/// deleted, even if clock skew makes its name sort old. `.json` sidecars written by older
/// pgs versions are removed. Files that are not `backup-*` are left alone. Every deletion is
/// attempted; the first failure is returned afterwards.
///
/// # Errors
///
/// Returns an error if the directory cannot be listed or a file cannot be deleted.
fn prune_backups(backup_dir: &Path, keep_id: &str) -> Result<(), PgsError> {
    let mut stale = Vec::new();
    let mut snapshots = Vec::new();
    for entry in fs::read_dir(backup_dir).map_err(|e| PgsError::io(backup_dir, e))? {
        let entry = entry.map_err(|e| PgsError::io(backup_dir, e))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("backup-") {
            continue;
        }
        if name.strip_suffix(".json").is_some() {
            stale.push(name);
        } else if let Some(id) = name.strip_suffix(".index")
            && id != keep_id
        {
            snapshots.push(name);
        }
    }

    snapshots.sort_unstable_by(|a, b| b.cmp(a));
    stale.extend(
        snapshots
            .into_iter()
            .skip(BACKUPS_RETAINED.saturating_sub(1)),
    );

    let mut first_error = None;
    for name in stale {
        let path = backup_dir.join(name);
        match fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                first_error.get_or_insert_with(|| PgsError::io(&path, e));
            }
            _ => {}
        }
    }
    first_error.map_or(Ok(()), Err)
}

/// Restore `backup_id` after `error` aborted a mutation, and return the error to report.
///
/// That is `error` itself when the restore succeeds, and `RestoreFailed` carrying both
/// failures when it does not, since the index may then be inconsistent.
pub fn rollback(repo: &Repository, backup_id: &str, error: PgsError) -> PgsError {
    match restore_backup(repo, backup_id) {
        Ok(()) => error,
        Err(restore_error) => PgsError::RestoreFailed {
            backup_id: backup_id.to_owned(),
            op_error: error.to_string(),
            restore_error: restore_error.to_string(),
        },
    }
}

/// Restore the git index from a previously created backup.
///
/// The working tree is not modified; only the index file is overwritten.
///
/// # Errors
///
/// Returns an error if the backup does not exist, or if reading/writing the index file fails.
pub fn restore_backup(repo: &Repository, backup_id: &str) -> Result<(), PgsError> {
    let backup_dir = repo.path().join("pgs").join("backups");
    let backup_file = backup_dir.join(format!("{backup_id}.index"));

    if !backup_file.exists() {
        return Err(PgsError::Internal(format!("backup not found: {backup_id}")));
    }

    let backup_content = fs::read(&backup_file).map_err(|e| PgsError::io(&backup_file, e))?;
    let index_path = repo.path().join("index");
    let lock_path = repo.path().join("index.lock");
    // Take the lock git and libgit2 take for every index write, so no other
    // writer interleaves with the restore and no reader sees a partial index.
    lock::wait_for_lock_release(repo, 5)?;
    let mut lock_file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => PgsError::IndexLocked,
            _ => PgsError::io(&lock_path, e),
        })?;

    let written = if backup_content.is_empty() {
        drop(lock_file);
        match fs::remove_file(&index_path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(PgsError::io(&index_path, e)),
            _ => fs::remove_file(&lock_path).map_err(|e| PgsError::io(&lock_path, e)),
        }
    } else {
        lock_file
            .write_all(&backup_content)
            .and_then(|()| lock_file.sync_all())
            .map_err(|e| PgsError::io(&lock_path, e))
            .and_then(|()| {
                drop(lock_file);
                fs::rename(&lock_path, &index_path).map_err(|e| PgsError::io(&index_path, e))
            })
    };
    if written.is_err() {
        let _ = fs::remove_file(&lock_path);
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::Repository;
    use tempfile::TempDir;

    fn setup_test_repo() -> (TempDir, Repository) {
        let temp = TempDir::new().unwrap();
        let repo = Repository::init(temp.path()).unwrap();
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test").unwrap();
        config.set_str("user.email", "test@test.com").unwrap();
        let sig = repo.signature().unwrap();
        let tree_id = {
            let mut index = repo.index().unwrap();
            index.write_tree().unwrap()
        };
        {
            let tree = repo.find_tree(tree_id).unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
                .unwrap();
        }
        // Flush index to disk so the index file exists.
        let mut index = repo.index().unwrap();
        index.write().unwrap();
        (temp, repo)
    }

    #[test]
    fn restore_of_snapshot_taken_without_an_index_removes_the_index() {
        let temp = TempDir::new().unwrap();
        let repo = Repository::init(temp.path()).unwrap();
        let index_path = repo.path().join("index");
        assert!(!index_path.exists());

        let backup = create_backup(&repo).unwrap();
        fs::write(&index_path, b"written by a failed stage").unwrap();
        restore_backup(&repo, &backup).unwrap();

        assert!(!index_path.exists());
    }

    #[test]
    fn create_backup_creates_files_in_backup_dir() {
        let (_temp, repo) = setup_test_repo();
        let info = create_backup(&repo).expect("create_backup failed");

        let backup_dir = repo.path().join("pgs").join("backups");
        let index_file = backup_dir.join(format!("{info}.index"));

        assert!(index_file.exists(), "index backup file should exist");
        assert!(!backup_dir.join(format!("{info}.json")).exists());
    }

    #[test]
    fn create_backup_content_matches_original_index() {
        let (_temp, repo) = setup_test_repo();
        let index_path = repo.path().join("index");
        let original = fs::read(&index_path).unwrap();

        let info = create_backup(&repo).expect("create_backup failed");

        let backup_dir = repo.path().join("pgs").join("backups");
        let backup_file = backup_dir.join(format!("{info}.index"));
        let backed_up = fs::read(&backup_file).unwrap();

        assert_eq!(
            original, backed_up,
            "backed-up content must match original index"
        );
    }

    #[test]
    fn restore_backup_restores_original_index_state() {
        let (_temp, repo) = setup_test_repo();
        let index_path = repo.path().join("index");
        let original = fs::read(&index_path).unwrap();

        let info = create_backup(&repo).expect("create_backup failed");

        // Corrupt the index to simulate a change.
        fs::write(&index_path, b"corrupted").unwrap();
        assert_ne!(fs::read(&index_path).unwrap(), original);

        restore_backup(&repo, &info).expect("restore_backup failed");
        let restored = fs::read(&index_path).unwrap();
        assert_eq!(restored, original, "restored index must match original");
    }

    #[test]
    fn restore_backup_refuses_while_another_writer_holds_the_index_lock() {
        let (_temp, repo) = setup_test_repo();
        let index_path = repo.path().join("index");
        let lock_path = repo.path().join("index.lock");
        let backup = create_backup(&repo).unwrap();
        fs::write(&index_path, b"written by a failed stage").unwrap();
        fs::write(&lock_path, b"held by git").unwrap();

        let err = restore_backup(&repo, &backup).unwrap_err();

        assert!(matches!(err, PgsError::IndexLocked), "got: {err}");
        assert_eq!(fs::read(&index_path).unwrap(), b"written by a failed stage");
        assert_eq!(fs::read(&lock_path).unwrap(), b"held by git");
    }

    #[test]
    fn restore_backup_releases_the_index_lock_it_takes() {
        let (_temp, repo) = setup_test_repo();
        let backup = create_backup(&repo).unwrap();

        restore_backup(&repo, &backup).unwrap();

        assert!(!repo.path().join("index.lock").exists());
    }

    #[test]
    fn restore_nonexistent_backup_returns_error() {
        let (_temp, repo) = setup_test_repo();
        let result = restore_backup(&repo, "backup-does-not-exist");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, PgsError::Internal(_)),
            "expected Internal error, got: {err}"
        );
    }

    fn backup_dir_of(repo: &Repository) -> std::path::PathBuf {
        repo.path().join("pgs").join("backups")
    }

    fn index_ids(repo: &Repository) -> Vec<String> {
        let mut ids: Vec<String> = fs::read_dir(backup_dir_of(repo))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter_map(|n| n.strip_suffix(".index").map(str::to_owned))
            .collect();
        ids.sort();
        ids
    }

    #[test]
    fn create_backup_back_to_back_yields_distinct_ids_in_creation_order() {
        let (_temp, repo) = setup_test_repo();
        let ids: Vec<String> = (0..20).map(|_| create_backup(&repo).unwrap()).collect();

        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted, ids, "ids must be unique and sort in creation order");
    }

    #[test]
    fn create_backup_beyond_retention_keeps_only_the_newest() {
        let (_temp, repo) = setup_test_repo();
        let ids: Vec<String> = (0..60).map(|_| create_backup(&repo).unwrap()).collect();

        assert_eq!(index_ids(&repo), ids[60 - BACKUPS_RETAINED..]);

        let index_path = repo.path().join("index");
        let original = fs::read(&index_path).unwrap();
        fs::write(&index_path, b"corrupted").unwrap();
        restore_backup(&repo, ids.last().unwrap()).unwrap();
        assert_eq!(fs::read(&index_path).unwrap(), original);
    }

    #[test]
    fn create_backup_removes_legacy_json_sidecars_and_spares_other_files() {
        let (_temp, repo) = setup_test_repo();
        let dir = backup_dir_of(&repo);
        fs::create_dir_all(&dir).unwrap();
        let legacy = dir.join("backup-20240101T000000-deadbeef.json");
        let unrelated = dir.join("notes.json");
        fs::write(&legacy, b"{}").unwrap();
        fs::write(&unrelated, b"{}").unwrap();

        let id = create_backup(&repo).unwrap();

        assert!(!legacy.exists());
        assert!(unrelated.exists());
        assert!(dir.join(format!("{id}.index")).exists());
    }

    #[test]
    fn create_backup_never_prunes_itself_when_older_names_sort_newer() {
        let (_temp, repo) = setup_test_repo();
        let dir = backup_dir_of(&repo);
        fs::create_dir_all(&dir).unwrap();
        for n in 0..BACKUPS_RETAINED + 10 {
            fs::write(
                dir.join(format!("backup-29990101T0000{n:02}-deadbeef.index")),
                b"",
            )
            .unwrap();
        }

        let id = create_backup(&repo).unwrap();

        let ids = index_ids(&repo);
        assert_eq!(ids.len(), BACKUPS_RETAINED);
        assert!(ids.contains(&id));
    }

    #[test]
    fn restore_backup_accepts_an_id_from_before_the_id_format_changed() {
        let (_temp, repo) = setup_test_repo();
        let dir = backup_dir_of(&repo);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("backup-20240101T000000-1a2b3c4d.index"),
            b"legacy index",
        )
        .unwrap();

        restore_backup(&repo, "backup-20240101T000000-1a2b3c4d").unwrap();

        assert_eq!(
            fs::read(repo.path().join("index")).unwrap(),
            b"legacy index"
        );
    }
}
