use crate::error::PgsError;
use chrono::Utc;
use git2::Repository;
use std::fs;
use std::sync::atomic::{AtomicU32, Ordering};

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

    Ok(backup_id)
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
    if backup_content.is_empty() {
        return match fs::remove_file(&index_path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(PgsError::io(&index_path, e)),
            _ => Ok(()),
        };
    }
    fs::write(&index_path, backup_content).map_err(|e| PgsError::io(&index_path, e))?;

    Ok(())
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
}
