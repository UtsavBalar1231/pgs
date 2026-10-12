# src/git/ — Git Operations Layer

All git operations via libgit2. No subprocess calls. No `std::process::Command`.

## Index-Direct Staging Strategy

This project does NOT use patch-apply. Instead:

1. Read the base blob (index, falling back to HEAD)
2. Read the workdir file through git's clean filters (`read_workdir_for_index`)
3. Diff them with `similar::TextDiff` (line-level)
4. Build a new blob by selectively accepting/rejecting changes; replace runs are
   flushed by `flush_replace_run` in the order `replace_run_partners` decides
5. Write the blob to ODB via `repo.blob()`
6. Update the index via `index.add()` + `index.write()` (entries carry zero stat data)

This strategy handles file, hunk, and line-level staging uniformly.

## Module Map

### `mod.rs` — Shared Helpers

| Function | Purpose |
|----------|---------|
| `read_workdir_for_index(repo, path)` | Workdir content as `git add` stores it: built-in clean filters applied; refuses external filter drivers and non-file entries |
| `read_workdir_for_blob(root, path)` | Raw workdir bytes (symlinks as their target string); used for checksums |
| `build_index_entry(index, path, oid, mode_override)` | Constructs `git2::IndexEntry` with zero stat data, preserving mode/flags (minus intent-to-add) or defaulting to `0o100644` |
| `replace_run_partners(deletions, additions)` | The one deletion→addition pairing rule (content, then position, then last) |
| `flush_replace_run(deletes, inserts, out)` | Emits a buffered replace run in pairing order; shared by stage and unstage |
| `read_head_blob(repo, path)` | Read file content from HEAD tree |
| `read_index_blob(repo, path)` | Read file content from current index |
| `read_head_mode(repo, path)` | Read file mode from HEAD (e.g. `0o100644`) |
| `content_is_binary(content)` | Null-byte scan of first 8000 bytes (matches libgit2 heuristic) |

### `staging.rs` — Stage Operations

| Function | Granularity | Notes |
|----------|-------------|-------|
| `stage_file(repo, path, mode_override)` | Whole file | Reads workdir, creates blob, updates index |
| `stage_lines(repo, path, selected_lines, mode_override)` | Line ranges | Diffs index/HEAD vs workdir, applies only selected lines |
| `stage_hunk(repo, path, hunk, mode_override)` | Single hunk | No production caller (`cmd/` stages hunks via `line_selection_for` + `stage_lines`); kept for its regression tests |
| `stage_deletion(repo, path)` | Whole file | Removes entry from index |
| `stage_rename(repo, old, new, mode_override)` | File pair | Unreachable: scan never emits renames (see ARCHITECTURE.md) |

`mode_override: Option<u32>` on `stage_file`, `stage_lines`, `stage_hunk`, and `stage_rename` enables staging file permission changes (e.g. `chmod +x`).

### `unstaging.rs` — Unstage Operations

| Function | Granularity | Notes |
|----------|-------------|-------|
| `unstage_file(repo, path)` | Whole file | Restores index entry from HEAD blob |
| `unstage_lines(repo, path, selected_lines)` | Line ranges | Reverse-diffs: HEAD vs index, selectively reverts |
| `unstage_hunk(repo, path, hunk)` | Single hunk | Reverts a single hunk from the staged diff |

### `diff.rs` — Diff Engine

| Function | Diff Base | Used By |
|----------|-----------|---------|
| `diff_index_to_workdir(repo, context)` | Index -> Workdir | `scan` command |
| `diff_head_to_index(repo, context)` | HEAD -> Index (empty tree before the first commit) | `status`, `unstage` commands |

Also contains: `build_scan_result()`, `build_status_report()` — walk diff deltas and build typed results.

### `repo.rs` — Repository Discovery

- `open(path)` — open repo at exact path, or discover from CWD; `NotARepository` when nothing is found
- `workdir(repo)` — get working dir, error on bare repos

## Critical Invariant

**Diff base correctness**: `scan` MUST use Index->Workdir (excludes already-staged content). `status` and `unstage` MUST use HEAD->Index (shows/operates on what's staged). Mixing these up will produce incorrect results.

## Cross-Cutting Patterns

All workdir reads go through `read_workdir_for_blob` (raw) or `read_workdir_for_index` (staged form) — do not call `fs::read` directly on a worktree path. They are the only places that handle symlinks, clean filters, and non-file entries correctly. Hunk ids and checksums hash each line with its origin marker (` `, `+`, `-`).
