# Using pgs

[← Project overview](../README.md) · [Install](INSTALL.md) · [Integrations](INTEGRATIONS.md)

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
pgs plan-apply --stdin < plan.json     # stage and commit each planned commit, in order
```

A plan looks like
`{"commits": [{"id": "c1", "selections": ["abc123def456", "src/b.rs"], "message": "..."}]}`.
Each commit takes `selections`, and optionally `id`, `exclude`, and `message`.
`plan-check` and `plan-diff` are descriptive: they report overlaps, gaps, and
drift but never stage or commit. `plan-apply` executes a clean plan: it needs an
empty index and a `message` on every commit, refuses anything `plan-check` would
flag before writing, and if a later commit fails it keeps the earlier ones and
reports exactly which were created.

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
`git add`. A moved submodule pointer stages by path, like `git add <submodule>`;
hunk and line selectors on it are refused.

The plugin launcher downloads `pgs-mcp` on first start for Linux (x86_64,
aarch64), macOS, and Windows (x86_64, ARM64). It is a POSIX `sh` script, so on
Windows it needs the `sh` that ships with Git for Windows on `PATH`; otherwise
install the binary from the release and register it with your agent directly.

## Output

Default: structured text markers, `@@pgs:v1 <kind> <json>`.
JSON: opt-in via `--json` or `--output json`.

See [CLI_SPEC.md](CLI_SPEC.md) for the full output contract and the
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
[MCP_SERVER.md](MCP_SERVER.md).

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
