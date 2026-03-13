# agstage

Programmatic git staging CLI for AI agents. Text-first output, explicit JSON opt-in, no interactive UI.

`agstage` lets coding agents stage git changes at file, hunk, and line-range granularity without `git add -p`.
It is machine-parseable in both modes:
- text (default): marker records beginning with `@@agstage:v1`
- json (opt-in): `--json` or `--output json`

Compact scan is default for `scan`; use `--full` only when line-level diff content is required.

## Output Contract

Text mode uses this exact grammar for machine-significant records:

`@@agstage:v1 <kind> <minified-json-payload>`

Examples of record kinds:
- `scan.begin`, `file`, `hunk`, `summary`, `scan.end`
- `stage.begin`, `item`, `warning`, `stage.end`
- `unstage.begin`, `item`, `unstage.end`
- `status.begin`, `status.file`, `status.end`
- `commit.result`
- `error`

Errors are structured in both text and JSON modes with:
`version`, `command`, `phase`, `code`, `message`, `exit_code`.

Parse failures use `command: "cli"` + `phase: "parse"`; runtime failures use the real command and `phase: "runtime"`.

## Quick Start

```bash
# Text default (marker output)
agstage scan

# Full scan with diff body framed by markers
agstage scan --full

# JSON mode (explicit opt-in)
agstage --json scan

# Stage by file, hunk ID, or line range (auto-detected positional syntax)
agstage stage src/lib.rs
agstage stage abc123def456
agstage stage src/lib.rs:10-20,30-40

# Commit staged changes
agstage commit -m "feat: add feature X"
```

## Build

```bash
cargo build
cargo test
cargo clippy -- -D warnings
cargo fmt --check
```

See `docs/CLI_SPEC.md` for the complete contract and `docs/ARCHITECTURE.md` for system design.
