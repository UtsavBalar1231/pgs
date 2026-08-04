# pgs

Non-interactive git staging at file, hunk, and line granularity — built for AI
agents and scripts.

`git add -p` needs a TTY. `pgs` doesn't.

## Why

- **`git add -p` is interactive** — AI agents and scripts have no TTY.
- **Hand-built patches are fragile** — one off-by-one line number and
  `git apply --cached` rejects the whole patch.
- **`git diff` is unstructured** — no stable way to name a specific hunk across
  commands.

`pgs` gives every change a content-addressed hunk ID (SHA-256), stages
atomically with automatic backup/restore, and emits structured output that both
humans and machines can parse. It ships a CLI (`pgs`) and a stdio MCP server
(`pgs-mcp`) over the same engine.

## Quick start

```bash
# Inspect
pgs scan                       # unstaged changes + hunk IDs (Index → Workdir)
pgs scan src/main.rs --full    # line-level diff for one file
pgs status                     # staged changes (HEAD → Index)
pgs overview                   # unstaged and staged changes in one view

# Stage with the narrowest honest selector
pgs stage src/main.rs                       # whole file
pgs stage abc123def456                      # one hunk, by content-addressed ID
pgs stage src/main.rs:10-20                 # a line range (1-indexed, inclusive)
pgs stage src/main.rs --dry-run --explain   # preview exact staged lines, no mutation
pgs unstage src/main.rs                     # remove from the index

# Commit
pgs commit -m "feat: add feature"           # commit staged changes
pgs commit --amend -m "feat: reword"        # rewrite HEAD with current index/message
pgs log                                     # recent history (for message style)
```

## Splitting and planning

```bash
pgs split-hunk abc123def456            # classify addition/deletion/mixed runs in a hunk
pgs plan-check --stdin < plan.json     # validate a multi-commit plan against a fresh scan
pgs plan-diff  --stdin < plan.json     # reconcile a saved plan after edits or commits
```

A plan looks like
`{"commits": [{"id": "c1", "selections": ["src/a.rs:abc123def456"], "message": "..."}]}`.
`plan-check` and `plan-diff` are descriptive — they report overlaps, gaps, and
drift but never stage or commit.

## Selection syntax

Positional arguments are auto-detected:

| Pattern | Example | Meaning |
|---------|---------|---------|
| File path | `src/main.rs` | Entire file |
| Directory | `src/` | All files under a directory |
| Hunk ID | `abc123def456` | 12-hex content-addressed ID from `scan` |
| Line range | `src/main.rs:10-20,30-40` | 1-indexed, inclusive |

`--exclude` uses the same syntax: `pgs stage src/main.rs --exclude abc123def456`.
If a real file path is exactly 12 hex characters, prefix it with `./` so it is
read as a path, not a hunk ID.

Hunk IDs are stable only while a hunk's content and position are unchanged.
Re-scan after any edit, commit, or index change before reusing them.

## Guard against drift

`scan` reports a per-file `checksum`. Pass it back to `stage` to refuse staging
if the file changed between the scan and the stage:

```bash
pgs stage src/main.rs:10-20 --expect "src/main.rs=<checksum from scan>"
```

A stale checksum fails with exit code 3 (`stale_scan`) and leaves the index
untouched — re-scan and retry. Partial (hunk/line) staging also refuses non-UTF-8
files and files whose line endings differ from the index; stage those whole-file.

## Output

Default: structured text markers — `@@pgs:v1 <kind> <json>`.
JSON: opt-in via `--json` or `--output json`.

See `docs/CLI_SPEC.md` for the full output contract.

## Exit codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | No effect (nothing to stage, empty selection) |
| 2 | User error (bad selector; binary, non-UTF-8, or CRLF partial-stage constraint) |
| 3 | Conflict — re-scan and retry (stale scan, locked index) |
| 4 | Internal error |

## Claude Code plugin

Install `pgs` as a Claude Code plugin for automatic MCP tool integration and the
`git-commit-staging` skill:

```bash
# Add the marketplace
/plugin marketplace add UtsavBalar1231/pgs

# Install the plugin
/plugin install pgs@pgs-marketplace
```

Or test locally during development:

```bash
claude --plugin-dir /path/to/pgs/plugins/pgs
```

**What you get:**
- **10 MCP tools**: `pgs_scan`, `pgs_status`, `pgs_stage`, `pgs_unstage`,
  `pgs_commit`, `pgs_log`, `pgs_overview`, `pgs_split_hunk`, `pgs_plan_check`,
  and `pgs_plan_diff` — available automatically via the bundled MCP server.
- **git-commit-staging skill**: teaches agents the scan → plan → stage → commit
  workflow with hunk-level precision.
- **Auto-install**: the plugin downloads the correct prebuilt binary for your
  platform before launching the MCP server.

**Supported platforms:** macOS (Intel + Apple Silicon), Linux (x86_64 + ARM64).
Windows binaries are available for standalone use via `claude mcp add`.

## Codex plugin

Install `pgs` as a Codex plugin for the same MCP tools and skill:

```bash
codex plugin marketplace add UtsavBalar1231/pgs
codex plugin add pgs@pgs-marketplace
```

Or test locally during development:

```bash
codex plugin marketplace add /path/to/pgs
codex plugin add pgs@pgs-marketplace
```

The Codex manifest lives in `.codex-plugin/plugin.json` and is mirrored into
`plugins/pgs/` for marketplace installation. The canonical skill source is
`plugins/pgs/skills/git-commit-staging/SKILL.md`; the top-level `skills/` path is
a repo-local symlink for root-plugin development.

## MCP server

`pgs` also ships `pgs-mcp`, a local stdio MCP server over the same
scan/status/stage/unstage/commit workflow.

```bash
cargo run --bin pgs-mcp
```

Or add it manually without the plugin:

```bash
claude mcp add --transport stdio pgs -- /path/to/pgs-mcp
codex mcp add pgs -- /path/to/pgs-mcp
```

MCP tool calls require an explicit `repo_path`. The server speaks MCP
`2026-07-28` and nothing else; clients on an older revision are not supported.
Advertised capabilities are exactly `{"tools":{}}` — no prompts,
resources, logging, or tasks. For the full protocol contract and safety notes,
see `docs/MCP_SERVER.md`.

### Opt-in hook: nudge toward pgs instead of `git add`

`pgs` ships **no hooks by default**. That is deliberate: a plugin should not
impose policy on your session or add latency to every tool call.

If you want one, add this to your own `.claude/settings.json`. It fires when
Claude is about to run `git add` through the Bash tool and injects a one-line
reminder. It is a nudge — the `git add` still runs.

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "if": "Bash(git add *)",
            "command": "printf %s '{\"hookSpecificOutput\":{\"hookEventName\":\"PreToolUse\",\"additionalContext\":\"This repo has pgs. Prefer pgs_scan then pgs_stage with the narrowest selector (file, 12-hex hunk id, or path:10-20) over whole-file git add.\"}}'"
          }
        ]
      }
    ]
  }
}
```

No dependencies — the command is a `printf` of a fixed string, so there is
nothing to parse and nothing to install. Drop the `if` line to fire on every
Bash call instead of only `git add`.

Two caveats. `if` is a best-effort filter: it inspects command names, including
inside `$(...)` and after `&&`, but it fails open on shell it cannot parse, so
the hook may run on a command you did not expect. And `git add` with no
arguments does not match `git add *`, which wants at least one argument.

## Build

```bash
cargo build                        # compile
cargo test                         # all tests
cargo clippy -- -D warnings        # lint (zero warnings)
cargo fmt --check                  # format check
```

Requires Rust 1.88+ and a C compiler (for libgit2).

See `docs/CLI_SPEC.md` for the complete output contract and
`docs/ARCHITECTURE.md` for system design.
