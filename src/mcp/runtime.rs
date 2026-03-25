use std::{
    collections::{BTreeSet, HashMap},
    path::PathBuf,
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use chrono::Utc;
use rmcp::{
    ErrorData,
    model::{
        CallToolResult, CancelTaskResult, CreateTaskResult, ErrorCode, GetTaskPayloadResult,
        GetTaskResult, ListTasksResult, RequestId, Task, TaskStatus,
    },
    service::{RequestContext, RoleServer},
};
use serde_json::Value;
use tokio::sync::{Mutex, Notify, oneshot};
use uuid::Uuid;

use crate::{
    cmd::mcp_adapter::{self, McpCommandRequest},
    git::repo,
    mcp::contract::{
        PGS_COMMIT_TOOL, PGS_LOG_TOOL, PGS_SCAN_TOOL, PGS_STAGE_TOOL, PGS_STATUS_TOOL,
        PGS_UNSTAGE_TOOL, map_execution_result,
    },
};

/// In-memory coordinator for MCP read tasks and per-repo mutation ordering.
#[derive(Debug, Default)]
pub struct PgsMcpRuntime {
    tasks: Mutex<TaskRegistry>,
    mutation_lanes: StdMutex<HashMap<PathBuf, Arc<MutationLane>>>,
    preregistered_mutations: StdMutex<HashMap<RequestOrderKey, RegisteredMutation>>,
    next_arrival_sequence: AtomicU64,
}

#[derive(Debug, Default)]
struct TaskRegistry {
    entries: HashMap<String, TaskEntry>,
}

#[derive(Debug)]
struct TaskEntry {
    task: Task,
    result: Option<Value>,
    cancel_sender: Option<oneshot::Sender<()>>,
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

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
struct MutationOrder {
    request_key: RequestOrderKey,
    arrival_sequence: u64,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
enum RequestOrderKey {
    Number(i64),
    String(String),
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

#[derive(Debug)]
enum TaskCompletion {
    Completed(Value),
    Failed(String),
    Cancelled,
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

    /// Register and spawn a server-side task for a read-only MCP command.
    ///
    /// # Errors
    ///
    /// This method currently returns `Ok` after in-memory task registration
    /// completes; the `Result` is retained to match the surrounding MCP server
    /// interface.
    pub async fn enqueue_read_task(
        self: &Arc<Self>,
        tool_name: &'static str,
        command: McpCommandRequest,
    ) -> Result<CreateTaskResult, ErrorData> {
        let task_id = format!("task-{}", Uuid::new_v4().simple());
        let now = Utc::now().to_rfc3339();
        let task = Task::new(task_id.clone(), TaskStatus::Working, now.clone(), now)
            .with_status_message(format!("Running {tool_name}"))
            .with_poll_interval(25);
        let (cancel_sender, cancel_receiver) = oneshot::channel();

        {
            let mut tasks = self.tasks.lock().await;
            tasks.entries.insert(
                task_id.clone(),
                TaskEntry {
                    task: task.clone(),
                    result: None,
                    cancel_sender: Some(cancel_sender),
                },
            );
        }

        let runtime = Arc::clone(self);
        tokio::spawn(async move {
            let completion = runtime.run_task(tool_name, command, cancel_receiver).await;
            runtime.finish_task(&task_id, completion).await;
        });

        Ok(CreateTaskResult::new(task))
    }

    /// List every server-side task currently tracked by the runtime.
    pub async fn list_tasks(&self) -> ListTasksResult {
        let mut tasks: Vec<Task> = self
            .tasks
            .lock()
            .await
            .entries
            .values()
            .map(|entry| entry.task.clone())
            .collect();
        tasks.sort_by(|left, right| left.task_id.cmp(&right.task_id));
        ListTasksResult::new(tasks)
    }

    /// Return metadata for a previously created server-side task.
    ///
    /// # Errors
    ///
    /// Returns `invalid_params` if `task_id` does not identify a known task.
    pub async fn get_task_info(&self, task_id: &str) -> Result<GetTaskResult, ErrorData> {
        let tasks = self.tasks.lock().await;
        let task = {
            let entry = tasks
                .entries
                .get(task_id)
                .ok_or_else(|| unknown_task_error(task_id))?;
            entry.task.clone()
        };
        drop(tasks);

        Ok(GetTaskResult { meta: None, task })
    }

    /// Return the serialized payload for a completed server-side task.
    ///
    /// # Errors
    ///
    /// Returns `invalid_params` if `task_id` is unknown, `invalid_request` if
    /// the task is not yet completed, or `internal_error` if a completed task is
    /// missing its stored result.
    pub async fn get_task_result(&self, task_id: &str) -> Result<GetTaskPayloadResult, ErrorData> {
        let tasks = self.tasks.lock().await;
        let (status, result) = {
            let entry = tasks
                .entries
                .get(task_id)
                .ok_or_else(|| unknown_task_error(task_id))?;
            (entry.task.status.clone(), entry.result.clone())
        };
        drop(tasks);

        match status {
            TaskStatus::Completed => result.map(GetTaskPayloadResult::new).ok_or_else(|| {
                ErrorData::internal_error("completed task is missing a result", None)
            }),
            status => Err(ErrorData::new(
                ErrorCode::INVALID_REQUEST,
                format!(
                    "task result is unavailable while status is {}",
                    task_status_label(&status)
                ),
                None,
            )),
        }
    }

    /// Cancel a server-side task and signal its worker if it is still running.
    ///
    /// # Errors
    ///
    /// Returns `invalid_params` if `task_id` does not identify a known task.
    pub async fn cancel_task(&self, task_id: &str) -> Result<CancelTaskResult, ErrorData> {
        let mut tasks = self.tasks.lock().await;
        let task = {
            let entry = tasks
                .entries
                .get_mut(task_id)
                .ok_or_else(|| unknown_task_error(task_id))?;

            if entry.task.status == TaskStatus::Working {
                entry.task.status = TaskStatus::Cancelled;
                entry.task.status_message = Some("Task cancelled".to_owned());
                entry.task.last_updated_at = Utc::now().to_rfc3339();
                entry.result = None;
                if let Some(cancel_sender) = entry.cancel_sender.take() {
                    let _ = cancel_sender.send(());
                }
            }

            entry.task.clone()
        };
        drop(tasks);

        Ok(CancelTaskResult { meta: None, task })
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
        lock_std_mutex(&self.preregistered_mutations)
            .insert(request_key, RegisteredMutation { lane, order });
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
            request_key: RequestOrderKey::from_request_id(request_id),
            arrival_sequence: self.next_arrival_sequence.fetch_add(1, Ordering::Relaxed),
        }
    }

    fn take_preregistered_mutation(
        &self,
        request_key: &RequestOrderKey,
    ) -> Option<RegisteredMutation> {
        let mut registrations = lock_std_mutex(&self.preregistered_mutations);
        registrations.remove(request_key)
    }

    async fn run_task(
        &self,
        tool_name: &'static str,
        command: McpCommandRequest,
        cancel_receiver: oneshot::Receiver<()>,
    ) -> TaskCompletion {
        let mut cancel_receiver = cancel_receiver;

        tokio::select! {
            _ = &mut cancel_receiver => TaskCompletion::Cancelled,
            result = self.execute_command(tool_name, command) => match result {
                Ok(payload) => match serde_json::to_value(payload) {
                    Ok(value) => TaskCompletion::Completed(value),
                    Err(error) => TaskCompletion::Failed(format!("failed to serialize task result: {error}")),
                },
                Err(error) => TaskCompletion::Failed(error.message.to_string()),
            },
        }
    }

    async fn finish_task(&self, task_id: &str, completion: TaskCompletion) {
        let mut tasks = self.tasks.lock().await;
        let should_drop = {
            let Some(entry) = tasks.entries.get_mut(task_id) else {
                return;
            };

            if entry.task.status != TaskStatus::Working {
                return;
            }

            entry.cancel_sender = None;
            entry.task.last_updated_at = Utc::now().to_rfc3339();

            match completion {
                TaskCompletion::Completed(result) => {
                    entry.task.status = TaskStatus::Completed;
                    entry.task.status_message = Some("Task completed".to_owned());
                    entry.result = Some(result);
                }
                TaskCompletion::Failed(message) => {
                    entry.task.status = TaskStatus::Failed;
                    entry.task.status_message = Some(message);
                    entry.result = None;
                }
                TaskCompletion::Cancelled => {
                    entry.task.status = TaskStatus::Cancelled;
                    entry.task.status_message = Some("Task cancelled".to_owned());
                    entry.result = None;
                }
            }
            true
        };
        if should_drop {
            drop(tasks);
        }
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

fn unknown_task_error(task_id: &str) -> ErrorData {
    ErrorData::invalid_params(format!("unknown task id: {task_id}"), None)
}

fn cancelled_mutation_error() -> ErrorData {
    ErrorData::new(
        ErrorCode::INVALID_REQUEST,
        "mutation request cancelled before execution started",
        None,
    )
}

const fn task_status_label(status: &TaskStatus) -> &'static str {
    match status {
        TaskStatus::Working => "working",
        TaskStatus::InputRequired => "input_required",
        TaskStatus::Completed => "completed",
        TaskStatus::Failed => "failed",
        TaskStatus::Cancelled => "cancelled",
    }
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
            RequestId::String(value) => Self::String(value.to_string()),
        }
    }
}

fn lock_std_mutex<T>(mutex: &StdMutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
