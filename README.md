# pgs

Programmatic git staging CLI for AI agents. Text-first output, explicit JSON opt-in, no interactive UI.

`pgs` lets coding agents stage git changes at file, hunk, and line-range granularity without `git add -p`.
It is machine-parseable in both modes:
- text (default): marker records beginning with `@@pgs:v1`
- json (opt-in): `--json` or `--output json`

Compact scan is default for `scan`; use `--full` only when line-level diff content is required.

## Output Contract

Text mode uses this exact grammar for machine-significant records:

`@@pgs:v1 <kind> <minified-json-payload>`

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
pgs scan

# Full scan with diff body framed by markers
pgs scan --full

# JSON mode (explicit opt-in)
pgs --json scan

# Stage by file, hunk ID, or line range (auto-detected positional syntax)
pgs stage src/lib.rs
pgs stage abc123def456
pgs stage src/lib.rs:10-20,30-40

# Commit staged changes
pgs commit -m "feat: add feature X"
```

## Build

```bash
cargo build
cargo test
cargo clippy -- -D warnings
cargo fmt --check
```

See `docs/CLI_SPEC.md` for the complete contract and `docs/ARCHITECTURE.md` for system design.
