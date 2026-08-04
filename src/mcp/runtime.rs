use std::{
    collections::{BTreeSet, HashMap},
    path::PathBuf,
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use rmcp::{
    ErrorData,
    model::{CallToolResult, ErrorCode, RequestId},
    service::{RequestContext, RoleServer},
};
use tokio::sync::Notify;

use crate::{
    cmd::mcp_adapter::{self, McpCommandRequest},
    git::repo,
    mcp::contract::{
        PGS_COMMIT_TOOL, PGS_LOG_TOOL, PGS_SCAN_TOOL, PGS_STAGE_TOOL, PGS_STATUS_TOOL,
        PGS_UNSTAGE_TOOL, map_execution_result,
    },
};

/// In-memory coordinator for per-repo mutation ordering.
#[derive(Debug, Default)]
pub struct PgsMcpRuntime {
    mutation_lanes: StdMutex<HashMap<PathBuf, Arc<MutationLane>>>,
    preregistered_mutations: StdMutex<HashMap<RequestOrderKey, RegisteredMutation>>,
    next_arrival_sequence: AtomicU64,
}

#[derive(Debug, Default)]
struct MutationLane {
    state: StdMutex<MutationLaneState>,
    notify: Notify,
}

#[derive(Debug, Default)]
struct MutationLaneState {
    active: Option<MutationOrder>,
    pending: BTreeSet<MutationOrder>,
}

/// Declaration order IS the sort order: the derived `Ord` is lexicographic by
/// field, so `arrival_sequence` must stay first to keep lane admission FIFO by
/// arrival rather than by client-chosen request id.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
struct MutationOrder {
    arrival_sequence: u64,
    request_key: RequestOrderKey,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
enum RequestOrderKey {
    Number(i64),
    /// Shares rmcp's own `Arc<str>` allocation; `Ord`/`Hash` still delegate to
    /// `str`, so ordering and lookup semantics match an owned `String`.
    String(Arc<str>),
}

#[derive(Debug)]
struct MutationPermit {
    lane: Option<Arc<MutationLane>>,
    order: Option<MutationOrder>,
}

#[derive(Debug, Clone)]
struct RegisteredMutation {
    lane: Arc<MutationLane>,
    order: MutationOrder,
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
        tool_name: &'static str,
        command: McpCommandRequest,
    ) -> Result<CallToolResult, ErrorData> {
        let output = tokio::task::spawn_blocking(move || {
            maybe_test_delay(tool_name);
            mcp_adapter::execute(command)
        })
        .await
        .map_err(|error| {
            ErrorData::internal_error(format!("tool task join failed: {error}"), None)
        })?;

        map_execution_result(output)
            .map_err(|error| ErrorData::internal_error(error.to_string(), None))
    }

    /// Execute a mutating MCP command after acquiring the per-repo mutation lane.
    ///
    /// # Errors
    ///
    /// Returns an MCP error if preregistration data is invalid, the repository
    /// path cannot be canonicalized, the request is cancelled before execution
    /// begins, or the adapter result cannot be translated into an MCP response.
    pub async fn execute_mutation(
        &self,
        tool_name: &'static str,
        repo_path: &str,
        command: McpCommandRequest,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let request_key = RequestOrderKey::from_request_id(&context.id);
        let registered = self.take_preregistered_mutation(&request_key);
        let RegisteredMutation { lane, order } = if let Some(registered) = registered {
            registered
        } else {
            let repo_key = canonical_repo_path(repo_path)?;
            let order = self.next_mutation_order(&context.id);
            let lane = self.repo_lane(repo_key);
            lane.enqueue(order.clone());
            RegisteredMutation { lane, order }
        };

        let permit = tokio::select! {
            () = context.ct.cancelled() => {
                lane.cancel_pending(&order);
                return Err(cancelled_mutation_error());
            },
            permit = lane.acquire(order.clone()) => permit,
        };

        if context.ct.is_cancelled() {
            return Err(cancelled_mutation_error());
        }

        self.execute_mutation_with_permit(tool_name, command, permit)
            .await
    }

    async fn execute_mutation_with_permit(
        &self,
        tool_name: &'static str,
        command: McpCommandRequest,
        _permit: MutationPermit,
    ) -> Result<CallToolResult, ErrorData> {
        self.execute_command(tool_name, command).await
    }

    /// Reserve mutation ordering for a direct mutating request before handler scheduling races.
    ///
    /// # Errors
    ///
    /// Returns an internal MCP error if the repository path cannot be opened,
    /// resolved to a worktree, or canonicalized.
    pub fn preregister_mutation(
        &self,
        request_id: &RequestId,
        repo_path: &str,
    ) -> Result<(), ErrorData> {
        let repo_key = canonical_repo_path(repo_path)?;
        let order = self.next_mutation_order(request_id);
        let lane = self.repo_lane(repo_key);
        lane.enqueue(order.clone());
        let request_key = RequestOrderKey::from_request_id(request_id);
        let displaced = lock_std_mutex(&self.preregistered_mutations)
            .insert(request_key, RegisteredMutation { lane, order });
        // A client that reuses an in-flight request id violates JSON-RPC, but the
        // displaced registration still owns a pending lane slot no handler will
        // ever claim, and `pending.first()` would block that repo's lane forever.
        if let Some(stale) = displaced {
            stale.lane.cancel_pending(&stale.order);
        }
        Ok(())
    }

    /// Drop a preregistered mutation and free its pending lane slot, if present.
    pub fn discard_preregistered_mutation(&self, request_id: &RequestId) {
        let request_key = RequestOrderKey::from_request_id(request_id);
        if let Some(registered) = self.take_preregistered_mutation(&request_key) {
            registered.lane.cancel_pending(&registered.order);
        }
    }

    fn repo_lane(&self, repo_key: PathBuf) -> Arc<MutationLane> {
        let mut lanes = lock_std_mutex(&self.mutation_lanes);
        Arc::clone(
            lanes
                .entry(repo_key)
                .or_insert_with(|| Arc::new(MutationLane::default())),
        )
    }

    fn next_mutation_order(&self, request_id: &RequestId) -> MutationOrder {
        MutationOrder {
            arrival_sequence: self.next_arrival_sequence.fetch_add(1, Ordering::Relaxed),
            request_key: RequestOrderKey::from_request_id(request_id),
        }
    }

    fn take_preregistered_mutation(
        &self,
        request_key: &RequestOrderKey,
    ) -> Option<RegisteredMutation> {
        let mut registrations = lock_std_mutex(&self.preregistered_mutations);
        registrations.remove(request_key)
    }
}

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

fn maybe_test_delay(tool_name: &str) {
    let env_name = match tool_name {
        PGS_SCAN_TOOL => Some("PGS_MCP_TEST_SCAN_DELAY_MS"),
        PGS_STATUS_TOOL => Some("PGS_MCP_TEST_STATUS_DELAY_MS"),
        PGS_STAGE_TOOL => Some("PGS_MCP_TEST_STAGE_DELAY_MS"),
        PGS_UNSTAGE_TOOL => Some("PGS_MCP_TEST_UNSTAGE_DELAY_MS"),
        PGS_COMMIT_TOOL => Some("PGS_MCP_TEST_COMMIT_DELAY_MS"),
        PGS_LOG_TOOL => Some("PGS_MCP_TEST_LOG_DELAY_MS"),
        _ => None,
    };

    let Some(env_name) = env_name else {
        return;
    };

    let Ok(value) = std::env::var(env_name) else {
        return;
    };
    let Ok(delay_ms) = value.parse::<u64>() else {
        return;
    };

    std::thread::sleep(Duration::from_millis(delay_ms));
}

impl MutationLane {
    fn enqueue(&self, order: MutationOrder) {
        lock_std_mutex(&self.state).pending.insert(order);
        self.notify.notify_waiters();
    }

    async fn acquire(self: &Arc<Self>, order: MutationOrder) -> MutationPermit {
        loop {
            let notified = {
                let mut state = lock_std_mutex(&self.state);
                let next_pending = state.pending.first().cloned();
                if state.active.is_none() && next_pending.as_ref() == Some(&order) {
                    state.pending.remove(&order);
                    state.active = Some(order.clone());
                    drop(state);
                    return MutationPermit {
                        lane: Some(Arc::clone(self)),
                        order: Some(order),
                    };
                }

                self.notify.notified()
            };

            notified.await;
        }
    }

    fn release(&self, order: &MutationOrder) {
        let mut state = lock_std_mutex(&self.state);
        if state.active.as_ref() == Some(order) {
            state.active = None;
        }
        drop(state);
        self.notify.notify_waiters();
    }

    fn cancel_pending(&self, order: &MutationOrder) {
        let removed = lock_std_mutex(&self.state).pending.remove(order);
        if removed {
            self.notify.notify_waiters();
        }
    }
}

impl Drop for MutationPermit {
    fn drop(&mut self) {
        if let (Some(lane), Some(order)) = (self.lane.take(), self.order.take()) {
            lane.release(&order);
        }
    }
}

impl RequestOrderKey {
    fn from_request_id(request_id: &RequestId) -> Self {
        match request_id {
            RequestId::Number(value) => Self::Number(*value),
            RequestId::String(value) => Self::String(Arc::clone(value)),
        }
    }
}

fn lock_std_mutex<T>(mutex: &StdMutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::{BTreeSet, MutationOrder, PgsMcpRuntime, RequestOrderKey, lock_std_mutex};
    use rmcp::model::RequestId;

    #[test]
    fn mutation_order_sorts_by_arrival_sequence_not_request_id() {
        let first = MutationOrder {
            arrival_sequence: 0,
            request_key: RequestOrderKey::String("z".into()),
        };
        let second = MutationOrder {
            arrival_sequence: 1,
            request_key: RequestOrderKey::Number(1),
        };

        assert!(first < second);
    }

    #[test]
    fn preregister_mutation_with_reused_request_id_leaves_one_pending_slot() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        git2::Repository::init(dir.path()).expect("init repo");
        let repo_path = dir.path().to_str().expect("utf-8 path");

        let runtime = PgsMcpRuntime::default();
        let request_id = RequestId::Number(7);
        runtime
            .preregister_mutation(&request_id, repo_path)
            .expect("first preregistration");
        runtime
            .preregister_mutation(&request_id, repo_path)
            .expect("second preregistration");

        let registered = runtime
            .take_preregistered_mutation(&RequestOrderKey::from_request_id(&request_id))
            .expect("surviving registration");
        let pending = lock_std_mutex(&registered.lane.state).pending.clone();

        assert_eq!(pending, BTreeSet::from([registered.order]));
    }
}
