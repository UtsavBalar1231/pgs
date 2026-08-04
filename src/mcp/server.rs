use std::borrow::Cow;
use std::error::Error;
use std::sync::Arc;

use serde::Deserialize;
use serde::de::{DeserializeOwned, IntoDeserializer};
use serde_json::Value;

use rmcp::{
    ErrorData, ServerHandler,
    model::{
        CacheScope, CallToolRequestParams, CallToolResponse, ClientRequest, Implementation,
        ListToolsResult, PaginatedRequestParams, ProtocolVersion, ServerCapabilities, ServerInfo,
        Tool,
    },
    serve_server,
    service::{RequestContext, RoleServer, RxJsonRpcMessage, TxJsonRpcMessage},
    transport::{Transport, async_rw::AsyncRwTransport, stdio},
};

use crate::{
    cmd::mcp_adapter::McpCommandRequest,
    mcp::contract::{
        self, CommitToolInput, LogToolInput, OverviewToolInput, PGS_COMMIT_TOOL, PGS_LOG_TOOL,
        PGS_OVERVIEW_TOOL, PGS_PLAN_CHECK_TOOL, PGS_PLAN_DIFF_TOOL, PGS_SCAN_TOOL,
        PGS_SPLIT_HUNK_TOOL, PGS_STAGE_TOOL, PGS_STATUS_TOOL, PGS_UNSTAGE_TOOL, PlanCheckToolInput,
        PlanDiffToolInput, ScanToolInput, SplitHunkToolInput, StageToolInput, StatusToolInput,
        UnstageToolInput,
    },
    mcp::runtime::PgsMcpRuntime,
};

/// Cache lifetime advertised for `tools/list`, in milliseconds.
const TOOL_LIST_TTL_MS: u64 = 3_600_000;

const SERVER_INSTRUCTIONS: &str = "\
pgs stages git changes at file, hunk, and line granularity without a TTY.

Every tool requires an explicit `repo_path`; nothing is inferred from a working directory.

Workflow: call pgs_scan (unstaged changes) or pgs_overview (unstaged plus staged) first, then \
pgs_stage with the narrowest selector that covers the intent, then pgs_commit. Selectors are \
positional and auto-detected: `src/main.rs` (whole file), `abc123def456` (12-hex hunk id), \
`src/main.rs:10-20` (line range).

Hunk ids are content-addressed and go stale as soon as the file changes. Never reuse an id from \
an earlier session or from before an edit; re-run pgs_scan and take the fresh id. Use \
pgs_plan_diff to reconcile a saved plan against the current tree.

Line-range staging is not drift-safe on its own. Pass `expected_checksums` (map of path → \
file-level SHA-256 from a prior scan) so a workdir edit between scan and stage fails with \
StaleScan instead of staging unintended content.";

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
    ) -> Result<CallToolResponse, ErrorData> {
        let parsed_call = match parse_call(request) {
            Ok(parsed_call) => parsed_call,
            Err(error) => {
                self.runtime.discard_preregistered_mutation(&context.id);
                return Err(error);
            }
        };

        let result = match parsed_call {
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
        };

        result.map(Into::into)
    }

    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        // Narrows rmcp's default, which is every revision the SDK knows
        // (`ProtocolVersion::KNOWN_VERSIONS`). Without this override the server
        // would silently negotiate pre-2026-07-28 clients. Do not delete it.
        Cow::Borrowed(&[ProtocolVersion::V_2026_07_28])
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> + Send + '_ {
        // The tool set is frozen at compile time and carries no per-user data,
        // so any client or intermediary may cache it. 2026-07-28 requires both
        // fields on tools/list and rmcp leaves them unset by default.
        std::future::ready(Ok(ListToolsResult::with_all_items(
            contract::tool_definitions(),
        )
        .with_ttl_ms(TOOL_LIST_TTL_MS)
        .with_cache_scope(CacheScope::Public)))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        contract::tool_definition(name)
    }

    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_protocol_version(ProtocolVersion::V_2026_07_28)
            .with_server_info(Implementation::new("pgs-mcp", env!("CARGO_PKG_VERSION")))
            .with_instructions(SERVER_INSTRUCTIONS)
    }
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
            check_commit_message_source(&input)
                .map_err(|reason| ErrorData::invalid_params(reason, None))?;

            let repo_path = input.repo_path.clone();
            Ok(ParsedToolCall::Mutating {
                tool_name: PGS_COMMIT_TOOL,
                repo_path,
                command: McpCommandRequest::Commit(input.into()),
            })
        }
        PGS_LOG_TOOL => {
            let input: LogToolInput = parse_tool_input(request.arguments)?;
            Ok(ParsedToolCall::Read {
                tool_name: PGS_LOG_TOOL,
                command: McpCommandRequest::Log(input.into()),
            })
        }
        PGS_OVERVIEW_TOOL => {
            let input: OverviewToolInput = parse_tool_input(request.arguments)?;
            Ok(ParsedToolCall::Read {
                tool_name: PGS_OVERVIEW_TOOL,
                command: McpCommandRequest::Overview(input.into()),
            })
        }
        PGS_SPLIT_HUNK_TOOL => {
            let input: SplitHunkToolInput = parse_tool_input(request.arguments)?;
            Ok(ParsedToolCall::Read {
                tool_name: PGS_SPLIT_HUNK_TOOL,
                command: McpCommandRequest::SplitHunk(input.into()),
            })
        }
        PGS_PLAN_CHECK_TOOL => {
            let input: PlanCheckToolInput = parse_tool_input(request.arguments)?;
            Ok(ParsedToolCall::Read {
                tool_name: PGS_PLAN_CHECK_TOOL,
                command: McpCommandRequest::PlanCheck(input.into()),
            })
        }
        PGS_PLAN_DIFF_TOOL => {
            let input: PlanDiffToolInput = parse_tool_input(request.arguments)?;
            Ok(ParsedToolCall::Read {
                tool_name: PGS_PLAN_DIFF_TOOL,
                command: McpCommandRequest::PlanDiff(input.into()),
            })
        }
        _ => Err(ErrorData::invalid_params("tool not found", None)),
    }
}

/// Decide whether a `pgs_commit` request names exactly one usable message source.
///
/// Shared by the protocol-layer parameter check in [`parse_call`] and the
/// transport-layer preregistration hook so the two cannot drift apart.
fn check_commit_message_source(input: &CommitToolInput) -> Result<(), &'static str> {
    if input.message.as_ref().is_some_and(|m| m.trim().is_empty()) {
        return Err("message must be a non-empty string");
    }
    match (input.message.as_deref(), input.message_file.as_deref()) {
        (Some(_), Some(_)) => Err("message and message_file are mutually exclusive"),
        (None, None) => Err("supply exactly one of message or message_file"),
        // An MCP request has no stdin of its own; reading `-` would block
        // the server on a stream that never produces data.
        (None, Some("-")) => Err("message_file `-` (stdin) is not available over MCP"),
        _ => Ok(()),
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
    if let Some(repo_path) = preregistration_repo_path(&call.params) {
        let _ = runtime.preregister_mutation(&request.id, repo_path);
    }
}

fn preregistration_repo_path(params: &CallToolRequestParams) -> Option<&str> {
    let arguments = params.arguments.as_ref()?;
    let repo_path = arguments.get("repo_path")?.as_str()?;

    match params.name.as_ref() {
        PGS_STAGE_TOOL | PGS_UNSTAGE_TOOL => Some(repo_path),
        // Only reserve a mutation lane for a request that can actually reach the
        // commit handler, judged by the same predicate the dispatch path uses.
        PGS_COMMIT_TOOL => {
            let input = CommitToolInput::deserialize(arguments.into_deserializer()).ok()?;
            check_commit_message_source(&input).ok().map(|()| repo_path)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{PGS_COMMIT_TOOL, ParsedToolCall, parse_call, preregistration_repo_path};
    use rmcp::model::CallToolRequestParams;
    use serde_json::{Value, json};

    fn commit_params(arguments: &Value) -> CallToolRequestParams {
        let object = arguments.as_object().expect("object arguments").clone();
        CallToolRequestParams::new(PGS_COMMIT_TOOL).with_arguments(object)
    }

    /// The transport hook reserves a mutation lane only for a request the
    /// dispatch path will accept. If these two disagree, a rejected request
    /// either strands a lane slot or skips ordered admission.
    #[test]
    fn preregistration_and_dispatch_agree_on_commit_message_source() {
        let cases = [
            (
                "message only",
                json!({"repo_path": "/r", "message": "m"}),
                true,
            ),
            (
                "message_file only",
                json!({"repo_path": "/r", "message_file": "/m.txt"}),
                true,
            ),
            (
                "both sources",
                json!({"repo_path": "/r", "message": "m", "message_file": "/m.txt"}),
                false,
            ),
            ("neither source", json!({"repo_path": "/r"}), false),
            (
                "blank message",
                json!({"repo_path": "/r", "message": ""}),
                false,
            ),
            (
                "whitespace-only message",
                json!({"repo_path": "/r", "message": "  \n\t"}),
                false,
            ),
            (
                "message_file stdin sentinel",
                json!({"repo_path": "/r", "message_file": "-"}),
                false,
            ),
            (
                "non-string message",
                json!({"repo_path": "/r", "message": 7}),
                false,
            ),
            (
                "non-string message_file",
                json!({"repo_path": "/r", "message_file": ["/m.txt"]}),
                false,
            ),
        ];

        for (label, arguments, expected) in cases {
            let params = commit_params(&arguments);
            let preregisters = preregistration_repo_path(&params).is_some();
            let dispatches = matches!(
                parse_call(commit_params(&arguments)),
                Ok(ParsedToolCall::Mutating { .. })
            );

            assert_eq!(preregisters, expected, "preregistration for {label}");
            assert_eq!(dispatches, expected, "dispatch for {label}");
        }
    }
}
