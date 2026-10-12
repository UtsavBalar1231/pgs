# Contributing to pgs

## Build and test

You need Rust 1.88+ (edition 2024) and a C compiler for libgit2.

```bash
cargo build                                  # compile
cargo test                                   # unit and integration tests
cargo test --lib                             # unit tests only
cargo test --test '*'                        # integration tests only
cargo test test_name                         # one test by name
cargo clippy --all-targets -- -D warnings    # lint, zero warnings
cargo fmt --check                            # format check
```

Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and
`cargo test` before you open a pull request. CI runs the tests on Linux, macOS,
and Windows, so a change must pass on all three. Do not rely on a shell,
a path separator, or a git configuration that exists on only one of them.

## How to change code

- Write the failing test first, make it pass with the least code, then refactor
  with the tests green. Name tests `<thing>_<scenario>_<expected>`, for example
  `stage_single_line_in_modified_file_updates_index`.
- Tests share helpers in `tests/common/mod.rs`. Each test makes its own
  `TempDir`; bind the `TempDir` before the `Repository` so the directory outlives
  it. No sleeps, no network, no paths outside the temp directory.
- All git operations go through libgit2. Do not spawn `git` from `src/`.
- Errors are `thiserror` variants in `src/error.rs`, each with a code and an exit
  code. No `anyhow`, and no `unwrap` or `expect` in library code.
- A change to a CLI flag or an output field must update `docs/CLI_SPEC.md` in the
  same pull request. Read `docs/ARCHITECTURE.md` before changing module
  boundaries.

[CLAUDE.md](CLAUDE.md) and [AGENTS.md](AGENTS.md) hold the full conventions and
the module map.

## Commits and pull requests

Use conventional commits (`feat`, `fix`, `test`, `refactor`, `docs`, `chore`),
imperative mood, no trailing period, subject under 72 characters. Put the reason
for the change in the body. Keep one logical change per commit; do not mix a
feature with a refactor.

## Security issues

Do not open a public issue for a vulnerability. See [SECURITY.md](SECURITY.md).
