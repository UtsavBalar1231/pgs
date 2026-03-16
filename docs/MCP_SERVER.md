# MCP Server

## Scope

`pgs-mcp` is a local MCP v1 server for stdio only.

- transport: local `stdio`
- capabilities: `tools`, `tasks`
- prompts: unsupported
- resources: unsupported
- remote HTTP transports: unsupported
- OAuth: unsupported
- registry publishing/discovery: unsupported

The server baseline is `2025-11-25`.

`pgs-mcp` sets this explicitly because the bundled `rmcp` release still defaults to an older baseline. Clients that initialize with an older version can still negotiate down normally.

## Supported Tools

All tools require `repo_path`.

| Tool | Purpose | Mutates repo | Task mode |
|------|---------|--------------|-----------|
| `pgs_scan` | Show unstaged changes | no | optional |
| `pgs_status` | Show staged changes | no | optional |
| `pgs_stage` | Stage selected changes | yes | forbidden |
| `pgs_unstage` | Remove selected changes from index | yes | forbidden |
| `pgs_commit` | Create a git commit from staged changes | yes | forbidden |

`repo_path` is canonicalized internally, so the worktree path and its `.git` path map to the same mutation lane.

## Task Support Matrix

- `pgs_scan`: direct call or task-based invocation
- `pgs_status`: direct call or task-based invocation
- `pgs_stage`: direct call only
- `pgs_unstage`: direct call only
- `pgs_commit`: direct call only

Task lifecycle support is server-side and limited to read-only tool calls:

- `tasks/list`
- `tasks/get`
- `tasks/result`
- `tasks/cancel`

Mutating tools reject task augmentation by contract.

## Safety Notes

- `pgs_stage`, `pgs_unstage`, and `pgs_commit` change repository state
- approve mutating tool use explicitly in any agent or automation policy before enabling them
- same-repo mutating requests are serialized by canonical repo path
- cancellation is only honored before a mutating request starts its atomic section
- once a mutation starts, it is allowed to finish or roll back on the existing command path

## Launch

Run the server locally from another project with stdio:

```bash
cargo run --bin pgs-mcp
```

Example JSON-RPC session from another project:

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"other-project","version":"0.1.0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"pgs_scan","arguments":{"repo_path":"/path/to/other/project"}}}
```

Example mutating call:

```json
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"pgs_stage","arguments":{"repo_path":"/path/to/other/project","selections":["src/main.rs"]}}}
```

## Notes For Integrators

- keep stdout reserved for JSON-RPC messages only
- pass an explicit `repo_path` on every tool call
- treat `pgs_stage`, `pgs_unstage`, and `pgs_commit` as destructive operations
- do not assume prompts, resources, HTTP, OAuth, or registry support in v1
