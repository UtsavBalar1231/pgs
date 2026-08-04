# MCP Server

## Scope

`pgs-mcp` is a local MCP server for stdio only.

- transport: local `stdio`
- capabilities: exactly `{"tools":{}}`
- prompts: unsupported
- resources: unsupported
- logging: unsupported
- tasks: unsupported
- remote HTTP transports: unsupported
- OAuth: unsupported
- registry publishing/discovery: unsupported

## Protocol Version

The server speaks MCP `2026-07-28` and nothing else. Clients on an older
revision are not supported. `server/discover` advertises exactly one version:

```json
"supportedVersions": ["2026-07-28"]
```

Two refusal paths, both observable on the wire:

- A request that declares a pre-`2026-07-28` version in its own `_meta` is
  rejected with `-32022` (`Unsupported protocol version`); `error.data.supported`
  lists the one version the server serves.
- An `initialize` that asks for an older revision is answered with
  `protocolVersion: "2026-07-28"`, not the requested one. Per the MCP lifecycle
  the client must then disconnect; it is never served the older contract.

What this means in practice:

- `initialize` still works. It is the legacy handshake, removed from the
  `2026-07-28` spec but retained here because it is what the stdio session
  negotiates on. It only ever negotiates `2026-07-28`.
- `server/discover` is the modern replacement and is answered once the session
  is live. It returns `supportedVersions`, `capabilities`, `instructions`,
  `ttlMs`, and `cacheScope`.
- Under `2026-07-28`, `protocolVersion` and `clientCapabilities` move into
  per-request `_meta` under the keys
  `io.modelcontextprotocol/protocolVersion` and
  `io.modelcontextprotocol/clientCapabilities`. A `server/discover` call that
  omits them is rejected with `-32602`.
- The server sends `instructions` describing the scan → stage → commit workflow,
  hunk-id staleness, and the drift guard. Clients that surface server
  instructions get that guidance for free.

## Supported Tools

All tools require `repo_path`.

| Tool | Purpose | Mutates repo |
|------|---------|--------------|
| `pgs_scan` | Show unstaged changes | no |
| `pgs_status` | Show staged changes | no |
| `pgs_stage` | Stage selected changes, or preview with `dry_run` + `explain` | yes unless dry-run |
| `pgs_unstage` | Remove selected changes from index | yes |
| `pgs_commit` | Create a git commit, or amend `HEAD` with `amend: true` | yes |
| `pgs_log` | Show recent commit history | no |
| `pgs_overview` | Show unstaged and staged changes together | no |
| `pgs_split_hunk` | Classify contiguous line runs in a hunk | no |
| `pgs_plan_check` | Validate a commit plan against a fresh scan | no |
| `pgs_plan_diff` | Reconcile a saved commit plan against a fresh scan | no |

`repo_path` is canonicalized internally, so the worktree path and its `.git` path map to the same mutation lane.

## Result Envelope Fields

`2026-07-28` requires two additions that `pgs-mcp` sets explicitly:

- every result carries `resultType`, which is always `"complete"` — no tool
  result is ever partial or deferred.
- `tools/list` carries `ttlMs: 3600000` and `cacheScope: "public"`. The tool set
  is frozen at compile time and carries no per-user data, so any client or
  intermediary may cache it for an hour and share that cache across
  authorization contexts.

Tool payloads are unchanged: read the command envelope from
`structuredContent.pgs`.

## Safety Notes

- `pgs_stage`, `pgs_unstage`, and `pgs_commit` change repository state unless `dry_run` is true
- approve mutating tool use explicitly in any agent or automation policy before enabling them
- same-repo mutating requests are serialized by canonical repo path
- cancellation is only honored before a mutating request starts its atomic section
- once a mutation starts, it is allowed to finish or roll back on the existing command path

## Launch

Run the server locally from another project with stdio:

```bash
cargo run --bin pgs-mcp
```

Install the bundled plugin in Codex:

```bash
codex plugin marketplace add UtsavBalar1231/pgs
codex plugin add pgs@pgs-marketplace
```

Or add the stdio server manually in Codex without the plugin:

```bash
codex mcp add pgs -- /path/to/pgs-mcp
```

Example JSON-RPC session from another project:

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2026-07-28","capabilities":{},"clientInfo":{"name":"other-project","version":"0.1.0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"pgs_scan","arguments":{"repo_path":"/path/to/other/project"}}}
```

Example `server/discover` call, which needs the `_meta` keys:

```json
{"jsonrpc":"2.0","id":3,"method":"server/discover","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}}}
```

Example mutating call:

```json
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"pgs_stage","arguments":{"repo_path":"/path/to/other/project","selections":["src/main.rs"]}}}
```

Example exact preview call without mutation:

```json
{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"pgs_stage","arguments":{"repo_path":"/path/to/other/project","selections":["src/main.rs:10-20"],"dry_run":true,"explain":true,"limit":200}}}
```

Example amend call:

```json
{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"pgs_commit","arguments":{"repo_path":"/path/to/other/project","message":"feat: update subject\n\nExplain the rewritten commit.","amend":true}}}
```

## Notes For Integrators

- keep stdout reserved for JSON-RPC messages only
- pass an explicit `repo_path` on every tool call
- treat `pgs_stage`, `pgs_unstage`, and `pgs_commit` as destructive operations
- send the `_meta` protocol keys on every request when negotiating `2026-07-28`
- `tasks/*` methods return `-32601`; there is no task lifecycle to poll, so read
  every tool result from the `tools/call` response directly
- do not assume prompts, resources, logging, HTTP, OAuth, or registry support
