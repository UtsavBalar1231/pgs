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
      "checksum": "...optional in full...",
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
          "checksum": "...optional in full...",
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
- `whitespace_only` — `true` when every `Addition`/`Deletion` line in the hunk has empty or whitespace-only trimmed content. `false` for binary hunks (binary files emit no hunks at all; the flag is meaningful only on text hunks) and for hunks that carry any non-whitespace change. The field is additive metadata; it does not enter the content-addressed hunk-ID input.

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
