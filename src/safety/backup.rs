use crate::error::AgstageError;
use crate::models::BackupInfo;
use chrono::Utc;
use git2::Repository;
use sha2::{Digest, Sha256};
use std::fs;
use uuid::Uuid;

/// Create a backup of the current git index.
///
/// Writes the raw index file to `.git/agstage/backups/` along with a JSON metadata file.
///
/// # Errors
///
/// Returns an error if the backup directory cannot be created, the index cannot be read,
/// or writing the backup files fails.
pub fn create_backup(repo: &Repository) -> Result<BackupInfo, AgstageError> {
    let backup_dir = repo.path().join("agstage").join("backups");
    fs::create_dir_all(&backup_dir).map_err(|e| AgstageError::io(&backup_dir, e))?;

    let index_path = repo.path().join("index");
    let index_content = fs::read(&index_path).map_err(|e| AgstageError::io(&index_path, e))?;

    let mut hasher = Sha256::new();
    hasher.update(&index_content);
    let index_checksum = format!("{:x}", hasher.finalize());

    let timestamp = Utc::now().format("%Y%m%dT%H%M%S");
    let uuid = Uuid::new_v4();
    let uuid8 = &uuid.to_string()[..8];
    let backup_id = format!("backup-{timestamp}-{uuid8}");

    let backup_file = backup_dir.join(format!("{backup_id}.index"));
    fs::write(&backup_file, &index_content).map_err(|e| AgstageError::io(&backup_file, e))?;

    let backup_info = BackupInfo {
        backup_id,
        timestamp: Utc::now().to_rfc3339(),
        index_checksum,
    };

    let metadata_file = backup_dir.join(format!("{}.json", backup_info.backup_id));
    let metadata_json = serde_json::to_string_pretty(&backup_info)?;
    fs::write(&metadata_file, metadata_json).map_err(|e| AgstageError::io(&metadata_file, e))?;

    Ok(backup_info)
}

/// Restore the git index from a previously created backup.
///
/// The working tree is not modified; only the index file is overwritten.
///
/// # Errors
///
/// Returns an error if the backup does not exist, or if reading/writing the index file fails.
pub fn restore_backup(repo: &Repository, backup_id: &str) -> Result<(), AgstageError> {
    let backup_dir = repo.path().join("agstage").join("backups");
    let backup_file = backup_dir.join(format!("{backup_id}.index"));

    if !backup_file.exists() {
        return Err(AgstageError::Internal(format!(
            "backup not found: {backup_id}"
        )));
    }

    let backup_content = fs::read(&backup_file).map_err(|e| AgstageError::io(&backup_file, e))?;
    let index_path = repo.path().join("index");
    fs::write(&index_path, backup_content).map_err(|e| AgstageError::io(&index_path, e))?;

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
    fn create_backup_creates_files_in_backup_dir() {
        let (_temp, repo) = setup_test_repo();
        let info = create_backup(&repo).expect("create_backup failed");

        let backup_dir = repo.path().join("agstage").join("backups");
        let index_file = backup_dir.join(format!("{}.index", info.backup_id));
        let meta_file = backup_dir.join(format!("{}.json", info.backup_id));

        assert!(index_file.exists(), "index backup file should exist");
        assert!(meta_file.exists(), "metadata json file should exist");
    }

    #[test]
    fn create_backup_content_matches_original_index() {
        let (_temp, repo) = setup_test_repo();
        let index_path = repo.path().join("index");
        let original = fs::read(&index_path).unwrap();

        let info = create_backup(&repo).expect("create_backup failed");

        let backup_dir = repo.path().join("agstage").join("backups");
        let backup_file = backup_dir.join(format!("{}.index", info.backup_id));
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

        restore_backup(&repo, &info.backup_id).expect("restore_backup failed");
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
            matches!(err, AgstageError::Internal(_)),
            "expected Internal error, got: {err}"
        );
    }
}
