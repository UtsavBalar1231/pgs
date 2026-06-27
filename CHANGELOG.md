# Changelog

All notable changes to pgs will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

## 0.5.0 - 2026-06-27

### Added

- Scan-drift detection: `pgs stage --expect PATH=SHA` (CLI) and
  `expected_checksums` (MCP `pgs_stage`) abort with `StaleScan` (exit code 3,
  zero index mutation) when a file changed between the scan and the stage.
- `file_checksum` is now reported in compact `scan` output (and MCP `pgs_scan`),
  not only under `--full`, so agents can capture it to drive the drift guard.
- Executable-mode changes now propagate through partial (hunk/line) staging.
- `plan-check` reports stale hunk IDs in a dedicated `unknown_hunk_ids` array,
  distinct from `unknown_paths`.

### Changed

- Partial (hunk/line) staging now refuses non-UTF-8 files (`NonUtf8Partial`) and
  files whose line endings differ from the index (`CrlfMismatch`) instead of
  silently corrupting the staged blob; stage those whole-file.
- Backup-restore failures during a stage/unstage rollback are now surfaced via
  `RestoreFailed` (exit code 4) instead of being swallowed.
- `CommitPlan.version` is optional and defaults to `"v1"` when omitted.
- Internal refactor: the models and view layers were modularized and the MCP
  tool-result builders collapsed, with byte-identical tool output.
- The published `git-commit-staging` skill and the README were rewritten for
  clarity; internal implementation details were removed from the skill.

### Fixed

- Whole-file staging reported a byte count as `lines_affected`; it now reports
  the line count, consistent with hunk and line staging.
- The scan `mode_changed` summary no longer counts added or deleted files.
- `plan-diff` fuzzy matching dropped a dishonest first-hunk fallback: it now
  searches all files for a content-checksum match, allows checksum-only shifts,
  and classifies selections with no genuine signal as `gone`.
- Fixed a line-index overflow in `unstage`.

## 0.4.2 - 2026-06-13

### Fixed

- Plugin MCP first startup now serializes binary installation with a lock and
  atomically renames downloaded binary/version files into place, avoiding
  concurrent install `Text file busy` failures in Codex and Claude Code.
- Plugin MCP launchers now use simple environment fallback logic so Claude Code
  does not pre-expand nested shell parameter substitutions incorrectly.
- MCP tool schemas no longer emit nonstandard integer `format` annotations such
  as `uint32`, avoiding noisy Claude Code schema validation warnings.

## 0.4.1 - 2026-06-13

### Fixed

- Codex plugin MCP startup no longer relies on Claude-only plugin root variables.
- Plugin launcher scripts now refresh a stale cached binary by comparing the tracked install version against the bundled `VERSION`, and resolve the plugin root/data dirs from `CLAUDE_PLUGIN_ROOT`/`CLAUDE_PLUGIN_DATA` when present.
- `stage --dry-run --explain` no longer errors when previewing a newly-added file inside a brand-new directory; the absent base blob is treated as empty so every workdir line is reported as an addition.

## 0.4.0 - 2026-05-27

### Added

- Codex plugin packaging with bundled MCP server wiring and the `git-commit-staging` skill.
- `pgs_commit` amend support for rewriting the HEAD commit message and tree through the MCP/CLI contract.
- MCP `pgs_stage` exact dry-run previews via `dry_run`, `explain`, and `limit` fields.

### Changed

- Reworked the `git-commit-staging` skill into a concise MCP-only workflow with reference docs, full tool coverage, and an explicit commit-message quality gate.
- Updated Claude Code plugin packaging to the current marketplace/plugin layout, with `plugins/pgs` as the packaged plugin root and top-level `skills` as a symlink to the packaged skill tree.

## 0.3.1 - 2026-05-04

### Fixed

- Hunk-by-ID and line-range staging/unstaging no longer leak content from adjacent hunks when their old-file and new-file line numbers numerically alias. Internal `stage_lines` / `unstage_lines` API now takes a `LineSelection { old_lines, new_lines }` instead of a single `HashSet<u32>`.

## 0.3.0 - 2026-04-18

### Added
- `pgs overview` / `pgs_overview` — unified scan + status view composed from existing handlers.
- `pgs stage --dry-run --explain` — exact-content line preview via `OperationPreview`; never mutates the index.
- `pgs split-hunk` / `pgs_split_hunk` — descriptive classification of contiguous addition/deletion/mixed runs inside a hunk.
- `pgs plan-check` / `pgs_plan_check` — validates a saved `CommitPlan` against a fresh scan; reports overlaps, coverage gaps, and hunk-boundary crossings.
- `pgs plan-diff` / `pgs_plan_diff` — reconciles a saved `CommitPlan` against a fresh scan; classifies entries as `still_valid`, `shifted`, or `gone`.
- `HunkInfo.whitespace_only` — per-hunk metadata flag surfaced in scan text markers and JSON envelopes.
- `tests/test_skill_capability_table.rs` — anti-drift harness that verifies every `src/...:NNN` citation in the skill's Capability Truth Table resolves to a live load-bearing symbol within ±5 lines.

### Changed
- Switched CLI output to text-default with stable marker records.
- Added explicit JSON opt-in mode via `--json` / `--output json`.
- Unified parse and runtime failures under one structured error contract.
- Centralized rendering in `src/output/*` with shared view models.
- Rewrote public and internal docs to describe only the new contract:
  - marker grammar: `@@pgs:v1 <kind> <minified-json-payload>`
  - parse/runtime error fields: `version`, `command`, `phase`, `code`, `message`, `exit_code`
- Rewrote `skills/git-commit-staging/SKILL.md` around the MCP tool surface, including the §0 Capability Truth Table with source anchors for every shipped promise.

### Verification (final no-regression sweep — plan mcp-skill-rewrite)
- `cargo fmt --check`: clean.
- `cargo clippy --all-targets -- -D warnings`: clean.
- `cargo test`: full suite green (143 lib + ~30 integration test files, 0 failures).
- `pgs-mcp` `tools/list`: returns exactly 10 tools (`pgs_scan`, `pgs_status`, `pgs_stage`, `pgs_unstage`, `pgs_commit`, `pgs_log`, `pgs_overview`, `pgs_split_hunk`, `pgs_plan_check`, `pgs_plan_diff`).
- Scratch-repo `scan -> stage -> status -> commit` smoke: all four exit codes 0.
