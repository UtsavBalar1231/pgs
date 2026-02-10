use crate::error::AgstageError;
use git2::Repository;
use std::thread;
use std::time::Duration;

/// Check if the git index is locked.
pub fn is_index_locked(repo: &Repository) -> bool {
    let lock_path = repo.path().join("index.lock");
    lock_path.exists()
}

/// Wait for the index lock to be released with exponential backoff.
///
/// Retries up to `max_retries` times. Delay starts at 50ms and doubles each attempt.
///
/// # Errors
///
/// Returns [`AgstageError::IndexLocked`] if the lock is still held after all retries.
pub fn wait_for_lock_release(repo: &Repository, max_retries: u32) -> Result<(), AgstageError> {
    for attempt in 0..max_retries {
        if !is_index_locked(repo) {
            return Ok(());
        }
        let delay_ms = 50 * 2_u64.pow(attempt);
        thread::sleep(Duration::from_millis(delay_ms));
    }
    if is_index_locked(repo) {
        Err(AgstageError::IndexLocked)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::Repository;
    use std::fs;
    use tempfile::TempDir;

    fn setup_repo() -> (TempDir, Repository) {
        let temp = TempDir::new().unwrap();
        let repo = Repository::init(temp.path()).unwrap();
        (temp, repo)
    }

    #[test]
    fn is_index_locked_returns_false_when_no_lock_file() {
        let (_temp, repo) = setup_repo();
        assert!(!is_index_locked(&repo));
    }

    #[test]
    fn is_index_locked_returns_true_when_lock_exists() {
        let (_temp, repo) = setup_repo();
        let lock_path = repo.path().join("index.lock");
        fs::write(&lock_path, b"").unwrap();
        assert!(is_index_locked(&repo));
    }

    #[test]
    fn wait_for_lock_release_succeeds_when_unlocked() {
        let (_temp, repo) = setup_repo();
        // No lock file — returns Ok immediately without sleeping.
        let result = wait_for_lock_release(&repo, 3);
        assert!(result.is_ok());
    }

    #[test]
    fn wait_for_lock_release_fails_when_permanently_locked() {
        let (_temp, repo) = setup_repo();
        let lock_path = repo.path().join("index.lock");
        fs::write(&lock_path, b"").unwrap();
        // max_retries=2 keeps the test fast; lock is never removed.
        let result = wait_for_lock_release(&repo, 2);
        assert!(matches!(result, Err(AgstageError::IndexLocked)));
    }
}
