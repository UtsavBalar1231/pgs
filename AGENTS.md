# pgs — Agent Knowledge Base

Non-interactive git staging CLI with MCP server. Rust, libgit2, ~15k lines of `src/`
and ~10k of tests.

## Two Binaries

| Binary | Entry | Runtime | Purpose |
|--------|-------|---------|---------|
| `pgs` | `src/main.rs` | sync | CLI — scan, stage, unstage, status, commit, log, overview, split-hunk, plan-check, plan-diff |
| `pgs-mcp` | `src/bin/pgs-mcp.rs` | async (tokio) | MCP server over stdio — 10 tools; MCP `2026-07-28` plus `initialize`-era `2025-11-25`/`2025-06-18` |

Both share the same library crate. The MCP server reuses CLI command handlers via `cmd/mcp_adapter.rs`.

## Structure

```
src/
  main.rs              CLI entry — parse, dispatch, render, exit
  lib.rs               Module root + saturating_u32() helper
  models/              ALL serializable types, split by domain
    mod.rs             Module root + public re-exports
    scan.rs            ScanResult, FileInfo, HunkInfo, DiffLineInfo, FileStatus, CompactScanResult
    status.rs          StatusReport, StagedFileInfo, StatusSummary
    operation.rs       OperationStatus
    commit.rs          CommitResult
    preview.rs         OperationPreview, PreviewLine
    plan.rs            CommitPlan, PlannedCommit
    selection.rs       ResolvedSelection, LineRange
  error.rs             PgsError enum, code() and exit_code() mapping

  cmd/                 Command handlers — one file per command + adapter
    mod.rs             Cli struct (clap), Command enum, OutputMode, run() dispatcher
    mcp_adapter.rs     Typed bridge: MCP requests -> CLI command handlers
    scan|stage|unstage|status|commit|log|overview|split|plan_check|plan_diff|plan_io.rs

  git/                 All libgit2 operations — SEE src/git/AGENTS.md
  mcp/                 MCP protocol server (rmcp 3.5) — SEE src/mcp/AGENTS.md
    mod.rs             PROTOCOL_VERSION, SUPPORTED_PROTOCOL_VERSIONS
    contract.rs        Tool constants, input schemas, annotations, result mapping
    runtime.rs         Per-repo mutation lanes for stage/unstage/commit
    server.rs          rmcp ServerHandler, stdio transport, server instructions
  output/              Rendering boundary (view models -> text markers / JSON)
    view/              CommandOutput enum, shared view-model types
      mod.rs           CommandOutput enum, re-exports
      scan.rs          Scan view-model types
      operation.rs     Stage/unstage view-model types
      plan.rs          Plan-check and plan-diff view-model types
      common.rs        Shared error view model
    text.rs            @@pgs:v1 marker renderer
    json.rs            JSON envelope renderer
  selection/           Positional arg parsing + resolution
    parse.rs           Auto-detect: file path vs 12-hex hunk ID vs path:ranges
    resolve.rs         Validate selections against scan results
  safety/              Index protection
    lock.rs            index.lock detection + exponential backoff
    backup.rs          Pre-operation index backup + restore on error

tests/                 SEE tests/AGENTS.md
```

Packaging, outside `src/`:

```
plugins/pgs/           Claude Code / Codex / Copilot / Droid plugin (manifests, .mcp.json, skill, README)
skills/                byte-identical copy of plugins/pgs/skills/ (root Codex plugin, Gemini extension)
.claude-plugin/        marketplace.json (also read by Codex, Copilot CLI, Factory Droid)
.codex-plugin/         root Codex plugin manifest (mirrored in plugins/pgs/)
.agents/plugins/       Codex-native marketplace
gemini-extension.json  Gemini CLI extension (same inline launcher as .mcp.json)
server.json            MCP Registry draft (not published)
glama.json             Glama directory ownership
```

`tests/test_codex_plugin.rs` keeps the mirrors byte-identical and every version
anchor equal to the crate version.

## Where to Look

| Task | Start here | Key files |
|------|-----------|-----------|
| Add/change CLI flag | `src/cmd/mod.rs` | Cli struct, Command enum |
| Add/change MCP tool | `src/mcp/contract.rs` | Tool schemas, then `runtime.rs` |
| Modify staging logic | `src/git/staging.rs` | `stage_file`, `stage_lines`; pairing in `git/mod.rs` |
| Modify unstaging | `src/git/unstaging.rs` | `unstage_file`, `unstage_lines` |
| Change diff behavior | `src/git/diff.rs` | `diff_index_to_workdir`, `diff_head_to_index` |
| Change output format | `src/output/view/` | View-model types, then `text.rs`/`json.rs` |
| Add shared data type | `src/models/` | Pick the domain submodule; re-export from `mod.rs` |
| Add error variant | `src/error.rs` | PgsError enum, exit_code() |
| Change selection parsing | `src/selection/parse.rs` | Auto-detect rules |
| Change safety behavior | `src/safety/lock.rs`, `backup.rs` | Lock wait, backup restore |
| Add MCP-CLI bridge type | `src/cmd/mcp_adapter.rs` | McpCommandRequest enum |
| Understand test setup | `tests/common/mod.rs` | Shared helpers |

## Cross-Cutting Patterns

- **`saturating_u32(n: usize)`** in `lib.rs` — used everywhere git2 or contracts need `u32` from `usize`
- **Shared git helpers** in `git/mod.rs` — `read_workdir_for_index` (clean-filtered content for staging), `read_workdir_for_blob` (raw bytes, for checksums), `build_index_entry`, `replace_run_partners` + `flush_replace_run` (one pairing rule for selection and assembly), `read_head_blob`, `read_index_blob`
- **No subprocess calls** — all git operations via libgit2 (verified: zero `std::process::Command` in src/)
- **No trait objects** — zero `pub trait` or `dyn Trait` in the codebase; all concrete types
- **Atomic operations** — stage/unstage create index backup, stop on first failure, restore on error

## Config Files

| File | Purpose |
|------|---------|
| `rustfmt.toml` | `edition = "2024"`, `max_width = 100`, `use_field_init_shorthand = true` |
| `Cargo.toml [lints.clippy]` | `deny all`, `warn pedantic+nursery`, 6 specific `allow` exceptions |

## What CLAUDE.md Covers (don't duplicate)

Build commands, error handling rules, testing conventions (TDD, naming), code quality rules, dependency list, output contract spec, git conventions. See `CLAUDE.md` and `docs/CLI_SPEC.md`.
