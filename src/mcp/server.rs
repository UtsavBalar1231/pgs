use std::error::Error;
use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde_json::Value;

use rmcp::{
    ErrorData, ServerHandler,
    model::{
        CallToolRequestParams, CallToolResult, ClientRequest, CreateTaskResult, GetTaskInfoParams,
        GetTaskPayloadResult, GetTaskResult, Implementation, ListTasksResult, ListToolsResult,
        PaginatedRequestParams, ProtocolVersion, ServerCapabilities, ServerInfo, TasksCapability,
        Tool,
    },
    serve_server,
    service::{RequestContext, RoleServer, RxJsonRpcMessage, TxJsonRpcMessage},
    transport::{Transport, async_rw::AsyncRwTransport, stdio},
};

use crate::{
    cmd::mcp_adapter::McpCommandRequest,
    mcp::contract::{
        self, CommitToolInput, PGS_COMMIT_TOOL, PGS_SCAN_TOOL, PGS_STAGE_TOOL, PGS_STATUS_TOOL,
        PGS_UNSTAGE_TOOL, ScanToolInput, StageToolInput, StatusToolInput, UnstageToolInput,
    },
    mcp::runtime::PgsMcpRuntime,
};

/// MCP server bootstrap for local stdio transport.
#[derive(Debug, Clone, Default)]
pub struct PgsMcpServer {
    runtime: Arc<PgsMcpRuntime>,
}

impl ServerHandler for PgsMcpServer {
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let parsed_call = match parse_call(request) {
            Ok(parsed_call) => parsed_call,
            Err(error) => {
                self.runtime.discard_preregistered_mutation(&context.id);
                return Err(error);
            }
        };

        match parsed_call {
            ParsedToolCall::Read { tool_name, command } => {
                self.runtime.execute_command(tool_name, command).await
            }
            ParsedToolCall::Mutating {
                tool_name,
                repo_path,
                command,
            } => {
                self.runtime
                    .execute_mutation(tool_name, &repo_path, command, context)
                    .await
            }
        }
    }

    async fn enqueue_task(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CreateTaskResult, ErrorData> {
        match parse_call(request)? {
            ParsedToolCall::Read { tool_name, command } => {
                self.runtime.enqueue_read_task(tool_name, command).await
            }
            ParsedToolCall::Mutating { .. } => Err(ErrorData::invalid_params(
                "Tool does not support task-based invocation",
                None,
            )),
        }
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> + Send + '_ {
        std::future::ready(Ok(ListToolsResult::with_all_items(vec![
            scan_tool(),
            status_tool(),
            stage_tool(),
            unstage_tool(),
            commit_tool(),
        ])))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        match name {
            PGS_SCAN_TOOL => Some(scan_tool()),
            PGS_STATUS_TOOL => Some(status_tool()),
            PGS_STAGE_TOOL => Some(stage_tool()),
            PGS_UNSTAGE_TOOL => Some(unstage_tool()),
            PGS_COMMIT_TOOL => Some(commit_tool()),
            _ => None,
        }
    }

    async fn list_tasks(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListTasksResult, ErrorData> {
        Ok(self.runtime.list_tasks().await)
    }

    async fn get_task_info(
        &self,
        request: GetTaskInfoParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, ErrorData> {
        self.runtime.get_task_info(&request.task_id).await
    }

    async fn get_task_result(
        &self,
        request: rmcp::model::GetTaskResultParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetTaskPayloadResult, ErrorData> {
        self.runtime.get_task_result(&request.task_id).await
    }

    async fn cancel_task(
        &self,
        request: rmcp::model::CancelTaskParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::CancelTaskResult, ErrorData> {
        self.runtime.cancel_task(&request.task_id).await
    }

    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_tasks_with(TasksCapability::server_default())
                .build(),
        )
        .with_protocol_version(protocol_version_baseline())
        .with_server_info(Implementation::new("pgs-mcp", env!("CARGO_PKG_VERSION")))
    }
}

fn protocol_version_baseline() -> ProtocolVersion {
    serde_json::from_value(Value::String(
        crate::mcp::PROTOCOL_VERSION_BASELINE.to_owned(),
    ))
    .expect("MCP protocol baseline must be a valid protocol version")
}

enum ParsedToolCall {
    Read {
        tool_name: &'static str,
        command: McpCommandRequest,
    },
    Mutating {
        tool_name: &'static str,
        repo_path: String,
        command: McpCommandRequest,
    },
}

fn parse_call(request: CallToolRequestParams) -> Result<ParsedToolCall, ErrorData> {
    match request.name.as_ref() {
        PGS_SCAN_TOOL => {
            let input: ScanToolInput = parse_tool_input(request.arguments)?;
            Ok(ParsedToolCall::Read {
                tool_name: PGS_SCAN_TOOL,
                command: McpCommandRequest::Scan(input.into()),
            })
        }
        PGS_STATUS_TOOL => {
            let input: StatusToolInput = parse_tool_input(request.arguments)?;
            Ok(ParsedToolCall::Read {
                tool_name: PGS_STATUS_TOOL,
                command: McpCommandRequest::Status(input.into()),
            })
        }
        PGS_STAGE_TOOL => {
            let input: StageToolInput = parse_tool_input(request.arguments)?;
            let repo_path = input.repo_path.clone();
            Ok(ParsedToolCall::Mutating {
                tool_name: PGS_STAGE_TOOL,
                repo_path,
                command: McpCommandRequest::Stage(input.into()),
            })
        }
        PGS_UNSTAGE_TOOL => {
            let input: UnstageToolInput = parse_tool_input(request.arguments)?;
            let repo_path = input.repo_path.clone();
            Ok(ParsedToolCall::Mutating {
                tool_name: PGS_UNSTAGE_TOOL,
                repo_path,
                command: McpCommandRequest::Unstage(input.into()),
            })
        }
        PGS_COMMIT_TOOL => {
            let input: CommitToolInput = parse_tool_input(request.arguments)?;
            if input.message.trim().is_empty() {
                return Err(ErrorData::invalid_params(
                    "message must be a non-empty string",
                    None,
                ));
            }

            let repo_path = input.repo_path.clone();
            Ok(ParsedToolCall::Mutating {
                tool_name: PGS_COMMIT_TOOL,
                repo_path,
                command: McpCommandRequest::Commit(input.into()),
            })
        }
        _ => Err(ErrorData::invalid_params("tool not found", None)),
    }
}

fn parse_tool_input<T>(arguments: Option<rmcp::model::JsonObject>) -> Result<T, ErrorData>
where
    T: DeserializeOwned,
{
    let value = Value::Object(arguments.unwrap_or_default());
    serde_json::from_value(value).map_err(|error| {
        ErrorData::invalid_params(format!("failed to parse parameters: {error}"), None)
    })
}

fn scan_tool() -> Tool {
    contract::tool_definition(PGS_SCAN_TOOL)
        .expect("scan tool must be present in frozen MCP contract")
}

fn status_tool() -> Tool {
    contract::tool_definition(PGS_STATUS_TOOL)
        .expect("status tool must be present in frozen MCP contract")
}

fn stage_tool() -> Tool {
    contract::tool_definition(PGS_STAGE_TOOL)
        .expect("stage tool must be present in frozen MCP contract")
}

fn unstage_tool() -> Tool {
    contract::tool_definition(PGS_UNSTAGE_TOOL)
        .expect("unstage tool must be present in frozen MCP contract")
}

fn commit_tool() -> Tool {
    contract::tool_definition(PGS_COMMIT_TOOL)
        .expect("commit tool must be present in frozen MCP contract")
}

/// Start the `pgs-mcp` server over stdio and wait for shutdown.
///
/// # Errors
///
/// Returns an error if stdio transport setup fails, protocol initialization fails,
/// or the server loop exits with a transport/runtime error.
pub async fn run_stdio() -> Result<(), Box<dyn Error + Send + Sync>> {
    let runtime = Arc::new(PgsMcpRuntime::default());
    let (stdin, stdout) = stdio();
    let transport = RegistrationTransport {
        inner: AsyncRwTransport::new_server(stdin, stdout),
        runtime: Arc::clone(&runtime),
    };
    let server = serve_server(PgsMcpServer { runtime }, transport).await?;
    server.waiting().await?;

    Ok(())
}

#[derive(Debug)]
struct RegistrationTransport<T> {
    inner: T,
    runtime: Arc<PgsMcpRuntime>,
}

impl<T> Transport<RoleServer> for RegistrationTransport<T>
where
    T: Transport<RoleServer, Error = std::io::Error>,
{
    type Error = std::io::Error;

    fn send(
        &mut self,
        item: TxJsonRpcMessage<RoleServer>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static {
        self.inner.send(item)
    }

    fn receive(&mut self) -> impl Future<Output = Option<RxJsonRpcMessage<RoleServer>>> + Send {
        let runtime = Arc::clone(&self.runtime);
        let receive_future = self.inner.receive();
        async move {
            let message = receive_future.await;
            if let Some(ref message) = message {
                preregister_mutation_if_needed(&runtime, message);
            }
            message
        }
    }

    fn close(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.inner.close()
    }
}

fn preregister_mutation_if_needed(runtime: &PgsMcpRuntime, message: &RxJsonRpcMessage<RoleServer>) {
    let rmcp::model::JsonRpcMessage::Request(request) = message else {
        return;
    };
    let ClientRequest::CallToolRequest(call) = &request.request else {
        return;
    };
    if call.params.task.is_some() {
        return;
    }

    if let Some(repo_path) = preregistration_repo_path(&call.params) {
        let _ = runtime.preregister_mutation(&request.id, &repo_path);
    }
}

fn preregistration_repo_path(params: &CallToolRequestParams) -> Option<String> {
    let arguments = params.arguments.as_ref()?;
    let repo_path = arguments.get("repo_path")?.as_str()?.to_owned();

    match params.name.as_ref() {
        PGS_STAGE_TOOL | PGS_UNSTAGE_TOOL => Some(repo_path),
        PGS_COMMIT_TOOL => {
            let message = arguments.get("message")?.as_str()?;
            if message.trim().is_empty() {
                None
            } else {
                Some(repo_path)
            }
        }
        _ => None,
    }
}
