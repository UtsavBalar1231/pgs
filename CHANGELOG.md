# Changelog

All notable changes to pgs will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

## 0.7.0 - 2026-08-05

### Added

- `EmptyCommitMessage` (`empty_commit_message`, exit 2) joins the error enum.
  Blank is Rust's `str::trim` definition — the Unicode `White_Space` property —
  so `\r\n`, U+00A0 and U+3000 all count. The check now runs on the normalized
  message, so a message that normalizes to nothing is refused the same way.
- `pgs commit -F <path>` / `--message-file <path>` reads the commit message from
  a file, mirroring `git commit -F`. Works with `--amend`. `-F -` reads stdin on
  the CLI; the `pgs_commit` MCP tool rejects `-` because an MCP request has no
  stdin of its own. Exactly one of `-m` / `-F` must be supplied: neither or both
  is a user error (exit 2).
- `pgs_commit` MCP tool gains an optional `message_file` field; `message` is now
  optional at the schema level since either source satisfies the call.
- `InvalidArguments` (`invalid_arguments`, exit 2) and `InputFileUnreadable`
  (`input_file_unreadable`, exit 2) join the error enum.
- `UnterminatedInteriorLine` (`unterminated_interior_line`, exit 2) joins the
  error enum. Raised when a line-range selection would place content after a
  base file's unterminated last line: applying it also requires terminating that
  line, a change the selection never named. The guard is a post-condition on the
  assembled blob, so it cannot fire on a selection that produces a well-formed
  result. Whole-hunk and whole-file staging are unaffected.

### Changed

- Every commit message is normalized with git's `--cleanup=whitespace` rules
  regardless of source: CRLF to LF, trailing whitespace stripped per line,
  leading and trailing blank lines dropped, runs of blank lines collapsed to
  one, and exactly one trailing newline. `#` comment lines are preserved —
  stripping them is git's `strip` mode, which applies to editor input only.
  `-m "feat: x"` therefore now stores `feat: x\n`, matching git, where it
  previously stored `feat: x` with no trailing newline.

### Fixed

- `pgs commit` and `pgs commit --amend` now reject an empty or whitespace-only
  `-m` message with `empty_commit_message` (exit 2). Previously the CLI created
  a commit with no message, and `--amend` silently destroyed the existing one
  with no recovery outside the reflog. Validation runs before any index or
  object-database access, so a rejected `--amend` leaves `HEAD` untouched.
- The check moved into the shared command handler, so the CLI and the
  `pgs_commit` MCP tool now reject the same inputs. It had lived only at the MCP
  boundary, which is why the CLI path bypassed it entirely.


- Line-range staging no longer destroys a base file's unterminated last line.
  `similar::TextDiff::from_lines` tokenizes each line with its terminator, so
  base `x\nb` against workdir `x\nY\nb\n` diffs as `-b` / `+Y` / `+b`, where
  `-b` and `+b` are one line regaining its newline. Deletions were paired with
  additions by position, binding `-b` to `+Y`, so `stage f.txt:2-2` applied the
  deletion of `b` while leaving its replacement out and staged `x\nY\n`. A
  deletion now prefers the addition with identical content, falling back to the
  positional partner.
- Line-range staging no longer fabricates content. Appending after an
  unterminated last line (base `a\nb`, workdir `a\nb\nc\n`, `stage f.txt:3-3`)
  emitted the preserved token `b` at a non-final position and staged `a\nbc\n` —
  a line present in neither the base nor the workdir. Such a selection is now
  refused with `unterminated_interior_line` (exit 2) and the index is untouched.
- `pgs unstage` no longer appends a phantom trailing newline. With HEAD at
  `a\nb` (no final newline), undoing a staged deletion produced `a\nb\n`, so
  `pgs status` still reported the file modified and an "unstage everything,
  verify clean" loop never converged. The result's trailing newline is now taken
  from the final emitted token instead of the index side's convention, which
  only overrides bytes when that token is a kept index line.
- An unreadable `--plan` path on `plan-check` and `plan-diff` now exits 2 with
  `input_file_unreadable`, as `docs/CLI_SPEC.md` already promised. It exited 4
  with `io_error`, treating a caller's typo as an internal fault.

## 0.6.1 - 2026-08-04

### Fixed

- Line-range staging no longer applies a deletion the caller did not name. A
  pure-deletion run (deleted lines with no additions) was anchored to the
  surviving line that follows it, so `pgs stage f.txt:2-2` naming a single
  unchanged line could delete adjacent lines from the index. A pure-deletion run
  is now staged only when the range covers a surviving line on each side of the
  gap it occupies; at the start or end of the file, where the gap has one side,
  the range must cover the adjacent line plus one further line on that side.
- A deletion at the end of a file is reachable by a line range again. It
  anchored past the last line of the new file, so no range could select it and
  `pgs stage f.txt:1-N` silently failed to reproduce the workdir.
- `pgs unstage` shares the same selection path and is fixed with it.
- Whether a gap sits at the end of the file is now determined from the file's
  new-side line count instead of inferred from trailing context inside the hunk.
  The old inference was sound only because `--context` is clamped to a minimum
  of 1 in the CLI; a library caller passing zero context could misclassify an
  interior gap as trailing.

### Changed

- A line-range selection that resolves to no changed lines now returns
  `SelectionEmpty` (exit code 1) instead of reporting `status: "ok"` with
  `lines_affected: 0`. Callers treating exit 0 with a zero line count as success
  will now see exit 1.

## 0.6.0 - 2026-08-04

### Added

- `pgs-mcp` answers `server/discover`, the MCP `2026-07-28` replacement for
  `initialize`. It reports `supportedVersions`, capabilities, instructions,
  `ttlMs`, and `cacheScope`.
- `tools/list` now carries `ttlMs: 3600000` and `cacheScope: "public"`; every
  result carries `resultType: "complete"`. Both are required by `2026-07-28`.
- The server now sends `instructions` describing the scan → stage → commit
  workflow, hunk-id staleness, and the `expected_checksums` drift guard. It
  previously sent none.

### Changed

- Upgraded `rmcp` from 1.2 to 3.1 and moved the MCP protocol version from
  `2025-11-25` to `2026-07-28`. The server is modern-only: it advertises
  `["2026-07-28"]` and clients on an older revision are not supported. A
  request declaring an older version in `_meta` is rejected with `-32022`, and
  an `initialize` asking for one is answered with `2026-07-28` rather than the
  requested revision.
- Under `2026-07-28`, `protocolVersion` and `clientCapabilities` move into
  per-request `_meta`; a request that omits them is rejected with `-32602`.
- MSRV raised from 1.85 to 1.88, required by `rmcp` 3.1.
- Upgraded `git2` 0.20 to 0.21, `sha2` 0.10 to 0.11, `similar` 2 to 3, and
  `clap` 4.5 to 4.6. Hunk IDs, hunk checksums, and file checksums are
  byte-identical across the upgrade; the content-addressed output contract is
  unchanged.

### Removed

- MCP task support. `tasks/list`, `tasks/get`, `tasks/result`, and
  `tasks/cancel` now return `-32601`, and the per-tool `task_support` annotation
  is gone. `2026-07-28` moved tasks out of the core spec into a separate draft
  extension. Read every tool result from the `tools/call` response directly.
- Advertised capabilities are now exactly `{"tools":{}}`.

### Fixed

- Line-range staging no longer drops content. Staging a subset of a contiguous
  replace-run (adjacent modified lines) deleted the unselected lines' original
  content instead of preserving it, so `pgs stage f.txt:2-3` over a three-line
  replacement silently lost the third line. Within a replace-run the i-th
  deletion now pairs with the i-th addition and is staged only when that
  addition is selected. `pgs unstage` shared the same selection path and is
  fixed with it.
- MCP mutation lanes now order by arrival sequence rather than request id. The
  derived ordering compared the request id first, so FIFO held only because
  clients happen to send monotonically increasing ids, and a numeric id always
  sorted ahead of a string one.
- A reused in-flight request id no longer orphans a pending mutation slot. The
  displaced registration is now cancelled; previously it left an unreachable
  entry in the lane queue and stalled every later mutation on that repository.

Unchanged: all 10 tools, their input schemas, their `ToolAnnotations`, and the
per-repo serialization of `pgs_stage`/`pgs_unstage`/`pgs_commit`.

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
