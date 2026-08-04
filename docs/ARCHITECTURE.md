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

### How a line range reaches a deleted line

A line range is expressed in workdir (new-file) coordinates. That is the only
coordinate system the caller can observe — the caller reads the workdir, not the
index blob — but it has no number for a line that the edit removed. Deletions
therefore cannot be selected directly; they have to be attached to something the
range *can* name, and the attachment rules are what keep partial staging
lossless.

**Replace runs pair by content, then by position.** When deletions are
immediately followed by additions, the run is a replacement, and each deletion is
bound to one addition: the deletion is applied only when its partner addition is
selected. The partner is the first addition in the run with identical content;
failing that, the addition at the same index; failing that — a deletion past the
end of the addition list — the run's last addition. Without any pairing,
selecting one addition out of a multi-line replacement would either drop every
original line in the run (data loss) or keep every one of them (a duplicated
block). Pairing makes an unselected original survive in the staged blob
unchanged.

Content comes first because positional pairing alone destroys a line in a case
that occurs routinely. `similar` tokenizes lines *with* their terminator, so a
file whose last line had no trailing newline and regains one appears in the diff
as a `-b` / `+b` pair, not as an unchanged line. Positionally, that deletion of
`b` binds to whatever addition happens to sit at its index — an unrelated
neighbouring insertion — so selecting that insertion applied the deletion of `b`
and `b` vanished from the staged blob. Matching content first binds `-b` to `+b`,
which is the only pairing that means anything. git models the same edit the same
way; this is not a `similar` quirk to work around, it is the shape of the diff.

**Pure-deletion runs need both survivors.** A run of deletions with no additions
has nothing to pair with. It occupies the gap between the surviving line before
it and the surviving line after it, and it is applied only when the range covers
a survivor on each side of that gap. Requiring both sides is what makes the
selection unambiguous: covering one side alone cannot distinguish "stage the
deletion that follows this line" from "stage this line and stop". At the start
or end of the file the gap has only one side, so the rule degrades to the
adjacent survivor plus at least one further line on that side; when no further
line exists the adjacent survivor alone suffices, so the deletion stays
reachable rather than becoming unstageable.

Three invariants fall out of these rules and are the cheapest way to check them:

- A range naming only unchanged lines never mutates the index. It resolves to no
  changed lines and returns `SelectionEmpty` (exit 1) rather than reporting a
  successful no-op, so a caller cannot mistake an inert selection for applied
  work.
- Staging the full range `1-N` reproduces the workdir byte-for-byte. Every
  addition is selected, so every paired deletion applies and every gap has both
  survivors covered.
- A partial stage never destroys content present in both the base and the
  workdir. A line that both sides agree on survives the operation regardless of
  which subset the range names.

The third invariant is the one that catches real defects, because the first two
describe only the extremes: staging *no* changed line and staging *every*
changed line. Neither can reach a strict subset, which is exactly where content
gets destroyed — three separate data-loss bugs satisfied both of the original
invariants.

**Unterminated interior lines are refused, not repaired.** Diff tokens carry
their own terminator, so a token without a trailing newline can only be a file's
last line. After the result blob is assembled, pgs checks that no element other
than the final one lacks a terminator; a violation returns
`UnterminatedInteriorLine` (code `unterminated_interior_line`, exit 2) and the
index is untouched.

The check is a post-condition on the assembled blob, not a rule about the shape
of the input, and that is what makes it incapable of false-positives: any
selection that assembles into a well-formed file passes it, whatever route it
took to get there.

It refuses rather than auto-fixes because the fix is a change the caller never
named. Placing content after a base file's unterminated last line structurally
requires terminating that line too — one extra byte on a line outside the
selection. Silently staging a change nobody asked for is the precise failure
mode pgs exists to prevent, so the operation stops and names the alternative.
Whole-hunk and whole-file staging of the same file remain byte-exact.

**The trailing newline is derived, not imposed.** The result's final byte comes
from whichever token was emitted last. The index/workdir trailing-newline
convention is applied only when that final token came from the mutating side —
the workdir when staging, the index when unstaging. When the final token is a
preserved base line instead, its bytes are already correct and are left alone.
Imposing the convention unconditionally is what appended a phantom newline onto
a preserved base last line (making a staged file read as still-modified) and
what stripped one from a legitimately terminated result.

Hunk-ID and whole-file selections bypass all of this: both include every line of
their target, deletions included.

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
