# agstage

Programmatic git staging CLI for AI agents. JSON in/out, no interactive UI.

**agstage** enables AI coding agents to stage git changes at file, hunk, and line-range granularity through a pure CLI interface. It replaces the interactive `git add -p` workflow with machine-readable commands and structured JSON output, allowing agents to make precise, atomic commits without human intervention.

## Why agstage?

AI agents need to make precise, atomic git commits but lack tools for partial staging:
- **Non-Interactive**: `git add -p` is interactive and unusable by automated agents.
- **Programmatic**: `git apply --cached` requires manually constructing complex unified diffs.
- **Stable Referencing**: Content-based hunk IDs (SHA-256) ensure stable referencing across rescans, unlike line numbers which shift.
- **Safe**: Built-in index backups, lock detection, and audit logging prevent repository corruption.

## Technical Architecture

agstage is built on a **pure libgit2** foundation (via the `git2` crate), avoiding subprocess calls and shell injection risks.

### Dual Staging Strategy
agstage employs two distinct strategies for index manipulation:
1. **Patch-Apply (Default)**: Generates a synthetic unified diff of selected changes and applies it to the index. This is robust and matches standard git behavior.
2. **Index-Direct (Fallback)**: Directly constructs new blobs from HEAD and working tree content, then updates index entries. This is used when patch context is ambiguous or unstable.

### Content-Based Hunk IDs
Hunks are identified by a SHA-256 hash of their `path:position:content`. This makes IDs stable across file rewrites and immune to position changes in other parts of the file.

### Diff Base Correctness
- `scan` uses **Index → Workdir** diffing to correctly exclude already-staged content.
- `status` uses **HEAD → Index** diffing to show what is prepared for commit.

For a deep dive into the system design, see [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Library Usage

agstage is designed as both a CLI and a library.
- **CLI**: Optimized for AI agents with JSON output.
- **Library**: Public modules are re-exported for integration into other Rust tools.

Run `cargo doc --open` to view the full API documentation.

## Install

```bash
cargo install --path .
```
*Requires Rust 1.85+ and a C compiler (for libgit2).*

## Quick Start

```bash
# 1. Scan for unstaged changes (compact JSON — hunk IDs + counts)
agstage scan

# Scan with full diff line content (for code review)
agstage scan --full

# 2. Stage a specific hunk by ID (from scan output)
agstage stage --hunk abc123def456

# 3. Stage specific line ranges
agstage stage --lines src/lib.rs:10-20,30-40

# 4. Commit staged changes
agstage commit -m "feat: add feature X"
```

## Command Reference

agstage provides a comprehensive suite of commands for index manipulation:

- `scan`: Discover unstaged changes with content-based hunk IDs.
- `stage`: Add selections (file, hunk, lines) to the index. Supports `--atomic` and `--dry-run`.
- `unstage`: Remove selections from the index.
- `status`: Show currently staged changes (HEAD vs Index).
- `commit`: Create a git commit with structured JSON output.
- `repair`: Auto-retry failed staging operations with alternate strategies.
- `backup create` / `backup restore` / `backup list`: Manage index snapshots for safety.

For the full command reference and JSON schema specification, see [docs/CLI_SPEC.md](docs/CLI_SPEC.md).

## Development

```bash
cargo build                      # compile
cargo test                       # all tests (unit + integration)
cargo clippy -- -D warnings      # lint (zero warnings required)
cargo fmt --check                # format check
```

See [CLAUDE.md](CLAUDE.md) for contribution guidelines, [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for module design, and [docs/CLI_SPEC.md](docs/CLI_SPEC.md) for the complete JSON schema specification.

## License

MIT
