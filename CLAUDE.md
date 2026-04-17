# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Non-interactive git staging at file, hunk, and line granularity.

**Module map & per-subsystem guides**: see @AGENTS.md (full `src/` tree, links to `src/git/AGENTS.md`, `src/mcp/AGENTS.md`, `tests/AGENTS.md`).

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

## Architecture Invariants

Full module map lives in @AGENTS.md. The rules below are what isn't obvious from reading the code:

- **Index-direct staging only**: all staging/unstaging builds blobs via `similar::TextDiff` line diffing and `index.add_frombuffer()`. No patch-apply. No `std::process::Command` anywhere in `src/` — all git operations go through libgit2.
- **Atomic stage/unstage**: every mutation creates a mandatory index backup, stops on first failure, and restores the backup on error.
- **Critical diff bases** — getting these wrong silently produces wrong results:
  - `scan` uses Index → Workdir (excludes already-staged content)
  - `status` uses HEAD → Index (shows what's staged)
  - `unstage` matches against HEAD → Index
- **Selection auto-detection**: positional args are auto-detected as file path, 12-hex hunk ID, or `path:range`. No `--file`/`--hunk`/`--lines` flags.
- **MCP server reuses CLI handlers**: `src/mcp/` (binary `pgs-mcp`) dispatches through `src/cmd/mcp_adapter.rs` to the same command handlers as the CLI. Never reimplement git, selection, or output logic inside `src/mcp/`.

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
  `"failed to parse hunk header in src/foo.rs: expected @@ prefix"` not `"parse error"`.

## Testing (TDD Enforced)

1. Write a failing test first (RED)
2. Minimum code to pass (GREEN)
3. Refactor only if tests still pass

### Test naming: <thing>_<scenario>_<expected>
  parse_hunk_header_with_zero_context_returns_correct_range
  stage_single_line_in_modified_file_updates_index
  scan_empty_repo_returns_empty_files_list

### Shared test helpers (tests/common/mod.rs):
- `setup_repo()` -> `(TempDir, Repository)` with git identity and initial commit
- `write_file(repo, path, content)` -> write file to working directory
- `commit_file(repo, path, content, message)` -> write, add, commit
- `run_pgs(repo, args)` -> run the CLI binary with `--repo <dir> --json` (use when asserting on JSON)
- `run_pgs_raw(repo, args)` -> run the CLI binary with `--repo <dir>` only (text mode, for marker-contract tests)

## Output Contract

Default: text markers `@@pgs:v1 <kind> <json>`. JSON: opt-in via `--json`.
Errors: shared envelope with `version`, `command`, `phase`, `code`, `message`, `exit_code`.
Parse failures use `command: "cli"` + `phase: "parse"`. Runtime failures use the resolved command + `phase: "runtime"`.

See @docs/CLI_SPEC.md for the full contract.

### E2E tests:
`tests/test_e2e.rs` and `tests/test_output_modes.rs` use `assert_cmd`. Library-level integration tests are in `tests/test_*.rs`.

### Preventing flaky tests:
- No sleeps or hardcoded timeouts
- No shared mutable state — each test creates its own `TempDir`. **`TempDir` must outlive `Repository`** (bind `dir` before `repo` when destructuring, or the workdir gets deleted while the repo still references it)
- No filesystem paths outside the temp dir
- No network calls

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
- Doc comments (`///`) on every public item. Describe what + why. Include `# Errors` section.
- Inline comments only to explain WHY, never WHAT.
- TODO/FIXME only with issue number: `// TODO(#42): handle binary files`

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

## Dependencies

Full list lives in `Cargo.toml`. Adding a new dep requires: what it does, why hand-written is worse, >100 downloads/day on crates.io.

## Git Conventions

Conventional commits (feat/fix/test/refactor/docs/chore), imperative mood, no period, under 72 chars. One logical change per commit — don't mix features with refactors.
