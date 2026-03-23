---
name: git-commit-staging
description: >
  Non-interactive git staging at file, hunk, or line-range granularity.
  Use when staging changes, splitting commits, creating atomic commits,
  selective staging, or committing specific hunks/lines.
  Use when user mentions "commit", "stage", "split commits", "granular staging", "atomic commits",
  or "selective staging". Requires pgs CLI.
allowed-tools:
  - Bash
---

# Git Commit Staging with pgs

Non-interactive git staging at file, hunk, or line-range granularity. Use when splitting
mixed-intent changes into separate commits. Default output is structured text markers — read
them directly, no JSON parsing needed.

Prerequisites check:
```bash
which pgs && pgs --version
```

If `pgs` is not found, inform the user and stop — do not fall back to raw git commands.

---

## 1. Core Rules

1. Always scan before staging — hunk IDs are ephemeral, valid only until the file or index changes
2. Re-scan after each commit — the index changed, previous hunk IDs are stale
3. Use default text output — text markers are directly readable, no `--json` or parsing needed
4. Plan all commits before staging — group changes by intent, each group becomes one commit
5. Verify with `pgs status` before every commit
6. Use only pgs commands for all diff/staging/status operations — pgs provides diff-base-correct
   structured output that raw git commands cannot replicate. The only allowed git command is `git log`.
7. On exit code 3 (stale/conflict): re-scan, then retry with fresh IDs

---

## 2. Command Reference

| Command | Description |
|---------|-------------|
| `pgs scan [files...] [--full]` | Scan unstaged changes (compact by default, `--full` for line content) |
| `pgs stage <sel...> [--exclude <sel>...] [--dry-run]` | Stage selections |
| `pgs unstage <sel...> [--exclude <sel>...] [--dry-run]` | Unstage selections |
| `pgs status` | Show staged changes (HEAD vs index) |
| `pgs commit -m "message"` | Commit staged changes |

Global flags: `--repo PATH` (default: CWD), `--context N` (default: 3, min: 1)

**Selection syntax** (positional, auto-detected):

| Pattern | Detection Rule | Example |
|---------|---------------|---------|
| File path | Anything not matching other rules | `src/main.rs` |
| Hunk ID | Exactly 12 hex characters | `a1b2c3d4e5f6` |
| Line range | Path contains `:` followed by digit | `src/main.rs:10-20,30-40` |
| Directory | Ends with `/` | `tests/` |

Edge case: if a file path is exactly 12 hex chars, prefix with `./`.

**pgs equivalents for common git commands:**

| git command | pgs equivalent | What pgs adds |
|-------------|----------------|---------------|
| `git status -s` | `pgs scan` | Hunk IDs, structured counts |
| `git diff` | `pgs scan --full` | Structured line-level output |
| `git diff --stat` | `pgs scan` (compact) | Per-file + per-hunk stats |
| `git diff --cached` | `pgs status` | Structured staged info |
| `git add -p` | `pgs stage HUNK_ID` | Non-interactive, atomic |
| `git add file` | `pgs stage path` | Automatic backup |
| `git reset HEAD file` | `pgs unstage path` | Automatic backup |

Only allowed git command: `git log` (for commit history context).

---

## 3. Reading Scan Output

### Compact scan (default)

```
@@pgs:v1 scan.begin {"command":"scan","detail":"compact","items":2}
@@pgs:v1 file {"path":"src/auth.rs","status":{"type":"Modified"},"binary":false,"hunks_count":2,"lines_added":15,"lines_deleted":3}
@@pgs:v1 hunk {"path":"src/auth.rs","id":"a1b2c3d4e5f6","old_start":10,"old_lines":5,"new_start":10,"new_lines":7,"header":"@@ -10,5 +10,7 @@ fn authenticate","additions":2,"deletions":0}
@@pgs:v1 hunk {"path":"src/auth.rs","id":"f6e5d4c3b2a1","old_start":50,"old_lines":8,"new_start":52,"new_lines":5,"header":"@@ -50,8 +52,5 @@ fn validate","additions":0,"deletions":3}
@@pgs:v1 file {"path":"src/utils.rs","status":{"type":"Added"},"binary":false,"hunks_count":1,"lines_added":20,"lines_deleted":0}
@@pgs:v1 hunk {"path":"src/utils.rs","id":"1a2b3c4d5e6f","old_start":0,"old_lines":0,"new_start":1,"new_lines":20,"header":"@@ -0,0 +1,20 @@","additions":20,"deletions":0}
@@pgs:v1 summary {"command":"scan","detail":"compact","total_files":2,"total_hunks":3,"added":1,"modified":1,"deleted":0,"renamed":0,"binary":0,"mode_changed":0}
@@pgs:v1 scan.end {"command":"scan","detail":"compact","items":2}
```

Key fields:
- `id` — the 12-hex hunk ID you pass to `pgs stage`
- `header` — shows the function/section context after the `@@` markers
- `hunks_count` — how many hunks this file has (drives granularity decision)
- `status.type` — `Added`, `Modified`, `Deleted`, `Renamed` (Renamed also has `old_path`)
- `summary.mode_changed` — count of file permission-only changes

### Full scan (`--full`)

```
@@pgs:v1 scan.begin {"command":"scan","detail":"full","items":1}
@@pgs:v1 file.begin {"path":"src/auth.rs","status":{"type":"Modified"},"binary":false,"hunks_count":1,"lines_added":2,"lines_deleted":0,"checksum":"abc123..."}
@@pgs:v1 hunk.begin {"path":"src/auth.rs","id":"a1b2c3d4e5f6","old_start":10,"old_lines":3,"new_start":10,"new_lines":5,"header":"@@ -10,3 +10,5 @@ fn authenticate","additions":2,"deletions":0,"checksum":"def456..."}
 fn authenticate(user: &str) -> bool {
     let valid = check_password(user);
+    log::info!("auth attempt for {user}");
+    audit::record(user, valid);
     valid
@@pgs:v1 hunk.end {"path":"src/auth.rs","id":"a1b2c3d4e5f6","old_start":10,"old_lines":3,"new_start":10,"new_lines":5,"header":"@@ -10,3 +10,5 @@ fn authenticate","additions":2,"deletions":0,"checksum":"def456..."}
@@pgs:v1 file.end {"path":"src/auth.rs","status":{"type":"Modified"},"binary":false,"hunks_count":1,"lines_added":2,"lines_deleted":0,"checksum":"abc123..."}
@@pgs:v1 summary {"command":"scan","detail":"full","total_files":1,"total_hunks":1,"added":0,"modified":1,"deleted":0,"renamed":0,"binary":0,"mode_changed":0}
@@pgs:v1 scan.end {"command":"scan","detail":"full","items":1}
```

Raw diff lines between `hunk.begin` and `hunk.end`: ` ` = context, `+` = addition, `-` = deletion.
Use `--full` only on filtered scans (`pgs scan path --full`), not the whole repo.

### Stage output

```
@@pgs:v1 stage.begin {"command":"stage","status":"ok","items":1,"backup_id":"backup-550e8400-..."}
@@pgs:v1 item {"selection":"a1b2c3d4e5f6","lines_affected":2}
@@pgs:v1 stage.end {"command":"stage","status":"ok","items":1,"backup_id":"backup-550e8400-..."}
```

### Status output

```
@@pgs:v1 status.begin {"command":"status","items":1}
@@pgs:v1 status.file {"path":"src/auth.rs","status":{"type":"Modified"},"lines_added":2,"lines_deleted":0}
@@pgs:v1 summary {"command":"status","total_files":1,"total_additions":2,"total_deletions":0}
@@pgs:v1 status.end {"command":"status","items":1}
```

### Commit output

```
@@pgs:v1 commit.result {"version":"v1","command":"commit","commit_hash":"abc123...40chars","message":"feat: add auth logging","author":"Name <email>","files_changed":1,"insertions":2,"deletions":0}
```

### Error output

```
@@pgs:v1 error {"version":"v1","command":"scan","phase":"runtime","code":"no_changes","message":"no changes detected in working tree","exit_code":1}
```

---

## 4. Commit Planning & Granularity

Every staging session follows three phases: scan, plan, execute.

### Phase 1 — Scan

Run `pgs scan` to discover all changes (use `pgs scan`, not `git diff` — pgs provides the correct diff base and structured hunk IDs).

### Phase 2 — Plan commits

Before staging anything, analyze the scan output and write a commit plan.
For each file, read `hunks_count` and `header` to determine intent.

**Fill in this template** (one entry per commit group):
```
Commit 1 (type): file_a, file_b hunk ID — description of logical change
  Evidence: [why these changes belong together]
Commit 2 (type): file_c — description
  Evidence: [why this is a separate concern]
```

Example for a scan showing 2 modified files + 1 added file:
```
Commit 1 (fix): src/auth.rs hunk a1b2c3d4e5f6 — null check in authenticate()
  Evidence: header shows fn authenticate, additions only, isolated bug fix
Commit 2 (feat): src/api.rs, src/models.rs, src/routes.rs — new user profile endpoint
  Evidence: all three files relate to the same feature, headers show profile-related functions
```

If all changes belong to one logical intent, write that as a single-group plan.
Do not assume single intent — verify by reading the headers.

### Phase 3 — Per-file granularity

Choose the staging command based on file state and hunk count:

| Condition | Action | Command |
|-----------|--------|---------|
| `status.type` is `Added`, `Deleted`, or `Renamed` | File-level (required) | `pgs stage path` |
| `hunks_count` = 1, change belongs to this commit | File-level | `pgs stage path` |
| `hunks_count` >= 2, all hunks same intent | File-level | `pgs stage path` |
| `hunks_count` >= 2, hunks belong to different commits | Hunk-level — use `id` from scan | `pgs stage a1b2c3d4e5f6` |
| Single hunk mixes two intents (rare) | Line-level — run `--full` first | `pgs stage path:10-20` |
| All files in a directory share same intent | Directory-level | `pgs stage tests/` |

**How to determine intent from compact scan:**
- Read the `header` field — it shows the function/section name (e.g., `@@ -10,5 +10,7 @@ fn authenticate`)
- If headers show different functions, the hunks likely belong to different commits
- When unsure, run `pgs scan <file> --full` to inspect actual line changes

**Hunk IDs vs line ranges:** use hunk IDs by default; use line ranges only when a single hunk mixes two distinct intents.

---

## 5. Workflows

### Multi-commit workflow (default)

Scenario: `src/auth.rs` has a bug fix (hunk in `fn authenticate`) and a refactor (hunk in `fn validate`). New file `src/utils.rs` was added.

```bash
# Step 1: SCAN
pgs scan
```
```
@@pgs:v1 scan.begin {"command":"scan","detail":"compact","items":2}
@@pgs:v1 file {"path":"src/auth.rs","status":{"type":"Modified"},"binary":false,"hunks_count":2,"lines_added":5,"lines_deleted":2}
@@pgs:v1 hunk {"path":"src/auth.rs","id":"a1b2c3d4e5f6","old_start":10,"old_lines":3,"new_start":10,"new_lines":5,"header":"@@ -10,3 +10,5 @@ fn authenticate","additions":2,"deletions":0}
@@pgs:v1 hunk {"path":"src/auth.rs","id":"f6e5d4c3b2a1","old_start":50,"old_lines":5,"new_start":52,"new_lines":3,"header":"@@ -50,5 +52,3 @@ fn validate","additions":1,"deletions":2}
@@pgs:v1 file {"path":"src/utils.rs","status":{"type":"Added"},"binary":false,"hunks_count":1,"lines_added":20,"lines_deleted":0}
@@pgs:v1 hunk {"path":"src/utils.rs","id":"1a2b3c4d5e6f","old_start":0,"old_lines":0,"new_start":1,"new_lines":20,"header":"@@ -0,0 +1,20 @@","additions":20,"deletions":0}
@@pgs:v1 summary {"command":"scan","detail":"compact","total_files":2,"total_hunks":3,"added":1,"modified":1,"deleted":0,"renamed":0,"binary":0,"mode_changed":0}
@@pgs:v1 scan.end {"command":"scan","detail":"compact","items":2}
```

**Step 2 — Plan commits** (write out before staging):
```
Commit 1 (fix): src/auth.rs hunk a1b2c3d4e5f6 — bug fix in fn authenticate
  Evidence: header shows fn authenticate, additions only, 2 lines added
Commit 2 (refactor): src/auth.rs hunk f6e5d4c3b2a1 + src/utils.rs — extract validation
  Evidence: header shows fn validate, refactoring; utils.rs is new file supporting extraction
```
Two groups identified — two commits. Execute one at a time.

```bash
# Step 3: STAGE commit 1
pgs stage a1b2c3d4e5f6
```
```
@@pgs:v1 stage.begin {"command":"stage","status":"ok","items":1,"backup_id":"backup-..."}
@@pgs:v1 item {"selection":"a1b2c3d4e5f6","lines_affected":2}
@@pgs:v1 stage.end {"command":"stage","status":"ok","items":1,"backup_id":"backup-..."}
```

```bash
# Step 4: VERIFY
pgs status
```
```
@@pgs:v1 status.begin {"command":"status","items":1}
@@pgs:v1 status.file {"path":"src/auth.rs","status":{"type":"Modified"},"lines_added":2,"lines_deleted":0}
@@pgs:v1 summary {"command":"status","total_files":1,"total_additions":2,"total_deletions":0}
@@pgs:v1 status.end {"command":"status","items":1}
```

```bash
# Step 5: COMMIT
pgs commit -m "fix: correct authentication check"
```
```
@@pgs:v1 commit.result {"version":"v1","command":"commit","commit_hash":"abc123...","message":"fix: correct authentication check","author":"Dev <dev@example.com>","files_changed":1,"insertions":2,"deletions":0}
```

```bash
# Step 6: RE-SCAN (use pgs scan — hunk IDs are now stale after the commit)
pgs scan
```
```
@@pgs:v1 scan.begin {"command":"scan","detail":"compact","items":2}
@@pgs:v1 file {"path":"src/auth.rs","status":{"type":"Modified"},"binary":false,"hunks_count":1,"lines_added":1,"lines_deleted":2}
@@pgs:v1 hunk {"path":"src/auth.rs","id":"99887766aabb","old_start":50,"old_lines":5,"new_start":50,"new_lines":3,"header":"@@ -50,5 +50,3 @@ fn validate","additions":1,"deletions":2}
@@pgs:v1 file {"path":"src/utils.rs","status":{"type":"Added"},"binary":false,"hunks_count":1,"lines_added":20,"lines_deleted":0}
@@pgs:v1 hunk {"path":"src/utils.rs","id":"ccddee001122","old_start":0,"old_lines":0,"new_start":1,"new_lines":20,"header":"@@ -0,0 +1,20 @@","additions":20,"deletions":0}
@@pgs:v1 summary {"command":"scan","detail":"compact","total_files":2,"total_hunks":2,"added":1,"modified":1,"deleted":0,"renamed":0,"binary":0,"mode_changed":0}
@@pgs:v1 scan.end {"command":"scan","detail":"compact","items":2}
```

Note: the validate hunk ID changed from `f6e5d4c3b2a1` to `99887766aabb`. This is why re-scanning is mandatory.

```bash
# Step 7: STAGE commit 2 (use fresh IDs from re-scan)
pgs stage src/auth.rs src/utils.rs   # src/auth.rs now has 1 hunk, all same intent

# Step 8: VERIFY
pgs status

# Step 9: COMMIT
pgs commit -m "refactor: extract validation to utils"
```

### Single-commit shortcut

Use only when all of the following are true:
- Every changed file belongs to the same logical change
- You verified this by reading the scan output headers (not assumed from file names)
- The commit type is clear

```bash
pgs scan                              # 1. Scan
# Verify: all files/hunks serve one intent
pgs stage file1 file2 file3           # 2. Stage all
pgs status                            # 3. Verify
pgs commit -m "feat: description"     # 4. Commit
```

---

## 6. Constraints & Error Recovery

- **Whole-file constraint**: Added, Deleted, and Renamed files must be staged as whole files. Hunk/line selections return exit 2.
- **Binary constraint**: Binary files can only be staged at file level. Hunk/line selections return exit 2.
- **Stale hunk IDs**: After any commit, file edit, or index change, all hunk IDs are stale. Always re-scan.
- **`--context N` consistency**: Changing `--context` between scan and stage produces different hunk IDs, causing exit 3. Use the same value throughout.
- **Different diff bases**: `scan` diffs Index->Workdir. `unstage` diffs HEAD->Index. Hunk IDs from `scan` are not valid for `unstage`.
- **`pgs status` has no hunk IDs** — use file-level unstaging (`pgs unstage path`).

| Exit Code | Meaning | Recovery |
|-----------|---------|----------|
| 0 | Success | Continue |
| 1 | No effect | Check: are there unstaged changes? Maybe already staged/committed. |
| 2 | User error | Fix selection syntax. Check: binary? whole-file constraint? wrong ID? |
| 3 | Conflict/stale | Re-scan (`pgs scan`), retry with fresh hunk IDs |
| 4 | Internal error | Report the error. Check git repo state. |

---

## 7. Anti-Patterns

| Don't | Do Instead |
|-------|-----------|
| `pgs scan --json \| python3 -c "..."` | `pgs scan` — text markers are directly readable |
| `git diff`, `git status`, `git check-ignore` | `pgs scan`, `pgs status` — structured, diff-base-correct |
| Reuse hunk IDs after `pgs commit` | Re-scan: `pgs scan` |
| `pgs scan --full` (entire repo) | `pgs scan path --full` (filter first) |
| Compute line ranges manually | Use hunk IDs (`pgs stage ID`) whenever possible |
| Mix `git add` with `pgs stage` | Use one tool per workflow |
| Stage all files without a commit plan | Write commit plan first, stage per group |
| Identify N groups then make 1 commit | N groups = N commits, execute sequentially |
| Assume all changes are one commit | Read scan headers, verify intent per file |
| Skip the analysis/planning step | Always write the commit plan template before staging |

---

## 8. Commit Message Convention

Format: `type: short description`
- Imperative mood, no period, under 72 chars
- Types: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`, `perf`, `style`, `ci`, `build`
- Breaking changes: `feat!: description` or `BREAKING CHANGE:` in body
