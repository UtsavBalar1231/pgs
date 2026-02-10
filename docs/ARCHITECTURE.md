# agstage Architecture

## Overview

**agstage** is a Rust CLI tool that enables AI agents to stage git changes at multiple levels of granularity: file, hunk, and line-range. It provides a JSON-only API for programmatic index manipulation with zero runtime dependencies beyond libgit2.

Key design principles:
- **Non-interactive**: Fully machine-controlled, no TTY assumptions
- **Pure git2**: All operations via libgit2 (no subprocess calls)
- **Content-addressable**: Stable hunk IDs based on content hashing, not position
- **Safe by default**: Mandatory index backup and lock detection
- **Always atomic**: Stop on first failure, restore backup, return error
- **Index-direct only**: Single staging strategy via direct blob construction

---

## Module Map

```
src/
  main.rs            — CLI entry point: parse args, call run(), print JSON, set exit code
  lib.rs             — pub mod declarations

  cmd/               — Command handlers (one file per command)
    mod.rs           — Cli struct (clap derive), Command enum, run() dispatcher
    scan.rs          — scan subcommand handler
    stage.rs         — stage subcommand handler
    unstage.rs       — unstage subcommand handler
    status.rs        — status subcommand handler
    commit.rs        — commit subcommand handler

  git/               — All git2 operations (no subprocess calls)
    mod.rs           — Shared helpers (build_index_entry, read_head_blob, read_index_blob, content_is_binary)
    repo.rs          — Repository discovery (open, workdir)
    diff.rs          — Diff engine (index-to-workdir, HEAD-to-index, build_scan_result, build_status_report)
    staging.rs       — Index-direct staging (stage_file, stage_lines, stage_hunk, stage_deletion, stage_rename)
    unstaging.rs     — Index-direct unstaging (unstage_file, unstage_lines, unstage_hunk)

  selection/         — Selection parsing and resolution
    mod.rs           — Re-exports
    parse.rs         — Auto-detect positional args (file path, hunk ID, path:range)
    resolve.rs       — Resolve specs against scan results, validate binary/whole-file/freshness constraints

  safety/            — Lock detection and index backup
    mod.rs           — Re-exports
    lock.rs          — is_index_locked(), wait_for_lock_release()
    backup.rs        — create_backup(), restore_backup()

  error.rs           — AgstageError enum (14 variants), exit_code() mapping
  models.rs          — All serializable types (ScanResult, StageResult, StatusReport, CommitResult, etc.)
```

---

## Data Models

### Scan Results

```rust
ScanResult {
  files: Vec<FileInfo>,
  summary: ScanSummary,
}

FileInfo {
  path: String,
  status: FileStatus,
  file_checksum: String,  // SHA-256 of working-tree content
  is_binary: bool,
  hunks: Vec<HunkInfo>,   // empty for binary files
}

HunkInfo {
  hunk_id: String,        // sha256(path:old_start:new_start:content)[0..12]
  old_start: u32,
  old_lines: u32,
  new_start: u32,
  new_lines: u32,
  header: String,         // @@ -old_start,old_lines +new_start,new_lines @@
  lines: Vec<DiffLineInfo>,
  checksum: String,       // SHA-256 of hunk lines
}

DiffLineInfo {
  line_number: u32,       // Position in old (deletions) or new (additions/context)
  origin: LineOrigin,     // Context | Addition | Deletion
  content: String,
}
```

### Compact Scan Results (Default Output)

Default `agstage scan` output -- hunk metadata without diff line content. Use `--full` for the full `ScanResult` above.

```rust
CompactScanResult {
  files: Vec<CompactFileInfo>,
  summary: ScanSummary,          // same as full scan
}

CompactFileInfo {
  path: String,
  status: FileStatus,
  is_binary: bool,
  hunks: Vec<CompactHunkInfo>,
  hunks_count: usize,
  lines_added: u32,
  lines_deleted: u32,
}

CompactHunkInfo {
  hunk_id: String,        // identical to HunkInfo.hunk_id
  header: String,         // @@ header with function context
  old_start: u32,
  old_lines: u32,
  new_start: u32,
  new_lines: u32,
  additions: u32,         // count of Addition lines
  deletions: u32,         // count of Deletion lines
}
```

Conversion: `CompactScanResult` is derived from `ScanResult` via `From<&ScanResult>` at the CLI output boundary. Internal callers (stage, unstage) always use the full `ScanResult`.

### Stage Results

```rust
StageResult {
  status: OperationStatus,  // Ok | DryRun
  succeeded: Vec<StagedItem>,
  failed: Vec<FailedItem>,   // always empty on success (failures roll back)
  warnings: Vec<String>,
  backup_id: String,         // always present (backup is mandatory)
}

StagedItem {
  selection: String,        // the positional arg (e.g., "src/main.rs" or "abc123def456")
  lines_staged: u32,
}

FailedItem {
  selection: String,
  reason: String,           // stale_scan | index_locked | staging_failed | ...
  suggestion: String,       // human/agent-readable fix
}
```

### Selection Types

```rust
SelectionSpec {
  File { path: String },
  Hunk { hunk_id: String },
  Lines { path: String, ranges: Vec<LineRange> },
}

LineRange {
  start: u32,
  end: u32,  // inclusive
}
```

### Commit Result

```rust
CommitResult {
  commit_hash: String,    // full 40-char hex SHA
  message: String,
  author: String,         // "Name <email>"
  files_changed: usize,
  insertions: u32,
  deletions: u32,
}
```

---

## Data Flow

### 1. `agstage scan`

Discover all unstaged changes in the working tree.

```
CLI args (positional files optional, --full optional)
  |
  v
git/repo.rs -- open repository
  |
  v
git/diff.rs -- diff_index_to_workdir() via git2::DiffOptions
  |
  v
git/diff.rs -- build_scan_result(): iterate deltas, extract hunks,
               compute content-based hunk IDs and checksums
  |
  v
ScanResult (full, always built internally)
  |
  v
[--full flag?]
  |-- yes --> ScanResult serialization (with lines[], checksums)
  +-- no  --> CompactScanResult::from(&result) (metadata only)
  |
  v
serde_json::to_string() --> stdout
```

**Critical**: Uses **index-to-workdir diff**, not HEAD-to-workdir. This correctly excludes already-staged content.

### 2. `agstage stage`

Stage selected changes into the git index.

```
CLI args (positional selections, --exclude, --dry-run)
  |
  v
selection/parse.rs -- auto-detect each arg into SelectionSpec
  |
  v
git/diff.rs -- diff_index_to_workdir() --> ScanResult (fresh scan)
  |
  v
selection/resolve.rs -- resolve_selection(): match specs against scan hunks
                     -- validate_binary_constraints(): reject hunk/lines on binary files
                     -- validate_whole_file_constraints(): reject hunk/lines on added/deleted/renamed
                     -- validate_freshness(): check file checksum matches scan
  |
  v
safety/lock.rs -- wait_for_lock_release(): check .git/index.lock
  |
  v
safety/backup.rs -- create_backup(): snapshot index to .git/agstage/backups/
  |
  v
[For each resolved selection:]
  git/staging.rs -- stage_file():     read workdir file, create blob, update index
                 -- stage_lines():    diff HEAD vs workdir, selectively apply lines
                 -- stage_hunk():     convert hunk to line numbers, delegate to stage_lines()
                 -- stage_deletion(): remove file from index
                 -- stage_rename():   remove old path, add new path
  |
  v
[On failure: safety/backup.rs -- restore_backup()]
  |
  v
StageResult serialization --> stdout
```

**Always atomic**: On first staging failure, the index is restored from backup and an error is returned.

**Dry-run mode**: All steps through validation, but no index modifications. Reports what would happen.

### 3. `agstage unstage`

Remove selected changes from the index (working tree unchanged).

```
CLI args (positional selections, --exclude, --dry-run)
  |
  v
selection/parse.rs -- auto-detect specs
  |
  v
git/diff.rs -- diff_head_to_index() --> uses HEAD-to-index diff base
  |
  v
selection/resolve.rs -- same validation pipeline as stage
  |
  v
safety/lock.rs + safety/backup.rs -- lock check + backup
  |
  v
[For each resolved selection:]
  git/unstaging.rs -- unstage_file():  restore HEAD blob (or remove added file from index)
                   -- unstage_lines(): diff HEAD vs index, selectively revert lines
                   -- unstage_hunk():  convert hunk to line numbers, delegate to unstage_lines()
  |
  v
[On failure: restore backup]
  |
  v
StageResult serialization --> stdout
```

### 4. `agstage status`

Show currently staged changes (HEAD vs. index).

```
CLI args
  |
  v
git/repo.rs -- open repository
  |
  v
git/diff.rs -- diff_head_to_index()
  |
  v
git/diff.rs -- build_status_report(): extract files and line counts
  |
  v
StatusReport serialization --> stdout
```

### 5. `agstage commit`

Create a git commit from staged changes.

```
CLI args (-m MESSAGE)
  |
  v
git/repo.rs -- open repository
  |
  v
Check that index differs from HEAD (otherwise: NoChanges)
  |
  v
git2 commit: write tree, create commit
  |
  v
CommitResult serialization --> stdout
```

---

## Index-Direct Staging Strategy

agstage v2 uses a single staging strategy: index-direct blob construction.

### How It Works

**For file-level staging** (`stage_file`):
1. Read the working-tree file content
2. Create a blob in the ODB via `repo.blob()`
3. Build an `IndexEntry` preserving existing mode/flags
4. Write to index via `index.add_frombuffer()`

**For line-level staging** (`stage_lines`):
1. Read HEAD blob (the base state) and working-tree file (the target state)
2. Diff HEAD vs workdir using `similar::TextDiff::from_lines()`
3. Walk all changes:
   - **Equal** lines: always include
   - **Delete** lines: keep HEAD line (unless its old line number is selected)
   - **Insert** lines: include only if its new line number is in `selected_lines`
4. Reconstruct the result blob, preserving trailing-newline semantics
5. Create blob, update index entry

**For hunk-level staging** (`stage_hunk`):
1. Extract new-file line numbers for all Addition and Context lines in the hunk
2. Build a `HashSet<u32>` of these line numbers
3. Delegate to `stage_lines()`

**For deletions** (`stage_deletion`):
1. Remove the file entry from the index via `index.remove_path()`

**For renames** (`stage_rename`):
1. Remove the old path from the index
2. Read the new file from the working directory
3. Create a blob and add the new entry to the index

### Unstaging (Reverse Direction)

Unstaging reverses the direction: the diff base is HEAD vs index (not HEAD vs workdir).

- `unstage_file()`: Restore the HEAD blob into the index. For added files (not in HEAD), remove from index.
- `unstage_lines()`: Diff HEAD vs index content, selectively revert lines to HEAD state.
- `unstage_hunk()`: Convert hunk to line numbers, delegate to `unstage_lines()`.

### Trailing Newline Preservation

Both staging and unstaging track whether the source files had trailing newlines and preserve this property in the result blob. This prevents spurious whitespace-only diffs.

### O(N) Line Processing

All line-level operations use pre-collected `Vec<&str>` slices from `similar::TextDiff`, avoiding repeated `lines().nth(i)` lookups.

---

## Critical Diff Base Explanation

This is the most important architectural decision:

| Command | Diff Base | Reason |
|---------|-----------|--------|
| `scan` | **Index --> Workdir** | Shows unstaged changes only; already-staged content is excluded |
| `status` | **HEAD --> Index** | Shows what is staged for commit |
| `stage` | **Index --> Workdir** | Stages from workdir into index |
| `unstage` | **HEAD --> Index** | Unstages from index back toward HEAD |

**Why NOT HEAD --> Workdir for scan?**
- If you already staged a change, HEAD-to-workdir would include it in the scan
- Index-to-workdir correctly shows only what remains to stage
- Agents can call scan --> stage --> scan again and get correct answers

---

## Selection Auto-Detection

v2 replaces explicit `--file`/`--hunk`/`--lines` flags with positional arguments that are auto-detected.

Detection rules (applied in order):
1. Contains `:` where the character after the last `:` is a digit --> `Lines { path, ranges }`
2. Exactly 12 hexadecimal characters --> `Hunk { hunk_id }`
3. Everything else --> `File { path }`

This handles edge cases like Windows paths (`C:\Users\file.rs` -- last `:` followed by `\`, not a digit) and short/long hex strings (11 or 13 hex chars are treated as file paths).

---

## Safety

### Index Backup (Mandatory)

Every stage/unstage operation creates an index backup before modifying the index:

1. Read the raw `.git/index` file
2. Write a copy to `.git/agstage/backups/{backup_id}.index`
3. Write metadata to `.git/agstage/backups/{backup_id}.json`
4. Backup ID format: `backup-{YYYYMMDDTHHMMSS}-{uuid8}`

On failure, the backup is restored automatically to ensure atomic rollback.

### Lock Detection

Before staging operations, agstage checks for `.git/index.lock`:
- If locked, retries with exponential backoff (50ms, 100ms, 200ms, ...)
- After max retries, returns `IndexLocked` error (exit code 3)

---

## Dependency Graph

```
main.rs
  +-- cmd/mod.rs
        |-- cmd/{scan,stage,unstage,status,commit}.rs
        |    +-- git/*, selection/*, safety/*
        |
        |-- selection/{parse,resolve}.rs
        |    |-- models.rs (SelectionSpec)
        |    +-- git/diff.rs (ScanResult)
        |
        |-- git/{repo,diff,staging,unstaging}.rs
        |    |-- models.rs
        |    +-- error.rs
        |
        +-- safety/{backup,lock}.rs
             +-- models.rs

error.rs (no deps)
models.rs (serde only)
```

**Dependency principles:**
- Models (error.rs, models.rs) have no internal dependencies
- CLI handlers depend on business logic modules
- Safety layer is composable and independent
- No circular dependencies

---

## Error Handling and Exit Codes

All errors map to exit codes for script-friendly operation:

```rust
pub enum AgstageError {
  // Exit 1: No effect
  NoChanges,
  SelectionEmpty,

  // Exit 2: User error (invalid input)
  InvalidSelection { detail },
  InvalidLineRange { path, start, end },
  UnknownHunkId { hunk_id },
  FileNotInDiff { path },
  BinaryFileGranular { path },
  GranularOnWholeFile { path },

  // Exit 3: Conflict/failure (retryable -- agent should re-scan)
  StaleScan { path },
  IndexLocked,
  StagingFailed { path, reason },

  // Exit 4: Internal error (bug or system)
  Git(git2::Error),
  Io { path, source },
  Json(serde_json::Error),
  Internal(String),
}
```

`main.rs` catches all errors and outputs a JSON error response `{"error": "...", "exit_code": N}` to stdout.

---

## Binary File Handling

agstage detects binary files using a two-step heuristic:
1. Check `git2::DiffFile::is_binary()` / `is_not_binary()` flags from the delta
2. Fallback: read the working-tree file and scan the first 8000 bytes for null bytes

Binary files are treated specially:
- **Scan**: `is_binary: true` in `FileInfo`, empty `hunks` vector, checksum still computed
- **Stage/Unstage**: Only file-level selections allowed; sub-file selections return `BinaryFileGranular` error (exit code 2)
- **Index-direct**: `stage_file()` handles binary content correctly since it operates on raw bytes

---

## Whole-File Constraints

Added, deleted, and renamed files must be staged at file level. Attempting hunk or line-level selections on such files returns `GranularOnWholeFile` error (exit code 2). This is enforced because:
- Added files have no HEAD blob to diff against for partial staging
- Deleted files must be removed from the index atomically
- Renamed files involve path changes that cannot be partially applied

---

## Design Decisions Summary

| Decision | Choice | Rationale |
|----------|--------|-----------|
| **Language** | Rust | Type safety, performance, single static binary, no runtime deps |
| **Git library** | libgit2 (git2 crate) | Mature, widely-used, supports index manipulation |
| **Subprocess calls** | None | Avoid shell injection, platform-specific issues, consistency |
| **Staging strategy** | Index-direct only | Simpler codebase; precise blob construction; works for all cases |
| **Diff base for scan** | Index --> Workdir | Correctly excludes already-staged content |
| **Hunk IDs** | Content-based (SHA-256) | Stable across file rewrites, immune to position changes |
| **Error handling** | thiserror, typed errors | Compile-time exhaustiveness, clean exit codes |
| **Atomicity** | Always atomic | Agents need predictable state; no partial success |
| **Index backup** | Mandatory | Required for atomic rollback |
| **Output format** | JSON only | Machine-first design for AI agents |
| **Selection syntax** | Positional auto-detect | Fewer flags, simpler CLI, natural for agents |
| **Context lines** | Default 3, minimum 1 | Balance hunk boundary granularity with patch robustness |
| **Line diffing** | similar crate | Production-quality LCS diff for selective line staging |

---

## Testing Strategy

### Unit Tests
- **error.rs**: Exit code mappings for all 14 variants
- **models.rs**: JSON serialization round-trips for all output types
- **selection/parse.rs**: Auto-detection rules (file, hunk, lines, edge cases)
- **selection/resolve.rs**: Resolution, binary constraints, whole-file constraints, freshness validation
- **git/mod.rs**: `build_index_entry`, `read_head_blob`, `read_index_blob`, `content_is_binary`
- **git/diff.rs**: Diff engine, hunk extraction, checksums, context lines
- **git/staging.rs**: `stage_file`, `stage_lines`, `stage_hunk`, `stage_deletion`, `stage_rename`
- **git/unstaging.rs**: `unstage_file`, `unstage_lines`, `unstage_hunk`
- **safety/lock.rs**: Lock detection and wait logic
- **safety/backup.rs**: Create and restore backups

### Integration Tests
- Real git repository in tmpdir
- End-to-end workflows: scan --> stage --> status --> unstage
- Verify index state with `git2::Repository::index()`

### Property Tests
- Hunk IDs stable across multiple scans
- Index backup/restore idempotent

---

## Future Extensions

1. **Sparse checkout support**: Skip-worktree flag handling and `.git/info/sparse-checkout` parsing
2. **Network staging**: HTTP API for remote agents to manipulate indexes
3. **IDE integration**: LSP extension for VS Code
4. **Performance**: Native diffing engine to replace libgit2 diff

---

## References

- [Git Update-Index](https://git-scm.com/docs/git-update-index)
- [libgit2 Index API](https://docs.rs/git2/latest/git2/index/struct.Index.html)
- [similar crate](https://docs.rs/similar/latest/similar/) -- line-level diffing
