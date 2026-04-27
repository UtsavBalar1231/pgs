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
- `src/output/view.rs` defines shared models used by both text and JSON renderers.
- `src/output/text.rs` renders v1 marker records.
- `src/output/json.rs` renders the same models as JSON envelopes.
- `src/main.rs` handles parsing, dispatch, error capture, and output dispatch.

## Output Contracts

All command outputs are versioned with `version: "v1"`.

Text mode uses the exact marker grammar:

`@@pgs:v1 <kind> <minified-json-payload>`

Recognized marker kinds:

- scan: `scan.begin`, `file`, `hunk`, `summary`, `scan.end`
- scan full: `file.begin`, `hunk.begin`, raw diff body, `hunk.end`, `file.end`, `scan.end`
- stage: `stage.begin`, `item`, `warning`, `stage.end`
- unstage: `unstage.begin`, `item`, `warning`, `unstage.end`
- status: `status.begin`, `status.file`, `summary`, `status.end`
- commit: `commit.result`
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

## Symlink staging

`pgs` uses `symlink_metadata()` + `read_link()` to detect and read symlinks; it never follows
the link to the target file. Index blobs for symlinks contain the raw link-target string bytes
(mode `0o120000`), not the target file's contents. The single point of truth for all workdir
blob reads is the `read_workdir_for_blob` helper in `src/git/mod.rs`; all staging call sites
route through it.

## Verification Expectations

Contract-sensitive changes should validate:

- text marker tests
- JSON contract tests
- parse/runtime error tests
- end-to-end text workflow tests
- `cargo test`, `cargo build`, `cargo fmt --check`, `cargo clippy -- -D warnings`
