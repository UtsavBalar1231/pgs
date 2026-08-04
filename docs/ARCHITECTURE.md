# pgs Architecture

## Overview

`pgs` is a Rust CLI for non-interactive git staging at file, hunk, and line granularity.

Key properties:

- structured output: text markers by default, JSON via `--json`
- centralized renderer boundary and shared view-model contracts in `src/output/*`
- command handlers return typed outputs (no direct printing)
- all git operations via libgit2
- `scan` defaults to compact output and emits full line content only with `--full`

## Rendering Boundary

The presentation layer is intentionally separated from execution:

- `src/cmd/*` creates typed command results.
- `src/output/view/` defines shared models used by both text and JSON renderers.
- `src/output/text.rs` renders v1 marker records.
- `src/output/json.rs` renders the same models as JSON envelopes.
- `src/main.rs` handles parsing, dispatch, error capture, and output dispatch.

## Output Contracts

All command outputs are versioned with `version: "v1"`.

Text mode uses the exact marker grammar:

`@@pgs:v1 <kind> <minified-json-payload>`

Recognized marker kinds:

- scan: `scan.begin`, `file`, `hunk`, `summary`, `scan.end`
- scan full: `file.begin`, `hunk.begin`, raw diff body, `hunk.end`, `file.end`, `summary`, `scan.end`
- stage: `stage.begin`, `item`, `warning`, `stage.preview.begin`, `stage.preview.line`, `stage.preview.end`, `stage.end`
- unstage: `unstage.begin`, `item`, `warning`, `unstage.end`
- status: `status.begin`, `status.file`, `summary`, `status.end`
- commit: `commit.result`
- log: `log`
- overview: `overview.begin`, (full `scan.*` block), (full `status.*` block), `overview.end`
- split-hunk: `split.begin`, `split.range`, `split.end`
- plan-check: `plan.check.begin`, `plan.check.overlap`, `plan.check.uncovered`, `plan.check.unsafe`, `plan.check.unknown`, `plan.check.unknown_hunk`, `plan.check.end`
- plan-diff: `plan.diff.begin`, `plan.diff.valid`, `plan.diff.shifted`, `plan.diff.gone`, `plan.diff.end`
- error: `error`

JSON mode is opt-in (`--json`/`--output json`) and serializes the same view-models.

## Error Flow

Parse and runtime failures use a shared model in both modes:

```json
{
  "version": "v1",
  "command": "cli|scan|stage|unstage|status|commit",
  "phase": "parse|runtime",
  "code": "snake_case_error_code",
  "message": "...",
  "exit_code": 2
}
```

- Parse failures: `command: "cli"`, `phase: "parse"`
- Runtime failures: resolved command name and `phase: "runtime"`

## Command Layer

`src/cmd/mod.rs` owns:

- command parsing and output mode handling (`--output`, `--json`)
- best-effort mode detection for parse failures
- command dispatch returning typed outputs

Command handlers (`scan`, `stage`, `unstage`, `status`, `commit`) produce typed results for renderers.

## Git/Data/Safety Layers

Output redesign does not change git behavior.

- `src/git/*`: diffing, staging, unstaging, repo access
- `src/selection/*`: selection parsing and resolution
- `src/safety/*`: index lock checks and backup/restore

Critical diff bases:

- `scan`: Index -> Workdir
- `status`: HEAD -> Index
- `unstage`: HEAD -> Index

## Staging design rationale

### Why index-direct blob reconstruction instead of patch-apply

pgs builds a new index blob from scratch on every partial staging operation rather
than generating a unified diff and applying it with `git apply --cached`. The
mechanism, implemented in `src/git/staging.rs`:

1. Read the base blob from the current index (`read_index_blob`), falling back to
   HEAD (`read_head_blob`) for newly-tracked files.
2. Read the workdir file (`read_workdir_for_blob`).
3. Diff them with `similar::TextDiff::from_lines`.
4. Walk every change; accept or reject each line based on the caller's selection set.
5. Write the resulting bytes as an ODB blob (`repo.blob()`).
6. Commit the entry into the index (`index.add_frombuffer()` + `index.write()`).

No subprocess, no patch file, no `git apply`.

Three concrete problems with the patch-apply approach make it unsuitable here:

**CRLF context mismatch.** A unified diff embeds context lines verbatim. If the
workdir file uses CRLF line endings and the index uses LF (or vice versa), context
lines diverge and `git apply` refuses the patch unless `--whitespace=fix` is passed —
which silently alters content. Index-direct reconstruction avoids context lines
entirely; the line-ending invariant is checked up front (`CrlfMismatch` error) and
both sides are then diffed as-is.

**Whitespace fuzz / offset drift.** `git apply` silently relocates hunks up to three
lines from their declared position ("fuzz"), logging a warning that is easy to miss.
An agent driving structured hunk IDs has precise addresses; silent relocation would
stage different lines than the caller intended. The `similar` diff operates on the
live base + workdir pair, so there is no declared position to drift from.

**One subprocess per hunk.** Spawning a process for each granular stage call carries
fixed OS overhead. pgs is designed for agent workloads that issue many fine-grained
operations in sequence; all work stays in-process via libgit2.

### External validation: GitButler convergence

GitButler's production codebase — a git client aimed at agent and branch-management
workflows — independently arrived at the same strategy: `apply_hunks` reconstructs
the target blob and writes it via gitoxide's blob writer, with no patch-apply step.
This convergence from a separate team working on a separate implementation is
external validation that index-direct reconstruction is the correct approach for
structured partial staging.

### `git apply -p<n>` is path-prefix stripping, not partial staging

The `-p<n>` flag strips `n` leading path components from filenames embedded in a
patch header (e.g. `-p1` turns `a/src/main.rs` into `src/main.rs`). It is unrelated
to choosing which lines or hunks to stage. pgs has no equivalent because it addresses
files by repo-relative path directly — no `a/`/`b/` prefixes, no stripping needed.
Partial staging granularity is controlled by the selection argument (`path:range` or
`hunk_id`), not by any depth flag.

### Enforced staging boundaries

These are hard boundaries checked at call time; violation returns an error and leaves
the index unchanged:

- **Non-UTF-8 partial staging** (`NonUtf8Partial`, exit 2): `similar::TextDiff`
  operates on `&str`. Invalid UTF-8 bytes would be silently replaced by U+FFFD (3
  bytes each) on the `String::from_utf8_lossy` conversion, corrupting the staged
  blob. The guard checks both the base blob and the workdir bytes before any diff
  begins. Whole-file staging (`stage <path>`) is always available and is byte-exact.

- **Cross-ending partial staging** (`CrlfMismatch`, exit 2): when the base blob is
  predominantly CRLF and the workdir is predominantly LF (or vice versa),
  `similar::TextDiff` treats the trailing `\r` as part of each line value, so every
  line appears changed and the output blob is a mixed-ending mess. The guard compares
  dominant line endings on both sides before diffing. Files with no newlines (single-
  line) bypass the guard. Whole-file staging is byte-exact and handles CRLF correctly.

- **Mode changes propagate through partial staging**: `stage_lines` and `stage_hunk`
  accept `mode_override: Option<u32>`, which flows into `build_index_entry`. A file
  whose permission bits changed is therefore staged with the new mode even when only
  some lines are selected.

## Symlink staging

`pgs` uses `symlink_metadata()` + `read_link()` to detect and read symlinks; it never follows
the link to the target file. Index blobs for symlinks contain the raw link-target string bytes
(mode `0o120000`), not the target file's contents. The single point of truth for all workdir
blob reads is the `read_workdir_for_blob` helper in `src/git/mod.rs`; all staging call sites
route through it.

## Rename detection boundary

`diff_index_to_workdir` (`src/git/diff.rs`) does not call libgit2's `find_similar`.
Without that call libgit2 never emits `Delta::Renamed`, so a renamed file always
surfaces as two independent entries — a `Deleted` entry for the old path and an
`Added` entry for the new path — and therefore produces two `FileInfo` values in
the scan result, never one.

`FileStatus::Renamed` (`src/models/scan.rs:68`) and the `stage_rename` call site
(`src/cmd/stage.rs:382`, `src/git/staging.rs:524`) exist in the codebase but are
not reachable from a normal scan. `delta_to_file_status` (`src/git/diff.rs:181`)
maps `Delta::Renamed` correctly, but that branch is never triggered because no
`Delta::Renamed` is ever produced by the index-to-workdir diff.

This is an intentional current boundary. Enabling rename detection would require
calling `diff.find_similar(None)` after constructing the diff and updating scan
output to expose `old_path` to callers. That is a possible future enhancement;
do not assume rename-aware staging is supported until it is explicitly enabled and
tested.

## Concurrency boundary

`pgs` calls `lock::wait_for_lock_release` (`src/safety/lock.rs:19`) at the start of every
mutating operation (`src/cmd/stage.rs:62`, `src/cmd/unstage.rs` equivalent). That call polls
`index.lock` with exponential backoff until the file is absent, then returns — it does not
create or hold the lock file. There is no OS-level lock held between the poll returning `Ok`
and the final `index.write()` call deep in the staging path.

The resulting read-modify-write window is:

1. wait for lock to clear (`lock::wait_for_lock_release`)
2. read index state (`diff_index_to_workdir`, `build_scan_result`)
3. create index backup (`backup::create_backup`)
4. write mutated index (`staging::stage_*` → `index.write()`)

A concurrent git operation that acquires `index.lock` between steps 1 and 4 can overwrite or
be overwritten by pgs's write, producing a silently lost update. The atomic backup (step 3)
protects against data loss — the pre-operation index state is always recoverable — but it does
not prevent the race itself.

**Current assumption**: pgs is designed for a serial driver. Only one process modifies the
index at a time (the agent issues one pgs call, waits for completion, then issues the next).
Under that assumption the TOCTOU window is never exercised. Do not run concurrent pgs
invocations or pair pgs with another tool that mutates the index without coordination.

**MCP mutation lanes**: `pgs-mcp` enforces that serial assumption for its own requests.
`PgsMcpRuntime` (`src/mcp/runtime.rs`) keys a `MutationLane` by canonicalized worktree path
and admits one `pgs_stage`/`pgs_unstage`/`pgs_commit` at a time per repository, in arrival
order. Lane slots are reserved at the transport layer before handler scheduling, so ordering
follows request arrival rather than task-scheduler nondeterminism. Read-only tools bypass the
lane entirely. This does not close the cross-process window above: a concurrent `git` process
outside pgs is still unserialized.

## Freshness invariant

`pgs stage` computes a diff/scan at call time and performs a workdir freshness
check on every file before writing the index. The check detects content drift
between an earlier `pgs scan` result and the live workdir state.

Two modes:

1. **Explicit assertion** (`--expect PATH=SHA` / MCP `expected_checksums`): the
   caller supplies a SHA-256 checksum captured from a prior scan (`checksum`
   at the file level, present in `scan --full` output). pgs compares it against
   the freshly-computed `file_checksum` in the scan result. Mismatch →
   `StaleScan` (exit 3), zero index mutation.

2. **Implicit TOCTOU guard** (no `--expect`): pgs re-hashes the workdir file
   after building the scan and compares against the scan's own checksum. Mismatch
   → `StaleScan` (exit 3). Best-effort — does not eliminate the race window but
   catches common concurrent-edit scenarios.

Both modes skip deleted files (no workdir content to hash). Mode 1 fails closed
when the scan's own checksum is empty; mode 2 skips that case.

The guard is `stage`-only. `unstage` diffs HEAD → Index and never reads the
workdir, so a workdir checksum is meaningless there.

**Drift safety by selection kind**:
- Hunk-ID selections are natively drift-safe: the ID is content-addressed, so a
  changed file yields a different ID that resolves to `unknown_hunk_id` before
  reaching the freshness guard.
- Line-range selections are only drift-safe when `--expect`/`expected_checksums`
  is supplied.
- Whole-file selections always stage current workdir content.

Implementation: `src/selection/resolve.rs::validate_freshness`; call site:
`src/cmd/stage.rs`.

## Deferred / future work

The following are known gaps that are **not** addressed in the current codebase. They are
recorded here so they are not forgotten and so maintainers can make informed decisions before
enabling concurrent or long-running use cases.

**Backup GC** — `backup::create_backup` (`src/safety/backup.rs:17`) writes two files per
operation under `.git/pgs/backups/` (a `.index` snapshot and a `.json` metadata file). There
is no TTL, count cap, or cleanup path anywhere in `src/safety/backup.rs`. In a long-lived
repository with heavy usage the backup directory grows without bound. A future implementation
should prune backups older than a configurable TTL or beyond a rolling count limit.

**Index lock during restore** — `backup::restore_backup` (`src/safety/backup.rs:56`) writes
the restored index via a plain `fs::write` with no lock file. A concurrent git process could
read or write `index` while the restore is in progress. Full atomicity would require acquiring
`index.lock` for the duration of the restore write, then releasing it — matching the protocol
libgit2 and git itself use. `PgsError::RestoreFailed` (`src/error.rs`) now surfaces restore
errors to the caller, but the lock-during-restore protocol is not yet implemented.

## Verification Expectations

Contract-sensitive changes should validate:

- text marker tests
- JSON contract tests
- parse/runtime error tests
- end-to-end text workflow tests
- `cargo test`, `cargo build`, `cargo fmt --check`, `cargo clippy -- -D warnings`
