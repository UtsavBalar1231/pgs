# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Programmatic git staging CLI for AI agents. Text-default output with explicit JSON opt-in, no interactive UI.

## Build & Test

Requires Rust 1.85+ (edition 2024) and a C compiler (for libgit2).

```
cargo build                        # compile
cargo test                         # all tests (unit + integration)
cargo test --lib                   # unit tests only
cargo test --test '*'              # integration tests only
cargo test test_name               # run a single test by name
cargo clippy -- -D warnings        # lint (zero warnings required)
cargo fmt --check                  # format check
cargo install --path .             # install locally
```

Clippy is configured strictly in Cargo.toml: `deny all`, `warn pedantic+nursery`. Allowed exceptions: `must_use_candidate`, `module_name_repetitions`, `missing_errors_doc`, `option_if_let_else`, `too_many_lines`, `missing_panics_doc`.

## Architecture

```
src/
  main.rs          — entry, clap args, delegates to cmd::run()
  lib.rs           — pub mod declarations
  error.rs         — AgstageError enum (14 variants), exit_code() mapping
  models.rs        — all serializable types (ScanResult, StageResult, etc.)

  cmd/             — command handlers (one file per command)
    mod.rs         — Cli struct (clap derive), Command enum, run() dispatcher
    scan.rs        — scan handler
    stage.rs       — stage handler
    unstage.rs     — unstage handler
    status.rs      — status handler
    commit.rs      — commit handler

  git/             — all git2 operations (no subprocess calls)
    mod.rs         — shared helpers (build_index_entry, read_head_blob, read_index_blob)
    repo.rs        — repository discovery (open, workdir)
    diff.rs        — diff engine (index-to-workdir, HEAD-to-index)
    staging.rs     — index-direct staging (stage_file, stage_lines, stage_hunk, stage_deletion, stage_rename)
    unstaging.rs   — index-direct unstaging (unstage_file, unstage_lines, unstage_hunk)

  selection/       — selection parsing + resolution
    parse.rs       — auto-detect positional args (file/hunk/lines)
    resolve.rs     — resolve specs, validate binary/whole-file/freshness

  safety/          — lock detection + index backup
    lock.rs        — is_index_locked(), wait_for_lock_release()
    backup.rs      — create_backup(), restore_backup()
```

main.rs is thin: parse args, call cmd::run(), render typed result or error output, set exit code.
All logic in lib.rs and modules so integration tests use the library crate.

**Index-direct staging only**: All staging/unstaging uses direct blob construction via `similar::TextDiff` line diffing and `index.add_frombuffer()`. No patch-apply strategy.

**Always atomic**: Stage/unstage operations create a mandatory index backup, stop on first failure, and restore the backup on error.

**Critical diff bases**: `scan` uses Index->Workdir (excludes already-staged content). `status` uses HEAD->Index (shows what's staged). `unstage` matches against HEAD->Index.

**Selection auto-detection**: Positional args are auto-detected as file path, 12-hex hunk ID, or path:range. No `--file`/`--hunk`/`--lines` flags.

Read @docs/ARCHITECTURE.md before modifying module boundaries.
Read @docs/CLI_SPEC.md before adding/changing any CLI flag or output contract field.

## Anti-Hallucination Rules (MANDATORY)

1. Never invent crate features. Verify a feature exists in the crate's Cargo.toml or docs.rs before using it.
2. Never invent API methods. Verify a method exists on a type before calling it. If unsure, use the simplest known-correct pattern.
3. When uncertain, write a failing test first. The compiler is the source of truth, not training data.
4. Do not generate code that depends on nightly-only features.

## Error Handling

- All errors use thiserror enums in src/error.rs. NO anyhow in this project.
- Each error variant maps to an exit code: 0 success, 1 no-effect, 2 user error, 3 conflict, 4 internal.
- Never use .unwrap() or .expect() in library code. Propagate with ?.
- .expect("reason") allowed ONLY in main.rs or tests, and only for programmer-error invariants.
- Error messages must include context: what failed, on what input.
  Good: "failed to parse hunk header in src/foo.rs: expected @@ prefix"
  Bad: "parse error"

## Testing (TDD Enforced)

For every new function:
1. Write a failing test first (cargo test confirms RED)
2. Write minimum code to make it GREEN
3. Refactor only if tests still pass

### Test naming: <thing>_<scenario>_<expected>
  parse_hunk_header_with_zero_context_returns_correct_range
  stage_single_line_in_modified_file_updates_index
  scan_empty_repo_returns_empty_files_list

### Shared test helpers (tests/common/mod.rs):
- `setup_repo()` -> `(TempDir, Repository)` with git identity and initial commit
- `write_file(repo, path, content)` -> write file to working directory
- `commit_file(repo, path, content, message)` -> write, add, commit
- `run_agstage(repo, args)` -> run the CLI binary with `--repo` pointed at the test repo

## Output Contract

Default mode is text with marker records:

`@@agstage:v1 <kind> <minified-json-payload>`

JSON is opt-in via `--json` or `--output json`.

Parse and runtime failures share the same envelope fields:

- `version`
- `command`
- `phase`
- `code`
- `message`
- `exit_code`

Parse failures use `command: "cli"` with `phase: "parse"`; runtime failures use resolved command + `phase: "runtime"`.

### E2E CLI tests:
`tests/test_e2e_cli.rs` uses `assert_cmd` for binary-level testing. Library-level integration tests are in `tests/test_*.rs`.

### Preventing flaky tests:
- No sleeps or hardcoded timeouts
- No shared mutable state between tests — each test creates its own TempDir
- No filesystem paths outside the temp dir
- No network calls
- No reliance on test execution order
- Deterministic inputs only

### Integration test pattern:
  fn setup_repo() -> (TempDir, Repository) {
      let dir = TempDir::new().unwrap();
      let repo = Repository::init(dir.path()).unwrap();
      let mut config = repo.config().unwrap();
      config.set_str("user.name", "Test").unwrap();
      config.set_str("user.email", "test@test.com").unwrap();
      // ... create initial commit ...
      (dir, repo)  // dir must live as long as repo
  }

## Code Quality

### Comments
- Doc comments (///) on every public item. Describe what + why. Include # Errors section.
- Inline comments (//) only to explain WHY, never WHAT.
- Never: "// create a new vector", "// return the result", "// iterate over hunks"
- TODO/FIXME only with issue number: // TODO(#42): handle binary files

### Abstraction
- No traits with single implementations. Use the concrete type.
- No wrapper types that only delegate. Use the inner type.
- No builder patterns for structs with <4 fields. Use a constructor.
- No "util" or "helpers" modules. Name modules after what they do.
- Don't split files until a module exceeds ~300 lines.
- Prefer free functions over methods when self is not needed.

### Structure
- Functions do one thing. If description has "and", split it.
- Max 4 function parameters. Use options struct beyond that.
- Prefer &str over String in parameters. Accept borrows, return owned.
- Prefer exhaustive match over if-let chains for >2 variants.
- Avoid clone() unless the borrow checker fight isn't worth it. Document why.

## Dependencies (approved list)

- clap (derive) — CLI parsing
- git2 — libgit2 bindings
- serde + serde_json — JSON serialization
- thiserror — error derivation
- sha2 — content checksums
- similar — line-level diffing for index-direct staging
- uuid — backup IDs
- chrono — timestamps
- tempfile, assert_cmd, predicates, proptest — dev only

Adding a new dep requires: what it does, why hand-written is worse, >100 downloads/day on crates.io.

## Git Conventions

- Conventional commits: feat:, fix:, test:, refactor:, docs:, chore:
- One logical change per commit. Don't mix features with refactors.
- Imperative mood, no period, under 72 chars.
