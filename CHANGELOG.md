# Changelog

All notable changes to pgs will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

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
