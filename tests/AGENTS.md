# tests/ — Test Organization

34 integration test files covering CLI commands, MCP tools, output modes, packaging, and E2E workflows. Unit tests live next to the code in `src/`.

## File Naming Convention

| Pattern | Purpose | Example |
|---------|---------|---------|
| `test_<domain>.rs` | CLI command integration tests | `test_scan.rs`, `test_stage.rs` |
| `test_mcp_<tool>.rs` | Individual MCP tool tests | `test_mcp_scan_tool.rs`, `test_mcp_stage_tool.rs` |
| `test_mcp_<feature>.rs` | MCP infrastructure tests | `test_mcp_stdio.rs`, `test_mcp_adapter.rs` |
| `test_output_modes.rs` | Text marker and JSON contract tests | |
| `test_e2e.rs` | End-to-end CLI workflows via `assert_cmd` | |
| `test_mode_change.rs` | File permission change staging/unstaging | |

## Shared Helpers (common/mod.rs)

| Helper | Purpose |
|--------|---------|
| `setup_repo()` | Creates `TempDir` + `Repository` with git identity, `core.autocrlf=false` (Windows runners set it true system-wide), and an initial empty commit |
| `write_file(dir, path, content)` | Write file to workdir, creating parent dirs |
| `commit_file(repo, dir, path, content, msg)` | Write + add + commit in one call |
| `run_pgs(dir, args)` | Run `pgs --json --repo <dir>` via `assert_cmd` |
| `run_pgs_raw(dir, args)` | Run `pgs --repo <dir>` (text mode, no --json) |
| `MCP_PROTOCOL_VERSION` | Re-exported from `pgs::mcp` for MCP test assertions |
| `open_session(...)` | Opens a `2026-07-28` MCP session with `server/discover`; `list_tools`/`call_tool`/`send_tool_call` then attach `modern_meta()` to every request |
| `initialize_legacy_session(..., version)` | Drives the `initialize` + `notifications/initialized` handshake for `2025-06-18`/`2025-11-25` tests |

**`run_pgs` vs `run_pgs_raw`**: `run_pgs` always adds `--json` — use it when asserting on JSON output. `run_pgs_raw` uses text mode — use it for text marker tests or when testing output mode selection.

## Test Isolation Rules

- Every test creates its own `TempDir` — no shared mutable state
- `TempDir` must outlive `Repository` (bind `dir` before `repo` in destructuring)
- No filesystem paths outside the temp dir
- No network calls
- No sleeps or hardcoded timeouts

## Vary Fixture Shape, Not Just Content

Every fixture in `tests/test_replace_run.rs` ended with a trailing newline. That
single blind spot in fixture *shape* hid three data-loss bugs from a suite
otherwise strong enough to survive mutation testing — the content varied plenty,
the file-level state never did.

The rule: when a code path handles file-level edge state — trailing newline
present or absent, empty file, CRLF, non-UTF-8 — the fixture set must vary that
state. Adding more content permutations at a fixed shape buys nothing.

## MCP Test Pattern

Most `test_mcp_*` files drive the real `pgs-mcp` binary over stdio through the
helpers above. `test_mcp_adapter.rs` calls `cmd::mcp_adapter::execute()`
directly. Pin pgs's own contract (tool set, cache hints, error envelopes, lane
behaviour), not rmcp's protocol plumbing: SDK negotiation details churn with
every rmcp release.

## Cross-Platform

CI runs the suite on Linux, macOS, and Windows. Gate Unix-only fixtures
(symlinks, file modes) with `#[cfg(unix)]`, and never rely on the runner's git
config: pin anything that changes bytes (`core.autocrlf`) in the test repo.

## Test Naming Convention

`<thing>_<scenario>_<expected>` — e.g.:
- `scan_modified_file_returns_hunk`
- `stage_single_line_in_modified_file_updates_index`
- `unstage_hunk_restores_head_content`
