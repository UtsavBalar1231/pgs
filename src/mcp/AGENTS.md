# src/mcp/ — MCP Server Module

Model Context Protocol server exposing pgs operations as tools. Separate async binary.

## Architecture

```
MCP Client (stdio)
  -> server.rs (PgsMcpServer, rmcp ServerHandler)
    -> contract.rs (deserialize input, build Tool schemas)
      -> runtime.rs (PgsMcpRuntime, per-repo mutation ordering)
        -> cmd/mcp_adapter.rs (bridge to CLI command handlers)
          -> cmd/scan.rs, cmd/stage.rs, etc. (actual execution)
```

The MCP server reuses ALL CLI command logic. It never reimplements git, selection, or output operations.

## Entry Point

`src/bin/pgs-mcp.rs` — `#[tokio::main]` async binary, calls `server::run_stdio()`.

`mod.rs` holds `PROTOCOL_VERSION` (the wire string `"2026-07-28"`, for test
assertions) and `SUPPORTED_PROTOCOL_VERSIONS` (`2025-06-18`, `2025-11-25`,
`2026-07-28`), which `supported_protocol_versions()` returns. A `2026-07-28`
client sends per-request `_meta` (optionally starting with `server/discover`);
an older client opens with `initialize` and rmcp negotiates the revision it
names, or `2025-11-25` when it names `2026-07-28`. `2025-06-18` is the floor:
it introduced the `structuredContent`/`outputSchema` every tool result relies
on. Most clients still use `initialize` (Codex, VS Code Copilot, Gemini CLI),
so do not narrow the list to the modern revision. Advertised capabilities are
exactly `{"tools":{}}`.

rmcp serves a whole `2026-07-28` session before the `initialize` it waits for,
so `run_stdio` treats `ServerInitializeError::ConnectionClosed` as a normal
end of session (exit 0).

## Module Roles

### `contract.rs` (~1275 lines)

- Ten tool-name constants: `PGS_SCAN_TOOL`, `PGS_STATUS_TOOL`, `PGS_STAGE_TOOL`,
  `PGS_UNSTAGE_TOOL`, `PGS_COMMIT_TOOL`, `PGS_LOG_TOOL`, `PGS_OVERVIEW_TOOL`,
  `PGS_SPLIT_HUNK_TOOL`, `PGS_PLAN_CHECK_TOOL`, `PGS_PLAN_DIFF_TOOL`
- One `*ToolInput` struct per tool, each deriving `JsonSchema` (from
  `rmcp::schemars`) for MCP auto-discovery
- `From<ToolInput> for Mcp*Request` conversions applying defaults (e.g. `context`
  defaults to 3)
- `tool_definitions()` / `tool_definition(name)` — the tool set is frozen and
  built once into the `TOOL_DEFINITIONS` `LazyLock` rather than rebuilt per
  `list_tools` call
- `sanitize_tool_schemas` / `strip_nonstandard_integer_formats` — drops
  nonstandard integer `format` annotations such as `uint32` that trip client
  schema validators
- `map_execution_result()` — converts the adapter's `CommandOutput` into
  `CallToolResult`, with `ToolOutcome` and `PgsToolError` carrying the
  structured failure payload. The text summary of a dry-run stage/unstage says
  the index is unchanged
- `ToolAnnotations` per tool: read-only tools are
  `read_only(true).destructive(false).idempotent(true).open_world(false)`;
  `pgs_stage`, `pgs_unstage`, and `pgs_commit` are
  `read_only(false).destructive(true).idempotent(false).open_world(false)`

### `runtime.rs` (~210 lines)

- `PgsMcpRuntime` — per-repo mutation coordinator: a map from canonical worktree
  path to `Arc<tokio::sync::Mutex<()>>`. There is no task registry; MCP task
  support was removed when the server moved to `2026-07-28`, where tasks live in
  a separate draft extension.
- Guarantee: stage/unstage/commit on one repo never run at the same time, and
  waiters are served FIFO in the order their handler called `lock()` (tokio's
  `Mutex` is fair). That is not wire-arrival order: rmcp spawns one task per
  request, so two requests sent back to back can reach the lock in either
  order. A client that needs stage-then-commit ordering must wait for the
  stage response before sending the commit.
- `execute_command()` — read path; runs the adapter on `spawn_blocking`.
- `execute_mutation()` — write path; takes the repo lock, honors cancellation
  before the atomic section starts, then runs `execute_command()` while holding
  the guard. A `repo_path` that resolves to no repository skips the lock and
  reports through `execute_command()`, so it gets the same structured
  `pgs_error` as a read.
- `lock_unless_cancelled()` — waits for the lock or the cancellation future,
  polling cancellation first. A request cancelled while queued drops its waiter
  and never touches the index. Takes the cancel signal as a plain future
  because `CancellationToken` lives in `tokio-util`, which pgs does not depend
  on directly; this is also what lets the unit tests drive it.
- Nothing is reserved before `call_tool` runs, so a request rmcp rejects early
  (for example an unsupported `_meta` protocol version) leaves no state behind.
- `canonical_repo_path()` — resolves the worktree and canonicalizes it, so the
  worktree path and its `.git` path share one lock.

### `server.rs` (~300 lines)

- `PgsMcpServer` — implements `rmcp::ServerHandler`
- `call_tool` — `parse_call()` classifies each request as
  `ParsedToolCall::Read` or `ParsedToolCall::Mutating`, then routes to the
  matching runtime path
- `list_tools` — returns all ten tools with `ttlMs: 3_600_000` and
  `cacheScope: Public` (rmcp would default a missing hint to `0` / `private`,
  and Claude Code rejects a `tools/list` without them)
- `get_tool` — single-tool lookup by name
- `get_info` — returns a `ServerConfig` (rmcp 3.4's name for `ServerInfo`):
  capabilities (`tools` only), implementation name/version, and
  `SERVER_INSTRUCTIONS` (workflow, hunk-id staleness, and the
  `expected_checksums` drift guard)
- Transport: stdio via `rmcp::transport::stdio` + `AsyncRwTransport`, unwrapped

## Adapter Pattern (cmd/mcp_adapter.rs)

Lives in `src/cmd/` not `src/mcp/` — bridges MCP to CLI:

| Type | Purpose |
|------|---------|
| `McpScanRequest`, `McpStatusRequest`, etc. | Typed request structs (all fields resolved, no Options); no `JsonSchema`, schemas come from the `*ToolInput` types |
| `McpCommandRequest` | Enum routing to the right command handler |
| `CommandOutput` | Re-export of `output::view::CommandOutput`, the result `execute` returns |
| `McpAdapterError` | Wraps `PgsError` with command context; `code()`/`exit_code()` read the source error |
| `execute(request)` | Dispatches `McpCommandRequest` to CLI handlers |

## Key Dependency

`rmcp` crate v3.5 with features `server` + `transport-io`. Provides:
- `ServerHandler` trait (async tool dispatch)
- `schemars::JsonSchema` derive for auto-generating tool input schemas
- `CallToolResult`, `Tool`, `ToolAnnotations`, `CacheScope` protocol types
- `server/discover`, `resultType`, and the `_meta` protocol-version plumbing
  required by `2026-07-28`
- stdio transport for the `pgs-mcp` binary

## Adding a New MCP Tool

1. Add tool constant in `contract.rs` (`pub const PGS_NEW_TOOL: &str = "pgs_new"`)
2. Add `NewToolInput` struct with `JsonSchema` derive and its
   `From<NewToolInput> for McpNewRequest` conversion
3. Add `McpNewRequest` in `cmd/mcp_adapter.rs`
4. Add variant to `McpCommandRequest` enum
5. Add execution branch in `mcp_adapter::execute()`
6. Add a `new_tool()` builder in `contract.rs` with the right `ToolAnnotations`,
   and register it in `TOOL_DEFINITIONS`
7. In `server.rs`, add the tool to `list_tools()`, `get_tool()`, and a
   `parse_call()` arm choosing `ParsedToolCall::Read` or `::Mutating`
8. Add test file: `tests/test_mcp_new_tool.rs`
