use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex as StdMutex},
};

use rmcp::{
    ErrorData,
    model::{CallToolResult, ErrorCode},
    service::{RequestContext, RoleServer},
};
use tokio::sync::{Mutex, OwnedMutexGuard};

use crate::{
    cmd::mcp_adapter::{self, McpCommandRequest},
    git::repo,
    mcp::contract::map_execution_result,
};

/// In-memory coordinator that serializes mutations per repository.
///
/// Each canonical worktree path owns one async mutex. Mutations on the same
/// repository run one at a time and wait in FIFO order of their `lock()`
/// calls (tokio's `Mutex` is fair). That is the order in which handler tasks
/// reach the lock, not the order requests arrived on the wire.
#[derive(Debug, Default)]
pub struct PgsMcpRuntime {
    repo_locks: StdMutex<HashMap<PathBuf, Arc<Mutex<()>>>>,
}

impl PgsMcpRuntime {
    /// Execute a typed MCP command on a blocking worker and map it into an MCP result.
    ///
    /// # Errors
    ///
    /// Returns an internal MCP error if the blocking worker fails to join or if
    /// the adapter result cannot be translated into the final MCP response.
    pub async fn execute_command(
        &self,
        command: McpCommandRequest,
    ) -> Result<CallToolResult, ErrorData> {
        let output = tokio::task::spawn_blocking(move || mcp_adapter::execute(command))
            .await
            .map_err(|error| {
                ErrorData::internal_error(format!("tool task join failed: {error}"), None)
            })?;

        map_execution_result(output)
            .map_err(|error| ErrorData::internal_error(error.to_string(), None))
    }

    /// Execute a mutating MCP command while holding the repository's mutation lock.
    ///
    /// # Errors
    ///
    /// Returns an MCP error if the request is cancelled before execution begins
    /// or the adapter result cannot be translated into an MCP response.
    pub async fn execute_mutation(
        &self,
        repo_path: &str,
        command: McpCommandRequest,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        // A path that resolves to no repository has no index to protect. The
        // command path reports it with the same structured error as a read.
        let Ok(repo_key) = canonical_repo_path(repo_path) else {
            return self.execute_command(command).await;
        };

        let _guard =
            lock_unless_cancelled(self.repo_lock(repo_key), context.ct.cancelled()).await?;
        if context.ct.is_cancelled() {
            return Err(cancelled_mutation_error());
        }

        self.execute_command(command).await
    }

    fn repo_lock(&self, repo_key: PathBuf) -> Arc<Mutex<()>> {
        let mut locks = self
            .repo_locks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Arc::clone(locks.entry(repo_key).or_default())
    }
}

/// Wait for `lock`, giving up as soon as `cancelled` resolves.
///
/// Cancellation is polled first, so a request already cancelled never takes
/// the lock even when it is free.
async fn lock_unless_cancelled(
    lock: Arc<Mutex<()>>,
    cancelled: impl Future<Output = ()>,
) -> Result<OwnedMutexGuard<()>, ErrorData> {
    tokio::select! {
        biased;
        () = cancelled => Err(cancelled_mutation_error()),
        guard = lock.lock_owned() => Ok(guard),
    }
}

/// Resolve `repo_path` to its canonical worktree path, so the worktree path
/// and its `.git` path share one mutation lock.
fn canonical_repo_path(repo_path: &str) -> Result<PathBuf, ErrorData> {
    let repository = repo::open(Some(repo_path))
        .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
    let workdir = repo::workdir(&repository)
        .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;

    std::fs::canonicalize(workdir).map_err(|error| {
        ErrorData::internal_error(format!("failed to canonicalize repo path: {error}"), None)
    })
}

/// Abort value for a mutation cancelled before it touched the index. The `Err`
/// return it feeds is what stops the mutation; the payload is never observable,
/// since rmcp answers a cancelled request with no response at all.
fn cancelled_mutation_error() -> ErrorData {
    ErrorData::new(
        ErrorCode::INVALID_REQUEST,
        "mutation request cancelled before execution started",
        None,
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::Mutex;

    use super::{
        PgsMcpRuntime, cancelled_mutation_error, canonical_repo_path, lock_unless_cancelled,
    };

    #[tokio::test]
    async fn lock_unless_cancelled_with_held_lock_and_cancelled_request_returns_cancelled_error() {
        let lock = Arc::new(Mutex::new(()));
        let _holder = Arc::clone(&lock).lock_owned().await;

        let result = lock_unless_cancelled(Arc::clone(&lock), std::future::ready(())).await;

        assert_eq!(result.err(), Some(cancelled_mutation_error()));
    }

    #[tokio::test]
    async fn lock_unless_cancelled_with_free_lock_and_cancelled_request_returns_cancelled_error() {
        let lock = Arc::new(Mutex::new(()));

        let result = lock_unless_cancelled(Arc::clone(&lock), std::future::ready(())).await;

        assert_eq!(result.err(), Some(cancelled_mutation_error()));
        assert!(
            lock.try_lock().is_ok(),
            "a cancelled waiter must not keep the lock"
        );
    }

    #[tokio::test]
    async fn lock_unless_cancelled_cancelled_while_waiting_releases_queue_slot() {
        let lock = Arc::new(Mutex::new(()));
        let holder = Arc::clone(&lock).lock_owned().await;
        let (cancel, cancelled) = tokio::sync::oneshot::channel::<()>();

        let waiter = tokio::spawn(lock_unless_cancelled(Arc::clone(&lock), async {
            let _ = cancelled.await;
        }));
        cancel.send(()).expect("waiter is still listening");

        let result = waiter.await.expect("waiter task joins");
        assert_eq!(result.err(), Some(cancelled_mutation_error()));

        drop(holder);
        assert!(
            lock.try_lock().is_ok(),
            "a cancelled waiter must leave the queue"
        );
    }

    #[tokio::test]
    async fn lock_unless_cancelled_with_free_lock_and_live_request_acquires_lock() {
        let lock = Arc::new(Mutex::new(()));

        let guard = lock_unless_cancelled(Arc::clone(&lock), std::future::pending())
            .await
            .expect("free lock is acquired");

        assert!(
            lock.try_lock().is_err(),
            "the returned guard must hold the lock"
        );
        drop(guard);
    }

    #[test]
    fn repo_lock_worktree_and_git_dir_paths_share_one_lock() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        git2::Repository::init(dir.path()).expect("init repo");
        let worktree = dir.path().to_str().expect("utf-8 path");
        let git_dir = dir.path().join(".git");
        let git_dir = git_dir.to_str().expect("utf-8 path");

        let runtime = PgsMcpRuntime::default();
        let from_worktree = runtime.repo_lock(canonical_repo_path(worktree).expect("worktree"));
        let from_git_dir = runtime.repo_lock(canonical_repo_path(git_dir).expect("git dir"));

        assert!(Arc::ptr_eq(&from_worktree, &from_git_dir));
    }
}
