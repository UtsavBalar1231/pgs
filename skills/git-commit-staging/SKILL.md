---
name: git-commit-staging
description: >
  Use agstage to create atomic git commits with surgical staging
  at file, hunk, or line-range granularity. Invoke when staging
  changes, splitting commits, creating atomic commits, selective
  staging, composing commits, organizing git changes, or committing
  specific hunks/lines. Requires agstage CLI.
allowed-tools:
  - Bash
---

# Git Commit Staging with agstage

## WHY THIS EXISTS

AI agents cannot stage changes at hunk or line granularity using standard git:

- **`git add -p` is impossible** — it requires interactive TTY input (y/n/s/e/q keypresses). AI agents have no TTY. This is the fundamental problem.
- **Manual patch construction is fragile** — building valid unified diffs for `git apply --cached` requires exact context lines, correct `@@` header math, and proper `---/+++` prefixes. One off-by-one line number = failed patch.
- **`git diff` output is unstructured** — no stable way to reference a specific hunk across commands.
- **No verification loop** — `git diff --cached` returns unstructured text that's hard to programmatically validate.

`agstage` solves all of these. It provides non-interactive staging with content-based hunk IDs, dry-run validation, automatic backup/restore, and structured text output by default with `--json` as opt-in.

## PREREQUISITES

Before using agstage, verify it is installed:

```bash
which agstage && agstage --version
```

If `agstage` is not found, inform the user that this skill requires the `agstage` CLI tool to be installed. Do not fall back to raw git commands — use the standard git staging workflow instead.

## WHEN TO USE

Invoke this skill when:
- The user wants to split changes into multiple logical commits
- There are mixed intents in the working tree (fix + refactor + feature)
- Hunk-level or line-level staging precision is needed
- The user asks for "clean commits", "atomic commits", or "perfect commits"

Do NOT use when:
- All changes belong to a single trivial commit (just use `git add` + `git commit`)
- The repository has no unstaged changes
- The user explicitly wants manual interactive staging

## CORE RULES (MANDATORY)

1. **Always scan before staging** — hunk IDs are only valid until the file or index changes
2. **Re-scan after each commit** — previous hunk IDs are now stale because the index changed
3. **Never use raw git commands for analysis or staging** — no `git diff`, `git diff --stat`, `git status`, `git add`. Use agstage exclusively (only `git log` is allowed for commit history context).
4. **Always verify after staging** — run `agstage status` to confirm staged content matches intent
5. **Use `--dry-run` first** for multi-selection or complex staging operations
6. **Parse structured output programmatically** — text mode uses `@@agstage:v1` marker records and JSON mode is opt-in
7. **Re-scan after exit code 3** — stale scan, index locked, or staging failure all require fresh data

---

## PROHIBITED COMMANDS (DO NOT USE)

When this skill is active, do NOT use these commands — agstage replaces them:

| Instead of... | Use... | Why |
|---------------|--------|-----|
| `git status -s` | `agstage scan` | Scan gives structured marker output (or JSON with `--json`) with hunk IDs |
| `git diff` | `agstage scan --full` | Full scan gives structured line-level diffs |
| `git diff --stat` | `agstage scan` (compact) | Compact scan already has per-file stats |
| `git diff -- path` | `agstage scan path --full` | Filtered scan with structured output |
| `git diff --cached` | `agstage status` | Status gives structured staged info |
| `git add -p` | `agstage stage HUNK_ID` | Non-interactive hunk staging |
| `git add file` | `agstage stage path` | Consistent workflow with backup |
| `git reset HEAD file` | `agstage unstage path` | Consistent workflow with backup |

**The only git command you may use** during this workflow is `git log` (for commit history context). Everything else goes through agstage.

---

## SELECTION SYNTAX (Positional Auto-Detection)

agstage uses flat positional arguments with auto-detection. Each argument is parsed as one of three types:

| Detection Rule (applied in order) | Example | Parsed As |
|------------------------------------|---------|-----------|
| Contains `:` where char after last `:` is a digit | `src/main.rs:10-20` | Lines (path + ranges) |
| Exactly 12 hexadecimal characters | `abc123def456` | Hunk ID |
| Everything else | `src/main.rs` | File path |

**Line ranges** are 1-indexed, inclusive: `src/main.rs:1-5,10-15` stages lines 1-5 and 10-15.

**`--exclude`** uses the same auto-detection: `agstage stage src/main.rs --exclude abc123def456` excludes a hunk by ID. Works on both `stage` and `unstage`.

**Edge case**: If a file path is exactly 12 hex characters, it will be misdetected as a hunk ID. Use a path prefix: `./abc123def456`.

---

## QUICK REFERENCE

### Commands

```bash
agstage scan                              # Compact scan (default — metadata only)
agstage scan --full                       # Full scan with line-level diff content
agstage scan src/main.rs src/lib.rs       # Filter to specific files (positional)
agstage stage src/main.rs                 # Stage entire file
agstage stage abc123def456                # Stage specific hunk (12-hex ID from scan)
agstage stage src/main.rs:10-20           # Stage line range (1-indexed, inclusive)
agstage stage src/main.rs --exclude abc123def456  # Stage file, exclude a hunk
agstage stage src/main.rs --dry-run       # Validate without modifying index
agstage unstage abc123def456              # Remove hunk from index
agstage unstage src/main.rs               # Unstage entire file
agstage status                            # Show what's staged (HEAD vs index)
agstage commit -m "type: message"         # Commit staged changes
```

### Global Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--repo PATH` | CWD | Repository path (auto-discovers `.git`) |
| `--context N` | 3 | Context lines for diff generation (min: 1) |

### Key Output Fields

| Command | Key fields to extract |
|---------|----------------------|
| `scan` | `files[].path`, `files[].status.type`, `files[].hunks[].id`, `files[].hunks[].header`, `summary` |
| `stage`/`unstage` | `status` (`ok`/`dry_run`), `items[].selection`, `items[].lines_affected`, `backup_id` |
| `status` | `files[].path`, `files[].status`, `summary.total_files` |
| `commit` | `commit_hash`, `files_changed`, `insertions`, `deletions` |
| error | `version`, `command`, `phase`, `code`, `message`, `exit_code` |

### Compact Scan Example (JSON mode)

```json
{
  "files": [
    {
      "path": "src/main.rs",
      "status": { "type": "Modified" },
      "binary": false,
      "hunks": [
        {
          "id": "abc123def456",
          "header": "@@ -10,3 +10,5 @@ fn main()",
          "old_start": 10, "old_lines": 3,
          "new_start": 10, "new_lines": 5,
          "additions": 2, "deletions": 0
        }
      ],
      "hunks_count": 1,
      "lines_added": 2,
      "lines_deleted": 0
    }
  ],
  "summary": { "total_files": 1, "total_hunks": 1, "added": 0, "modified": 1, "deleted": 0, "renamed": 0, "binary": 0 }
}
```

### Exit Codes

| Code | Meaning | Recovery |
|------|---------|----------|
| 0 | Success | Proceed normally |
| 1 | No effect | Check: are there unstaged changes? Maybe already staged. |
| 2 | User error | Fix selection syntax. Check: binary file? whole-file constraint? |
| 3 | Conflict | **Re-scan** (`agstage scan`), then retry with fresh hunk IDs. |
| 4 | Internal | Report the error. Check git repo state. |

---

## FAST PATH (simple changes)

When all changes belong to **one logical commit**, skip the full workflow:

```bash
agstage scan                              # 1. Discover
agstage stage src/main.rs src/lib.rs      # 2. Stage
agstage status                            # 3. Verify
agstage commit -m "feat: add new feature" # 4. Commit
```

Use the fast path when the scan shows changes that clearly belong together. Escalate to the full workflow when you see mixed intents.

---

## FULL WORKFLOW (multi-intent changes)

### Phase 1: DISCOVER

```bash
agstage scan                              # Compact overview of all changes
agstage scan src/auth.rs src/login.rs     # Filter to specific files
agstage scan src/auth.rs --full           # Get line-level diff content
```

Parse structured output (marker records by default, JSON when `--json` is used). Each file has `hunks[]` with stable `id` values you can pass directly to `stage`.

### Phase 2: PLAN

Group changes by logical intent. Each commit should have ONE purpose. **Present the grouping plan to the user before staging:**

> Commit 1 (feat): Add login endpoint — `src/auth.rs` (hunks abc123def456, 789012abcdef)
> Commit 2 (fix): Fix validation bug — `src/validate.rs:10-25`
> Commit 3 (refactor): Extract helper — `src/utils.rs` (full file)

### Phase 3: STAGE

Choose the right granularity:

```bash
# Entire file — all changes belong to this commit
agstage stage src/auth.rs

# Specific hunks — some hunks belong to different commits
agstage stage abc123def456 789012abcdef

# Line ranges — a single hunk mixes two intents
agstage stage src/main.rs:10-20,35-40

# Exclude — stage a file but skip specific hunks
agstage stage src/main.rs --exclude abc123def456

# Dry-run first for complex multi-selection
agstage stage abc123def456 789012abcdef --dry-run
```

**Granularity decision tree:**
```
Are ALL changes in a file for the same commit?
 YES -> stage PATH
 NO  -> Are changes separated into distinct hunks?
    YES -> stage HUNK_ID (prefer this — content-addressed, robust)
    NO  -> Does a single hunk mix two intents?
       YES -> stage PATH:START-END (most surgical)
```

Prefer hunk IDs over line ranges when possible. Hunk IDs are content-addressed (SHA-256) and robust; line numbers can shift if the file is edited.

### Phase 4: VERIFY

```bash
agstage status
```

Check that the correct files are listed and line counts match expectations. If something is wrong, unstage and retry:

```bash
agstage unstage abc123def456              # Unstage a specific hunk
agstage unstage src/wrong_file.rs         # Unstage entire file
```

### Phase 5: COMMIT + REPEAT

```bash
agstage commit -m "feat: add user authentication"
```

**After each commit, re-scan.** Previous hunk IDs are stale because the index changed:

```bash
agstage scan                              # Get fresh hunk IDs for remaining changes
```

Repeat from Phase 3 for the next commit group until all changes are committed.

---

## CONSTRAINTS AND GOTCHAS

**Whole-file constraint**: Added, Deleted, and Renamed files MUST be staged as whole files. Attempting hunk or line-range selections on them returns exit code 2. This is because added files have no HEAD blob for partial diffing, deleted files must be removed atomically, and renames involve path changes.

**Binary file constraint**: Binary files can only be staged at file level. Hunk or line-range selections return exit code 2.

**Unstage uses a different diff base**: `scan` diffs Index→Workdir (unstaged changes). `unstage` diffs HEAD→Index (staged changes). Hunk IDs from `scan` are NOT valid for `unstage`. Since `agstage status` does not output hunk IDs, prefer file-level unstaging (`agstage unstage src/file.rs`).

**Incremental staging is independently atomic**: Each `stage` call has its own backup. If call 1 succeeds and call 2 fails, call 1's changes remain staged. Call 2 is rolled back to its own backup.

**Re-scan shows reduced hunks**: After staging some hunks from a file, scanning again correctly shows only the remaining unstaged hunks — with NEW hunk IDs. This is correct behavior (Index→Workdir diff excludes already-staged content).

**`--context` affects hunk IDs**: Changing `--context` between scan and stage produces different hunk boundaries and IDs, causing stale-scan errors (exit 3). Use the same `--context` value consistently within a staging session.

---

## ANTI-PATTERNS

- **Don't reuse hunk IDs across file edits or commits** — always re-scan after any modification
- **Don't skip verification** — always run `agstage status` before committing
- **Don't mix `git add` with `agstage stage`** — use one tool per workflow
- **Don't ignore `--dry-run` output** — it catches errors before they happen
- **Don't stage binary files with hunk IDs or line ranges** — binary files only support file-level staging
- **Don't compute line ranges manually** — use hunk IDs from scan whenever possible
- **Don't pipe agstage output through `head`/`tail`/`grep`** — JSON gets truncated and becomes unparseable
- **Don't use `git diff` to "understand changes"** — `agstage scan` already gives structured per-file stats
- **Don't scan the entire repo with `--full`** — use compact scan for overview, filter with positional file args before using `--full`
- **Don't change `--context` between scan and stage** — it produces different hunk IDs, causing exit code 3

---

## COMMIT MESSAGE CONVENTIONS

Format: `type: short description`

Rules:
- Imperative mood ("add", not "added" or "adds")
- No period at the end
- Under 72 characters
- Lowercase after the type prefix
- Describe WHAT and WHY, not HOW

Types: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`, `perf`, `style`, `ci`, `build`

Breaking changes: `feat!: remove deprecated API` or add `BREAKING CHANGE:` in body.

---

## OUTPUT HANDLING

Text mode output is machine-parseable marker records:

`@@agstage:v1 <kind> <minified-json-payload>`

Use `--json` or `--output json` when raw JSON envelopes are required by your workflow.

Parse failures use `command: "cli"` + `phase: "parse"`; runtime failures use `command` for the resolved command and `phase: "runtime"`.

For large repos:
- Default compact scan output is already small (metadata only, no line content)
- Use positional file args to filter: `agstage scan src/auth.rs src/login.rs`
- Use `--full` only on filtered scans, not on the entire repo
- If output is very large, process it programmatically (parse JSON, extract fields)

---

## PERFECT COMMIT CHECKLIST

Before each commit, verify:
- [ ] Scanned with fresh data (no stale hunk IDs)
- [ ] Changes grouped by single logical intent
- [ ] Dry-run passed for complex multi-selection staging
- [ ] `agstage status` confirms correct staged content
- [ ] Commit message follows conventional commits format
- [ ] No unrelated changes included (check file list)
- [ ] No debug/temporary code staged
