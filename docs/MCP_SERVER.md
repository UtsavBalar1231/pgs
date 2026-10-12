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
- distribution: GitHub Releases (prebuilt `pgs-mcp-<target>` binaries) and the
  plugin marketplaces in this repository; the server is not listed in an MCP
  registry

## Protocol Version

`pgs-mcp` serves three MCP revisions and picks one from how the client opens
the session:

| Revision | How a client reaches it |
|----------|-------------------------|
| `2026-07-28` | per-request `_meta`, optionally starting with `server/discover` |
| `2025-11-25` | `initialize` handshake |
| `2025-06-18` | `initialize` handshake |

`server/discover` advertises all three:

```json
"supportedVersions": ["2025-06-18", "2025-11-25", "2026-07-28"]
```

`2026-07-28` removed `initialize`, but as of October 2026 most clients still
open with it: Codex CLI defaults to `2025-06-18`, VS Code Copilot to
`2025-11-25`, and Gemini CLI ships an SDK capped at `2025-06-18`. Claude Code
probes `server/discover` first and uses `2026-07-28`. Serving both shapes is
what keeps one binary usable from all of them.

`2025-06-18` is the floor because it introduced `outputSchema` and
`structuredContent`, which every pgs tool result depends on.

Negotiation, as observable on the wire:

- An `initialize` naming `2025-06-18` or `2025-11-25` is answered with that
  revision.
- An `initialize` naming `2026-07-28` is answered with `2025-11-25`, the newest
  revision that still has an `initialize` handshake.
- An `initialize` naming anything older than the floor is answered with
  `2025-11-25`; per the MCP lifecycle the client must then disconnect if it
  cannot speak it.
- A request whose `_meta` declares an unsupported revision is rejected with
  `-32022` (`Unsupported protocol version`); `error.data.supported` lists the
  three revisions above.
- A `2026-07-28` request must carry `io.modelcontextprotocol/protocolVersion`
  and `io.modelcontextprotocol/clientCapabilities` in `_meta`; one that omits
  them is rejected with `-32602`.

The server sends `instructions` describing the scan → stage → commit workflow,
hunk-id staleness, and the drift guard, under either revision.

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
| `pgs_plan_apply` | Stage and commit each planned commit of a `CommitPlan`, in order | yes unless dry-run |

`repo_path` is canonicalized internally, so the worktree path and its `.git` path map to the same mutation lock.

## Result Envelope Fields

`2026-07-28` requires two additions:

- every result carries `resultType`, which is always `"complete"` — no tool
  result is ever partial or deferred. Sessions opened with `initialize` predate
  the field and do not receive it.
- `tools/list` carries `ttlMs: 3600000` and `cacheScope: "public"`. The tool set
  is frozen at compile time and carries no per-user data, so any client or
  intermediary may cache it for an hour and share that cache across
  authorization contexts.

Tool payloads are unchanged: read the command envelope from
`structuredContent.pgs`.

## Safety Notes

- `pgs_stage`, `pgs_unstage`, and `pgs_commit` change repository state unless `dry_run` is true
- approve mutating tool use explicitly in any agent or automation policy before enabling them
- same-repo mutating requests never run at the same time: each canonical repo path has one
  lock, and waiting requests are served in the order they arrived, so a `pgs_commit`
  pipelined right behind a `pgs_stage` commits what that stage staged
- a mutating request cancelled while it waits for the lock is dropped without touching the
  index, and gets no response
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

Other harnesses, with `pgs-mcp` on your `PATH`:

```bash
claude mcp add --transport stdio --scope user pgs -- pgs-mcp
gemini mcp add -s user pgs pgs-mcp
code --add-mcp '{"name":"pgs","command":"pgs-mcp"}'
```

Cursor installs from
`cursor://anysphere.cursor-deeplink/mcp/install?name=pgs&config=eyJjb21tYW5kIjoicGdzLW1jcCJ9`.
Per-harness setup for these and the rest (Copilot CLI, Kiro, Windsurf, Cline,
Zed, OpenCode, Goose, Amp, Crush, Continue, Junie, Warp, Factory Droid) is in
[INTEGRATIONS.md](INTEGRATIONS.md).

Example `2026-07-28` session. There is no handshake; every request carries
the `_meta` keys:

```json
{"jsonrpc":"2.0","id":1,"method":"server/discover","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"pgs_scan","arguments":{"repo_path":"/path/to/other/project"},"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}}}
```

Example `initialize` session at `2025-11-25`. The remaining examples use this
shape for brevity:

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"other-project","version":"0.1.0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"pgs_scan","arguments":{"repo_path":"/path/to/other/project"}}}
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
- on `2026-07-28`, send the `_meta` protocol keys on every request; on an
  `initialize` session, send nothing extra
- `tasks/*` methods return `-32601`; there is no task lifecycle to poll, so read
  every tool result from the `tools/call` response directly
- do not assume prompts, resources, logging, HTTP, OAuth, or MCP registry
  listing
