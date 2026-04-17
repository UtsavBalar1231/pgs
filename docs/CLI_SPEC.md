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
  "backup_id": "backup-...|null"
}
```

Text record kinds:
- stage: `stage.begin`, `item`, `warning`, `stage.end`
- unstage: `unstage.begin`, `item`, `warning`, `unstage.end`

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
