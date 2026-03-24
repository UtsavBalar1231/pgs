# pgs

Non-interactive git staging at file, hunk, and line granularity.

`git add -p` requires a TTY. `pgs` doesn't.

## Why

- `git add -p` is interactive — AI agents and scripts have no TTY
- Manual patch construction via `git apply --cached` is fragile — one off-by-one line number and the patch fails
- `git diff` output is unstructured — no stable way to reference a specific hunk across commands

`pgs` provides content-addressed hunk IDs (SHA-256), atomic staging with automatic backup/restore, and structured output parseable by both humans and machines.

## Quick Start

```bash
pgs scan                              # list unstaged changes
pgs scan src/main.rs --full           # line-level diff for one file
pgs stage src/main.rs                 # stage entire file
pgs stage abc123def456                # stage specific hunk by ID
pgs stage src/main.rs:10-20           # stage line range (1-indexed, inclusive)
pgs stage src/main.rs --dry-run       # validate without modifying index
pgs unstage src/main.rs               # remove file from index
pgs status                            # show staged changes (HEAD vs index)
pgs commit -m "feat: add feature"     # commit
```

## Selection Syntax

Positional arguments are auto-detected:

| Pattern | Example | Meaning |
|---------|---------|---------|
| File path | `src/main.rs` | Entire file |
| Hunk ID | `abc123def456` | 12-hex content-addressed ID from scan |
| Line range | `src/main.rs:10-20,30-40` | 1-indexed, inclusive |

`--exclude` uses the same syntax: `pgs stage src/main.rs --exclude abc123def456`

## Output

Default: structured text markers — `@@pgs:v1 <kind> <json>`.
JSON: opt-in via `--json` or `--output json`.

See `docs/CLI_SPEC.md` for the full output contract.

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | No effect (nothing to stage, empty selection) |
| 2 | User error (bad syntax, binary file constraint) |
| 3 | Conflict — re-scan and retry (stale scan, locked index) |
| 4 | Internal error |

## Claude Code Plugin

Install `pgs` as a Claude Code plugin for automatic MCP tool integration and the `git-commit-staging` skill:

```bash
# Add the marketplace
/plugin marketplace add UtsavBalar1231/pgs

# Install the plugin
/plugin install pgs@pgs-marketplace
```

Or test locally during development:

```bash
claude --plugin-dir /path/to/pgs
```

**What you get:**
- **5 MCP tools**: `pgs_scan`, `pgs_stage`, `pgs_unstage`, `pgs_status`, `pgs_commit` — available automatically via the bundled MCP server
- **git-commit-staging skill**: teaches Claude the scan → plan → stage → commit workflow with hunk-level precision
- **Auto-install**: the plugin automatically downloads the correct prebuilt binary for your platform on first session

**Supported platforms:** macOS (Intel + Apple Silicon), Linux (x86_64 + ARM64). Windows binaries are available for standalone use via `claude mcp add`.

## MCP Server

`pgs` also ships `pgs-mcp`, a local stdio MCP server for the same scan/status/stage/unstage/commit workflow.

```bash
cargo run --bin pgs-mcp
```

Or add it manually to Claude Code without the plugin:

```bash
claude mcp add --transport stdio pgs -- /path/to/pgs-mcp
```

MCP tool calls require an explicit `repo_path`. For full MCP usage, task support, and safety notes, see `docs/MCP_SERVER.md`.

## Build

```bash
cargo build                        # compile
cargo test                         # all tests
cargo clippy -- -D warnings        # lint (zero warnings)
cargo fmt --check                  # format check
```

Requires Rust 1.85+ and a C compiler (for libgit2).

See `docs/CLI_SPEC.md` for the complete output contract and `docs/ARCHITECTURE.md` for system design.
