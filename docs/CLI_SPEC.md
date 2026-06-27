# pgs CLI Specification

This document defines the output contract for `pgs` v1 markers and v1 JSON envelopes.

## Output Modes

- Default mode is text.
- JSON mode is opt-in via `--json` or `--output json`.
- Explicit `--output text` forces text.
- `--json --output text` is a user error (exit code 2).

`--help` and `--version` remain clap-native text.

## Marker Grammar (Text Mode)

Machine-significant text records use this exact syntax:

`@@pgs:v1 <kind> <minified-json-payload>`

Notes:
- Marker prefix always starts at column 0.
- Payload is minified JSON.
- Human prose is not required for machine parsing.

## Common Error Contract

All parse and runtime failures use one envelope in both modes:

```json
{
  "version": "v1",
  "command": "cli|scan|stage|unstage|status|commit",
  "phase": "parse|runtime",
  "code": "snake_case_error_code",
  "message": "human-readable detail",
  "exit_code": 1
}
```

Parse failures use:
- `command: "cli"`
- `phase: "parse"`

Runtime failures use:
- resolved command name
- `phase: "runtime"`

## Command Contracts

All successful JSON payloads include `version` and `command`.

### `scan`

JSON envelope:

```json
{
  "version": "v1",
  "command": "scan",
  "detail": "compact|full",
  "files": [
    {
      "path": "src/main.rs",
      "status": { "type": "Modified" },
      "binary": false,
      "hunks_count": 1,
      "lines_added": 2,
      "lines_deleted": 0,
      "checksum": "sha256-hex",
      "hunks": [
        {
          "id": "abc123def456",
          "old_start": 1,
          "old_lines": 2,
          "new_start": 1,
          "new_lines": 3,
          "header": "@@ -1,2 +1,3 @@",
          "additions": 1,
          "deletions": 0,
          "whitespace_only": false,
          "checksum": "...present in full only...",
          "lines": [
            { "line_number": 1, "origin": "Context", "content": "fn main() {" }
          ]
        }
      ]
    }
  ],
  "summary": {
    "total_files": 1,
    "total_hunks": 1,
    "added": 0,
    "modified": 1,
    "deleted": 0,
    "renamed": 0,
    "binary": 0
  }
}
```

Text record kinds:
- compact: `scan.begin`, `file`, `hunk`, `summary`, `scan.end`
- full: `scan.begin`, `file.begin`, `hunk.begin`, raw diff body lines, `hunk.end`, `file.end`, `summary`, `scan.end`

Hunk payload notes:
- `id` (`hunk_id`) — position-DEPENDENT addressing key: `sha256(path:old_start:new_start:content)[..12]`. Shifts when an earlier hunk changes the line count. Use to select a hunk when the scan is fresh.
- `checksum` (hunk-level) — position-STABLE content fingerprint: SHA-256 of all hunk lines (context + additions + deletions) concatenated, no positional data. Invariant under position shifts — if earlier edits push this hunk to a new line number, `checksum` survives unchanged. Agents should prefer `checksum` over `id` to re-locate a hunk after cross-hunk edits. `plan-diff` uses `checksum` for `High`-confidence shifted-hunk classification. Present in `scan --full` output only (omitted from compact hunks).
- `checksum` (file-level) — SHA-256 hex digest of the working-tree file content. Present in **both** compact and `--full` scan output. Use with `--expect`/`expected_checksums` on `stage` to detect workdir drift between scan and stage calls.
- `whitespace_only` — `true` when every `Addition`/`Deletion` line in the hunk has empty or whitespace-only trimmed content. `false` for binary hunks (binary files emit no hunks at all; the flag is meaningful only on text hunks) and for hunks that carry any non-whitespace change. The field is additive metadata; it does not enter the content-addressed hunk-ID input.
- Non-UTF-8 line content — when a diff line's raw bytes are not valid UTF-8, the `content` field in `DiffLineInfo` is set to `""` (empty string). The hunk `id` and `checksum` are computed from the same collapsed representation and remain self-consistent and selectable. The file-level `checksum` (used by `--expect` on `stage`) is computed from the raw workdir bytes and is unaffected by UTF-8 validity.

### `stage` and `unstage`

Shared operation envelope:

```json
{
  "version": "v1",
  "command": "stage|unstage",
  "status": "ok|dry_run",
  "items": [
    {
      "selection": "src/main.rs",
      "lines_affected": 7
    }
  ],
  "warnings": [],
  "backup_id": "backup-...|null",
  "previews": [
    {
      "selection": "src/main.rs:10-20",
      "file_path": "src/main.rs",
      "resolved_ranges": [{ "start": 10, "end": 20 }],
      "preview_lines": [
        { "line_number": 10, "origin": "Addition", "content": "let x = 1;" }
      ],
      "truncated": false,
      "limit_applied": 200
    }
  ]
}
```

Text record kinds:
- stage: `stage.begin`, `item`, `warning`, `stage.preview.begin`, `stage.preview.line`, `stage.preview.end`, `stage.end`
- unstage: `unstage.begin`, `item`, `warning`, `unstage.end`

#### `--dry-run --explain` preview (stage only)

`pgs stage --dry-run --explain` adds an optional `previews` array to the
operation envelope. The plain dry-run (no `--explain`) stays byte-identical
and omits the `previews` field entirely.

- Each file in the selection produces one `OperationPreview` entry.
  `preview_lines` contains the exact Addition lines that would land in the
  index, in file order.
- `--limit <N>` (default `200`, `0` = unlimited) caps each file independently;
  when a file exceeds its cap, `truncated` flips to `true` for that file only —
  never aggregated across files.
- Binary files short-circuit: `preview_lines: []`, `truncated: false`,
  `reason: "binary"`. Binary content is never rendered.
- Text markers: `stage.preview.begin` (with `selection`, `file_path`, `lines`,
  `truncated`, `limit_applied`, and optional `reason`), one
  `stage.preview.line` per row (with `file_path`, `line_number`, `origin`,
  `content`), then `stage.preview.end`. Binary entries emit begin/end with no
  `stage.preview.line` records between them.

#### Freshness guard and drift detection (stage only)

`pgs stage` validates that each staged file's workdir content has not changed
between the caller's scan and this call. A file that changed on disk after the
scan produces a `StaleScan` error (exit 3) with zero index mutation — no backup
is created, the index is never touched. Re-run `pgs scan` to obtain fresh hunk
IDs and checksums before retrying.

**`--expect <path>=<sha>` (CLI) / `expected_checksums` (MCP)**

The caller asserts a per-file SHA-256 checksum captured from a prior
`pgs scan` output (the file-level `checksum` field, present in both compact
and `--full` scan envelopes). When supplied, pgs compares the asserted checksum against the
checksum in the fresh scan. A mismatch returns `StaleScan` (exit 3)
immediately.

- CLI flag: `--expect src/main.rs=<sha256>`, repeatable once per file. Format
  is exactly `PATH=SHA`; a missing `=` separator is a user error (exit 2).
  Duplicate entries for the same path are a user error (exit 2).
- MCP field: `expected_checksums` — optional `HashMap<String, String>` (path →
  lowercase SHA-256 hex). Omit the field or pass an empty map to skip explicit
  checksum assertion.
- `--expect` paths must be part of the staging selection. Naming a path not in
  the selection is a user error (exit 2).
- Supplying `--expect` on a deleted file is a user error (exit 2): deleted
  files have no workdir content and therefore no checksum.
- When the scan produced an empty `checksum` for a file and `--expect` is
  supplied, pgs fails closed with `StaleScan` (exit 3).

**Implicit TOCTOU guard (no `--expect`)**

Even without `--expect`, pgs re-hashes the workdir file after computing the
scan and compares against the scan's own checksum. A mismatch returns `StaleScan`
(exit 3). This is best-effort and does not eliminate the race window, but it
catches common concurrent-edit scenarios. The implicit guard is skipped for
deleted files and files where the scan produced an empty checksum.

**Which selection kinds are drift-safe**

| Selection kind | Drift behavior |
|----------------|----------------|
| Hunk ID (`abc123def456`) | Natively drift-safe: the ID is content-addressed (`sha256(path:old_start:new_start:content)`). A changed file produces different hunk IDs; the call fails with `unknown_hunk_id` (exit 2) before reaching the freshness guard. |
| Line range (`path:A-B`) | Not natively drift-safe: a changed file may retain the same line numbers while pointing to different content. Supply `--expect`/`expected_checksums` to make line-range staging fail-safe on drift. |
| Whole-file (`path`) | Always stages current workdir content. The implicit TOCTOU guard still applies; `--expect` makes it strict. |

**Why the drift guard is `stage`-only**

`unstage` diffs HEAD → Index and never reads the workdir. A workdir checksum is
meaningless there, and the index is exclusively pgs's own prior output, so
content drift between a scan and an unstage call cannot occur in the same way.
No freshness guard is applied to `unstage`.

#### Non-UTF-8 partial staging

Partial staging (hunk or line-range selection) of a file whose workdir content
is not valid UTF-8 returns `NonUtf8Partial` (exit 2) directing the caller to
use whole-file staging instead. The rejection is conservative: the line-diff
engine (`similar::TextDiff`) would silently replace invalid bytes with U+FFFD,
producing a corrupted staged blob.

Whole-file staging of a non-UTF-8 file is fully supported; pgs stages the raw
bytes without encoding transformation.

### `status`

JSON envelope:

```json
{
  "version": "v1",
  "command": "status",
  "files": [
    {
      "path": "src/main.rs",
      "status": { "type": "Modified" },
      "lines_added": 3,
      "lines_deleted": 1
    }
  ],
  "summary": {
    "total_files": 1,
    "total_additions": 3,
    "total_deletions": 1
  }
}
```

Text record kinds:
- `status.begin`, `status.file`, `summary`, `status.end`

### `commit`

`pgs commit -m ...` creates a new child commit from staged changes. `pgs commit
--amend -m ...` rewrites the current `HEAD` commit with the current index tree
and message; message-only amend is allowed when the index already matches
`HEAD`.

JSON envelope:

```json
{
  "version": "v1",
  "command": "commit",
  "commit_hash": "40-char sha",
  "message": "feat: ...",
  "author": "Name <email>",
  "files_changed": 1,
  "insertions": 2,
  "deletions": 0
}
```

Text record kinds:
- `commit.result`

### `log`

Read commit history. Returns up to `--max-count` commits (default 20) walking
from HEAD. Optional path filters restrict output to commits that touch the
given files.

JSON envelope:

```json
{
  "version": "v1",
  "command": "log",
  "commits": [
    {
      "hash": "40-char-sha",
      "short_hash": "12-char prefix",
      "author": "Name <email>",
      "date": "2024-01-15T10:30:00+00:00",
      "message": "feat: add feature"
    }
  ],
  "total": 1,
  "truncated": false
}
```

`truncated` is `true` when the walk hit the internal 1000-commit safety cap
before reaching `--max-count`. `total` reflects the number of commits actually
returned.

Text record kind: `log` (single record containing the full envelope).

Exit codes:
- `0`: success (including an empty-history repo, which returns `commits: []`).
- `1`: `NoChanges` — not applicable; `log` always returns `0` on success.
- `2`: user error (invalid arguments).

MCP tool: `pgs_log` — read-only, `task_support: Optional`. Input schema
requires `repo_path`; optional `max_count` (default 20) and `paths` (array
of file paths to filter by). The full `LogOutput` envelope is returned inside
`structuredContent.pgs`.

### `overview`

Unified view of unstaged (scan) and staged (status) changes. Composes the
existing handlers without re-implementing any git logic.

JSON envelope:

```json
{
  "version": "v1",
  "command": "overview",
  "unstaged": { "...ScanOutput compact shape..." : null },
  "staged":   { "...StatusOutput shape..."         : null }
}
```

The `unstaged` section mirrors a compact `scan` envelope; the `staged`
section mirrors a `status` envelope. Both are always present; if either
side is empty, that section carries an empty `files` array and zeroed
summary.

Text record kinds:
- `overview.begin`, followed by the full `scan.*` block, followed by the
  full `status.*` block, followed by `overview.end`.

`overview` returns `NoChanges` (exit code 1) only when both the scan and
status sides are empty.

### `split-hunk`

Classify a single hunk's contiguous line runs as `addition`, `deletion`, or
`mixed`. Descriptive — the command does not stage or unstage anything; it
names the categories inside the hunk so the agent can decide how to act.

```
pgs split-hunk <hunk_id>
```

JSON envelope:

```json
{
  "version": "v1",
  "command": "split",
  "hunk_id": "abc123def456",
  "ranges": [
    { "start": 10, "end": 12, "origin_mix": "addition" },
    { "start": 15, "end": 18, "origin_mix": "mixed" }
  ]
}
```

`ranges` carries one entry per maximal run of non-context lines inside the
hunk. `origin_mix` values:

- `addition` — the run is only `Addition` lines.
- `deletion` — the run is only `Deletion` lines.
- `mixed` — the run interleaves additions and deletions.

Text record kinds:
- `split.begin`, one `split.range` per classified range, `split.end`.

Errors: `unknown_hunk_id` (exit 2) when the 12-hex id is absent from the
fresh scan (including content drift since the user captured the id).
Binary files expose no granular hunks, so any hunk id probe against a
binary file also returns `unknown_hunk_id` with guidance to re-run `pgs
scan`.

MCP tool: `pgs_split_hunk` — read-only, `task_support: Optional`. Input
schema requires `repo_path` and `hunk_id`; optional `context` defaults to 3.

### `plan-check`

Validate an agent-supplied `CommitPlan` against a fresh scan. Descriptive — the
command never stages, unstages, or commits. It reports how the plan's selectors
overlap, miss, or straddle boundaries relative to the current scan so the agent
can fix the plan before executing it.

```
pgs plan-check [--plan <path> | --stdin]
```

Input schema (JSON on stdin, default, or read from `--plan <path>`):

```json
{
  "version": "v1",
  "commits": [
    {
      "id": "optional-label",
      "selections": ["src/main.rs", "abc123def456", "src/lib.rs:10-20"],
      "exclude": [],
      "message": "optional commit message preview"
    }
  ]
}
```

All fields other than `commits` and `selections` are optional and defaulted via
`#[serde(default)]`. In particular, `version` is **optional** and defaults to
`"v1"` when omitted — agents following the MCP schema (which does not advertise
`version`) may omit it and the input will parse correctly. Plans that include
`"version":"v1"` continue to work unchanged. pgs only receives `CommitPlan`
(never emits one), so unknown input fields are silently tolerated. A6
`plan-diff` extends the schema additively (`captured_at`, `captured_hunk_id`,
`expected_checksum`) without bumping `version`.

JSON envelope:

```json
{
  "version": "v1",
  "command": "plan-check",
  "overlaps": [
    { "hunk_id": "abc123def456", "commits": ["commitA", "commitB"] }
  ],
  "uncovered": [
    { "file_path": "src/main.rs", "hunk_id": "deadbeefcafe" }
  ],
  "unsafe_selectors": [
    { "commit_id": "wide", "selection": "src/main.rs:1-40", "reason": "spans_hunk_boundary" }
  ],
  "unknown_paths": ["does/not/exist.rs"],
  "unknown_hunk_ids": [
    { "commit_id": "stale-commit", "hunk_id": "deadbeef0000" }
  ]
}
```

`unsafe_selectors[*].reason` values currently emitted:
- `spans_hunk_boundary` — a `path:A-B` range intersects two or more hunks.
- `invalid_selection` — the selector failed positional auto-detection.

`unknown_paths` carries only genuine file/directory path misses (the path string
is not present in the current scan at all). `unknown_hunk_ids` carries stale
12-hex hunk ids that were valid in a prior scan but are absent from the current
one — the agent should re-run `pgs scan` to obtain a fresh id rather than
treating this as a path problem. `commit_id` in each `unknown_hunk_ids` entry is
`null`/absent when the referencing planned commit had no `id` field.

Text record kinds:
- `plan.check.begin`, `plan.check.overlap`, `plan.check.uncovered`,
  `plan.check.unsafe`, `plan.check.unknown`, `plan.check.unknown_hunk`,
  `plan.check.end`.

Exit codes:
- `0` on a clean plan (every report array empty).
- `1` when any issue is reported (including non-empty `unknown_hunk_ids`) —
  the plan is rejected but nothing is mutated.
- `2` on malformed plan JSON or an unreadable `--plan` path.

MCP tool: `pgs_plan_check` — read-only, `task_support: Optional`. Input schema
requires `repo_path` and `plan` (inline `CommitPlan` object); optional
`context` defaults to 3. The full `PlanCheckOutput` envelope is returned
inside `structuredContent.pgs`.

### `plan-diff`

Reconcile a saved `CommitPlan` against a fresh scan. Descriptive — never
mutates the repository. Classifies each planned selection as one of
`still_valid`, `shifted`, or `gone` so the agent can tell whether a saved
plan still applies after intervening edits or commits.

```
pgs plan-diff [--plan <path> | --stdin]
```

Input schema: same `CommitPlan` accepted by `plan-check`. plan-diff uses
the A6 additive fields — `captured_at`, per-commit `captured_hunk_id`, and
per-commit `expected_checksum` — when present to raise match confidence.

JSON envelope:

```json
{
  "version": "v1",
  "command": "plan-diff",
  "still_valid": [
    {
      "commit_id": "c1",
      "selection": "src/main.rs",
      "file_path": "src/main.rs",
      "hunk_id": "abc123def456"
    }
  ],
  "shifted": [
    {
      "commit_id": "c1",
      "selection": "oldhunkid0000",
      "file_path": "src/main.rs",
      "old_hunk_id": "oldhunkid0000",
      "new_hunk_id": "newhunkid1234",
      "match_confidence": "high"
    },
    {
      "commit_id": "c2",
      "selection": "src/lib.rs:5-10",
      "file_path": "src/lib.rs",
      "new_hunk_id": "aabbcc112233",
      "match_confidence": "medium"
    }
  ],
  "gone": [
    {
      "commit_id": "c1",
      "selection": "removed.rs",
      "file_path": "removed.rs",
      "reason": "path_missing"
    }
  ]
}
```

`match_confidence` values: `high` (checksum match against a live hunk),
`medium` (≥50% old-range overlap with a live hunk). Shifted entries never
auto-upgrade to `still_valid` — callers reconcile explicitly.

`shifted[*].old_hunk_id` is optional: it is omitted when a checksum-only
match drove the shift (no `captured_hunk_id` was set on the planned commit).

`still_valid[*].reason` is optional. When present and set to
`no_unstaged_hunks`, the selection resolved to a file whose unstaged hunk
list is now empty (e.g. everything staged already); the caller may skip it.

`gone[*].reason` values currently emitted:
- `path_missing` — referenced path no longer present in the scan.
- `covered_by_commit` — all unstaged hunks are gone from the file (consumed by a prior stage/commit).
- `invalid_selection` — selector failed positional auto-detection.
- `no_match` — a captured hunk id could not be fuzzy-matched to any live hunk.

Text record kinds:
- `plan.diff.begin`, `plan.diff.valid`, `plan.diff.shifted`, `plan.diff.gone`,
  `plan.diff.end`.

Exit codes:
- `0` when every entry is `still_valid` (`shifted` and `gone` both empty).
- `1` when any entry shifted or went gone — the plan needs reconciliation.
- `2` on malformed plan JSON or an unreadable `--plan` path.

MCP tool: `pgs_plan_diff` — read-only, `task_support: Optional`. Input
schema requires `repo_path` and `plan` (inline `CommitPlan` object);
optional `context` defaults to 3. The full `PlanDiffOutput` envelope is
returned inside `structuredContent.pgs`.

## Selection Syntax

Selections are positional and auto-detected:
- `src/main.rs` -> file selection
- `abc123def456` -> hunk selection (12 hex)
- `src/main.rs:10-20,30-40` -> line-range selection

## Exit Codes

- `0`: success
- `1`: no effect (for example `NoChanges`, `SelectionEmpty`)
- `2`: user error
- `3`: conflict/retryable failure
- `4`: internal error
