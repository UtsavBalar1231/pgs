# pgs

[![CI](https://github.com/UtsavBalar1231/pgs/actions/workflows/ci.yml/badge.svg)](https://github.com/UtsavBalar1231/pgs/actions/workflows/ci.yml)
[![Latest release](https://img.shields.io/github/v/release/UtsavBalar1231/pgs)](https://github.com/UtsavBalar1231/pgs/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

`pgs` stages git changes at file, hunk, and line granularity without a
terminal prompt. It is a CLI (`pgs`) and a stdio MCP server (`pgs-mcp`) built on
one engine, plus an agent skill that teaches the scan, plan, stage, commit
workflow. It runs on Linux, macOS, and Windows.

## Why

`git add -p` can be driven headlessly by piping answers into it, so lack of a
TTY is not the problem. The problem is what the answers mean: they are
positional, one per hunk in diff order, so any change to the diff shifts every
later answer. A short answer list stops early, exits 0, and leaves only a subset
staged. Hunks have no stable names, so a selection made from one `git diff`
cannot be replayed against the next. Building a patch by hand for
`git apply --cached --unidiff-zero` has its own failure: a patch whose header
names the wrong line is applied at that line, silently, with exit 0.

`pgs` names every hunk by a 12-hex content hash, so a selection means the same
thing until the file changes, and an ID from a stale scan is refused. `--expect`
checksums extend that check to line ranges and refuse stale input with exit 3.
It builds each index blob in-process with libgit2 rather than shelling out to
`git`, backs up the index before every mutation, and restores it when anything
fails. The MCP server and the skill let an agent do all of this without
touching a shell.

## How it compares

| Tool | Granularity | How you select | Notes |
|------|-------------|----------------|-------|
| `git add -p` piped, `git add -e`, `git apply --cached` | hunk, line | positional answers, or a hand-built patch or edit | Scriptable, but answers and patch headers carry no stable names and a wrong position can apply with exit 0. |
| GitButler `but` | hunk | hunk IDs | Works inside GitButler's workspace model rather than the plain git index. |
| Jujutsu `jj split` | file, when run non-interactively | paths | Operates on jj changes. Hunk-level splits need the interactive diff editor. |
| git-surgeon | hunk | hunk IDs hashed from file path and content | Stages, unstages, and discards hunks. |
| git-hunk | hunk | hunk | Several unrelated projects share this name. |
| Roasbeef/hunk | line | `file:range` | Builds a unified diff for the range and applies it with `git apply --cached`. |
| pgs | file, hunk, line | path, 12-hex hunk ID, or `path:range` | Builds index blobs in-process, refuses stale input, ships an MCP server and a skill. |

This table describes what each project documents about itself. Check their
repositories for current behavior.

## Install

### Prebuilt binaries

Each [GitHub release](https://github.com/UtsavBalar1231/pgs/releases/latest)
ships both binaries for every platform, named `pgs-<target>` and
`pgs-mcp-<target>`, plus a `SHA256SUMS` file.

| Platform | `<target>` |
|----------|------------|
| Linux x86_64 | `x86_64-unknown-linux-musl` |
| Linux ARM64 | `aarch64-unknown-linux-musl` |
| macOS, Intel and Apple Silicon | `universal-apple-darwin` (or `x86_64-apple-darwin`, `aarch64-apple-darwin`) |
| Windows x86_64 | `x86_64-pc-windows-msvc` (`.exe`) |
| Windows ARM64 | `aarch64-pc-windows-msvc` (`.exe`, new in 0.8.0) |

Linux and macOS:

```bash
TARGET=x86_64-unknown-linux-musl   # or aarch64-unknown-linux-musl, universal-apple-darwin
BASE=https://github.com/UtsavBalar1231/pgs/releases/latest/download
mkdir -p ~/.local/bin
curl -fsSL -o ~/.local/bin/pgs     "$BASE/pgs-$TARGET"
curl -fsSL -o ~/.local/bin/pgs-mcp "$BASE/pgs-mcp-$TARGET"
chmod +x ~/.local/bin/pgs ~/.local/bin/pgs-mcp
```

Windows (PowerShell):

```powershell
$Target = "x86_64-pc-windows-msvc"   # or aarch64-pc-windows-msvc
$Base = "https://github.com/UtsavBalar1231/pgs/releases/latest/download"
$Dir = "$env:LOCALAPPDATA\pgs\bin"
New-Item -ItemType Directory -Force $Dir | Out-Null
Invoke-WebRequest "$Base/pgs-$Target.exe" -OutFile "$Dir\pgs.exe"
Invoke-WebRequest "$Base/pgs-mcp-$Target.exe" -OutFile "$Dir\pgs-mcp.exe"
```

Add the install directory to your `PATH`.

### From source

```bash
cargo install --git https://github.com/UtsavBalar1231/pgs --locked
```

This installs both `pgs` and `pgs-mcp`. It needs Rust 1.88+ and a C compiler
(for libgit2).

## Use with your agent

`pgs-mcp` must be on your `PATH` (or use its absolute path in the commands
below). The skill is optional; it is what teaches an agent the workflow.

**Claude Code**

```text
/plugin marketplace add UtsavBalar1231/pgs
/plugin install pgs@pgs-marketplace
```

Or, without the plugin: `claude mcp add --transport stdio --scope user pgs -- pgs-mcp`

**Codex**

```bash
codex plugin marketplace add UtsavBalar1231/pgs
codex plugin add pgs@pgs-marketplace
```

Or register only the server: `codex mcp add pgs -- pgs-mcp`, or in
`~/.codex/config.toml`:

```toml
[mcp_servers.pgs]
command = "pgs-mcp"
```

**Gemini CLI**

```bash
gemini extensions install https://github.com/UtsavBalar1231/pgs
```

Or register only the server: `gemini mcp add -s user pgs pgs-mcp`

**GitHub Copilot CLI and VS Code**

```bash
copilot plugin install UtsavBalar1231/pgs
code --add-mcp '{"name":"pgs","command":"pgs-mcp"}'
```

The first command reads `.claude-plugin/marketplace.json`. VS Code also accepts
the install link
`vscode:mcp/install?%7B%22name%22%3A%22pgs%22%2C%22command%22%3A%22pgs-mcp%22%7D`.

**Cursor**

Open `cursor://anysphere.cursor-deeplink/mcp/install?name=pgs&config=eyJjb21tYW5kIjoicGdzLW1jcCJ9`,
or add this to `~/.cursor/mcp.json`:

```json
{ "mcpServers": { "pgs": { "command": "pgs-mcp" } } }
```

**Skill only.** The `git-commit-staging` skill lives at
`plugins/pgs/skills/git-commit-staging`. Plugin installs include it. For a manual
MCP setup, copy that directory into `~/.agents/skills/` (read by Codex, Gemini,
Cursor, Copilot, OpenCode, Amp, Goose, Junie, Factory, Warp, Windsurf, Zed, and
Crush), `~/.claude/skills/` (Claude Code, Cline), or `~/.kiro/skills/` (Kiro).

The plugin installs register 10 MCP tools: `pgs_scan`, `pgs_status`,
`pgs_stage`, `pgs_unstage`, `pgs_commit`, `pgs_log`, `pgs_overview`,
`pgs_split_hunk`, `pgs_plan_check`, and `pgs_plan_diff`.

Setup for OpenCode, Goose, Amp, Crush, Continue, Cline, Zed, Kiro,
Windsurf/Devin, Factory Droid, JetBrains Junie, and Warp is in
[docs/INTEGRATIONS.md](docs/INTEGRATIONS.md).

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
pgs commit -F msg.txt                       # message from a file (.txt, .md, anything)
pgs commit -F -                             # message from stdin
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
`{"commits": [{"id": "c1", "selections": ["abc123def456", "src/b.rs"], "message": "..."}]}`.
Each commit takes `selections`, and optionally `id`, `exclude`, and `message`.
`plan-check` and `plan-diff` are descriptive: they report overlaps, gaps, and
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
Re-scan after any edit, commit, or index change before reusing them. The
`--context` value changes hunk boundaries and therefore IDs, so pass the same
value to every call that shares IDs.

## Guard against drift

`scan` reports a per-file `checksum`. Pass it back to `stage` to refuse staging
if the file changed between the scan and the stage:

```bash
pgs stage src/main.rs:10-20 --expect "src/main.rs=<checksum from scan>"
```

A stale checksum fails with exit code 3 (`stale_scan`) and leaves the index
untouched. Re-scan and retry. Partial (hunk/line) staging also refuses non-UTF-8
files and files whose line endings differ from the index; stage those whole-file.

## Platform support

`pgs` runs on Linux, macOS, and Windows, and CI tests all three.

Staging applies git's clean filters the way `git add` does: `core.autocrlf`,
`text` and `eol` attributes, and `ident`. A path whose `filter=` attribute runs
an external program, such as git-lfs or git-crypt, is refused rather than staged
unfiltered. The error code is `external_filter` (exit 2); stage those paths with
`git add`. This is new in 0.8.0.

The plugin launcher downloads `pgs-mcp` on first start for Linux (x86_64,
aarch64), macOS, and Windows (x86_64, ARM64). It is a POSIX `sh` script, so on
Windows it needs the `sh` that ships with Git for Windows on `PATH`; otherwise
install the binary from the release and register it with your agent directly.

## Output

Default: structured text markers, `@@pgs:v1 <kind> <json>`.
JSON: opt-in via `--json` or `--output json`.

See [docs/CLI_SPEC.md](docs/CLI_SPEC.md) for the full output contract and the
table of error codes.

## Exit codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | No effect (nothing to stage, empty selection) |
| 2 | User error (bad selector; binary, non-UTF-8, CRLF, or external-filter partial-stage constraint) |
| 3 | Conflict: re-scan and retry (stale scan, locked index) |
| 4 | Internal error |

## MCP server

`pgs-mcp` is a local stdio MCP server over the same engine as the CLI. Run it
with `pgs-mcp`, or from a source checkout with `cargo run --bin pgs-mcp`.

MCP tool calls require an explicit `repo_path`. The server speaks MCP
`2026-07-28` (per-request `_meta`, `server/discover`) and also serves
`initialize`-era clients at `2025-11-25` and `2025-06-18`. Advertised
capabilities are exactly `{"tools":{}}`: no prompts, resources, logging, or
tasks. For the full protocol contract and safety notes, see
[docs/MCP_SERVER.md](docs/MCP_SERVER.md).

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
cargo clippy --all-targets -- -D warnings   # lint (zero warnings)
cargo fmt --check                  # format check
```

Requires Rust 1.88+ and a C compiler (for libgit2). See
[CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request,
[docs/CLI_SPEC.md](docs/CLI_SPEC.md) for the output contract, and
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for system design. Report security
issues as described in [SECURITY.md](SECURITY.md).

## License

[MIT](LICENSE)
