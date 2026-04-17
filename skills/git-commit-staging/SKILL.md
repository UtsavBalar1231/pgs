---
name: git-commit-staging
description: >
  Non-interactive git staging at file, hunk, or line-range granularity via MCP tools.
  Use when staging changes, splitting commits, creating atomic commits,
  selective staging, or committing specific hunks/lines.
  Use when user mentions "commit", "stage", "split commits", "granular staging",
  "atomic commits", or "selective staging". Requires pgs MCP server.
allowed-tools:
  - pgs_scan
  - pgs_stage
  - pgs_unstage
  - pgs_status
  - pgs_commit
  - pgs_log
---

# Git Commit Staging with pgs

Non-interactive git staging at file, hunk, or line-range granularity. Use when splitting
mixed-intent changes into separate commits. All operations go through pgs MCP tools —
no Bash, no CLI binaries, no raw git commands.

---

## 0. Capability Truth Table — what pgs promises and what it does not

Before proposing a pgs improvement, check this table. Features in the left column already exist; features in the right column are real gaps tracked in `TODO.md`, not invented per-session.

| Promises (already shipped)                                                                                         | Non-promises (current gaps)                                             |
|--------------------------------------------------------------------------------------------------------------------|-------------------------------------------------------------------------|
| Content-addressed hunk IDs — `compute_hunk_id` at `src/git/diff.rs:379`                                            |                                                                         |
| Descriptive hunk run classification — `suggest_splits` at `src/git/diff.rs:211` (exposed as `pgs split-hunk` / `pgs_split_hunk`) |                                                                         |
| Freshness-validated staging — `validate_freshness` at `src/selection/resolve.rs:248`                               | No automatic selector remap after content changes                       |
| Structured JSON via `structured_content` — `structured_tool_result` at `src/mcp/contract.rs:764`                   | No message-rewrite workflow (amend/rebase are outside pgs's MCP surface) |
| Typed MCP tool outputs via macro — `define_tool_output!` at `src/mcp/contract.rs:281`                              |                                                                         |
| Exact-content dry-run preview via `--dry-run --explain` — `preview_stage` at `src/git/staging.rs` and `OperationPreview` in `src/models.rs` |                                                                         |
| Multiline commit bodies — `repository.commit(...)` at `src/cmd/commit.rs:34` passes `args.message` through intact  |                                                                         |
| Whole-file constraints for `Added`, `Deleted`, `Renamed`, and binary files                                         |                                                                         |
| Distinct diff bases per command — scan `Index→Workdir`, status `HEAD→Index`, unstage `HEAD→Index`                  |                                                                         |

Before proposing a pgs improvement, check this table. Features in the left column already exist; features in the right column are real gaps tracked in `TODO.md`, not invented per-session.

---

## 1. Core Rules

### Tool guarantees

What pgs promises so the agent does not have to reason about it:

- Content-addressed hunk IDs stable across unchanged rescans (`compute_hunk_id` at `src/git/diff.rs:379`; stability proven by `hunk_ids_stable_across_rescans` at `src/git/diff.rs:619`).
- Freshness validation with a `StaleScan` error and recovery guidance (`validate_freshness` at `src/selection/resolve.rs:248`).
- File / hunk / line selector resolution (auto-detected from positional syntax).
- Structured JSON output on every MCP call via `structured_content` (`structured_tool_result` at `src/mcp/contract.rs:764`).
- Whole-file constraints for `Added`, `Deleted`, `Renamed`, and binary files (hunk or line selectors on these produce a `user` error).
- Distinct diff bases per command — `pgs_scan` is `Index→Workdir`; `pgs_status` is `HEAD→Index`; `pgs_unstage` matches `HEAD→Index`.

### Agent responsibilities

What the agent must decide — the tool cannot do this for you:

- Infer commit intent from hunk headers and content.
- Group commits honestly by intent.
- Detect when atomicity is not safely achievable (see §5 "When atomicity is impossible").
- Choose between a split and an honest-merge commit per §5.
- Write high-quality commit messages that match actual staged content.
- Re-scan after every content change before referencing hunk IDs.

### Operational rules

1. Always call `pgs_scan` before staging — hunk IDs are ephemeral, valid only until the file or index changes.
2. Re-scan after each `pgs_commit` — the index changed, previous hunk IDs are stale.

   Hunk IDs are a SHA-256 of path + start lines + content (`compute_hunk_id` at `src/git/diff.rs:379`). If any of those change, the ID must change. The problem is never "unstable IDs" — it is that your captured selector now points at content that no longer exists. Re-scan, re-plan, continue. Stability across *unchanged* rescans is proven by `hunk_ids_stable_across_rescans` at `src/git/diff.rs:619`.
3. Plan all commits before staging — group changes by intent, each group becomes one commit.
4. Verify with `pgs_status` before every `pgs_commit`.
5. Use only pgs MCP tools for all diff/staging/history operations — no Bash, no raw git.
6. On error with `pgs_error.retryable: true`: re-scan with `pgs_scan`, then retry with fresh IDs.
7. `repo_path` is required for every tool call. Use the absolute path to the project root (the directory containing `.git`). In Claude Code, this is typically the current working directory.

---

## 2. Tool Reference

| Tool | Description | Required Params | Optional Params |
|------|-------------|-----------------|-----------------|
| `pgs_scan` | Inspect unstaged changes (Index to Workdir) | `repo_path` | `files`, `full`, `context` |
| `pgs_stage` | Stage selections into the index | `repo_path`, `selections` | `exclude`, `dry_run`, `context` |
| `pgs_unstage` | Remove selections from the index | `repo_path`, `selections` | `exclude`, `dry_run`, `context` |
| `pgs_status` | Show staged changes (HEAD to Index) | `repo_path` | `context` |
| `pgs_commit` | Create a commit from staged changes | `repo_path`, `message` | — |
| `pgs_log` | Recent commit history | `repo_path` | `max_count`, `paths` |

### Selection syntax (positional, auto-detected)

| Pattern | Detection Rule | Example |
|---------|---------------|---------|
| File path | Anything not matching other rules | `src/main.rs` |
| Hunk ID | Exactly 12 hex characters | `a1b2c3d4e5f6` |
| Line range | Path contains `:` followed by digit | `src/main.rs:10-20,30-40` |
| Directory | Ends with `/` | `tests/` |

Edge case: if a file path is exactly 12 hex chars, prefix with `./`.

### Parameter semantics

**`context`** — Omit for the tool default. If you pass a value, pass the **same** value in every call in the session (scan → stage → unstage). Mismatched `context` produces different hunk IDs and a retryable `StaleScan` error. The tool is correct; the call sequence was inconsistent.

**`exclude`** — Use when you want a directory selection minus specific files. Example:

```
pgs_stage(repo_path="/path/to/repo", selections=["src/"], exclude=["src/secrets.rs"])
```

**Rename** — When `status.type` is `Renamed`, stage the file as a whole unit via its *current* path (the top-level `path` field). The `old_path` field is informational only and must not be used as a stage target. Hunk or line selectors on a renamed file will produce a `user` error per the whole-file constraint.

---

## 3. Reading Tool Responses

> **Always read from `structured_content`. The `content` field is a human summary — never parse it. If you find yourself proposing "add structured JSON to MCP output", you are re-inventing a feature that already ships (`define_tool_output!` at `src/mcp/contract.rs:281`, `structured_tool_result` at `src/mcp/contract.rs:764`). See §0 Capability Truth Table.**

Every pgs MCP tool returns two payloads:

- `structured_content` — the typed JSON envelope. **Read all data from here.**
- `content` — one-line human summary. Informational only — do not parse it.

### Envelope structure

```json
{
  "outcome": "ok | no_effect | error",
  "pgs": { ... },
  "pgs_error": { ... }
}
```

- `outcome: "ok"` — success. Read data from `pgs`.
- `outcome: "no_effect"` — completed but matched nothing. `pgs` is absent; `pgs_error` explains why.
- `outcome: "error"` — failure. `pgs` is absent; `pgs_error` has `kind`, `code`, `message`, `retryable`, and `guidance`.

### Compact scan response

```json
{
  "outcome": "ok",
  "pgs": {
    "version": "v1",
    "command": "scan",
    "detail": "compact",
    "files": [
      {
        "path": "src/auth.rs",
        "status": { "type": "Modified" },
        "binary": false,
        "hunks_count": 2,
        "lines_added": 15,
        "lines_deleted": 3,
        "hunks": [
          {
            "id": "a1b2c3d4e5f6",
            "old_start": 10,
            "old_lines": 5,
            "new_start": 10,
            "new_lines": 7,
            "header": "@@ -10,5 +10,7 @@ fn authenticate",
            "additions": 2,
            "deletions": 0
          },
          {
            "id": "f6e5d4c3b2a1",
            "old_start": 50,
            "old_lines": 8,
            "new_start": 52,
            "new_lines": 5,
            "header": "@@ -50,8 +52,5 @@ fn validate",
            "additions": 0,
            "deletions": 3
          }
        ]
      },
      {
        "path": "src/utils.rs",
        "status": { "type": "Added" },
        "binary": false,
        "hunks_count": 1,
        "lines_added": 20,
        "lines_deleted": 0,
        "hunks": [
          {
            "id": "1a2b3c4d5e6f",
            "old_start": 0,
            "old_lines": 0,
            "new_start": 1,
            "new_lines": 20,
            "header": "@@ -0,0 +1,20 @@",
            "additions": 20,
            "deletions": 0
          }
        ]
      }
    ],
    "summary": {
      "total_files": 2,
      "total_hunks": 3,
      "added": 1,
      "modified": 1,
      "deleted": 0,
      "renamed": 0,
      "binary": 0,
      "mode_changed": 0
    }
  }
}
```

Key fields:
- `files[].hunks[].id` — the 12-hex hunk ID to pass to `pgs_stage`
- `files[].hunks[].header` — function/section context after the `@@` markers
- `files[].hunks_count` — how many hunks this file has (drives granularity decision)
- `files[].status.type` — `Added`, `Modified`, `Deleted`, `Renamed` (Renamed also carries `old_path`)
- `summary.mode_changed` — count of file permission-only changes

### Full scan response (with `full: true`)

When `full: true` is passed, hunks include a `lines` array and a `checksum` field. Each line has `line_number`, `origin` (`Context`, `Addition`, or `Deletion`), and `content`. Use full scan only on filtered scans (pass `files: ["path"]`), not the whole repo.

### Stage response

```json
{
  "outcome": "ok",
  "pgs": {
    "version": "v1",
    "command": "stage",
    "status": "ok",
    "items": [
      { "selection": "a1b2c3d4e5f6", "lines_affected": 2 }
    ],
    "warnings": [],
    "backup_id": "backup-550e8400-e29b-41d4-a716-446655440000"
  }
}
```

`status` is `"dry_run"` when `dry_run: true` was passed. `backup_id` is a UUID string when the index was mutated, or absent on dry runs.

### Status response

```json
{
  "outcome": "ok",
  "pgs": {
    "version": "v1",
    "command": "status",
    "files": [
      {
        "path": "src/auth.rs",
        "status": { "type": "Modified" },
        "lines_added": 2,
        "lines_deleted": 0
      }
    ],
    "summary": {
      "total_files": 1,
      "total_additions": 2,
      "total_deletions": 0
    }
  }
}
```

Note: `pgs_status` reflects HEAD-to-Index (what is staged). It does not emit hunk IDs — use file-level selections with `pgs_unstage`.

### Commit response

```json
{
  "outcome": "ok",
  "pgs": {
    "version": "v1",
    "command": "commit",
    "commit_hash": "abc123def456abc123def456abc123def456abc1",
    "message": "fix: correct authentication check",
    "author": "Dev <dev@example.com>",
    "files_changed": 1,
    "insertions": 2,
    "deletions": 0
  }
}
```

### Log response

```json
{
  "outcome": "ok",
  "pgs": {
    "version": "v1",
    "command": "log",
    "commits": [
      {
        "hash": "abc123def456abc123def456abc123def456abc1",
        "short_hash": "abc123def456",
        "author": "Dev <dev@example.com>",
        "date": "2026-03-25T10:00:00Z",
        "message": "fix: correct authentication check"
      }
    ],
    "total": 1,
    "truncated": false
  }
}
```

`truncated: true` means the walk limit was reached; use `max_count` to narrow the range.

### Error response

```json
{
  "outcome": "error",
  "pgs_error": {
    "kind": "retryable",
    "code": "stale_scan",
    "message": "hunk checksum mismatch: index changed since last scan",
    "exit_code": 3,
    "retryable": true,
    "guidance": "Re-run pgs_scan to refresh checksums and hunk IDs, then retry."
  }
}
```

---

## 4. Commit Planning & Granularity

Every staging session follows three phases: scan, plan, execute.

### Phase 1 — Scan

Call `pgs_scan` to discover all changes. This is the source of truth for hunk IDs.

### Phase 2 — Plan commits

Before staging anything, analyze the scan output and write a commit plan.
For each file, read `hunks_count` and each hunk's `header` to determine intent.

**Fill in this template** (one entry per commit group):

```
Commit 1 (type): file_a, file_b hunk ID — description of logical change
  Evidence: [why these changes belong together]
Commit 2 (type): file_c — description
  Evidence: [why this is a separate concern]
```

Example for a scan showing one modified file with two hunks plus one added file:

```
Commit 1 (fix): src/auth.rs hunk a1b2c3d4e5f6 — null check in fn authenticate
  Evidence: header shows fn authenticate, additions only, isolated bug fix
Commit 2 (refactor): src/auth.rs hunk f6e5d4c3b2a1 + src/utils.rs — extract validation
  Evidence: header shows fn validate, refactoring; utils.rs is new file supporting the extraction
```

If all changes belong to one logical intent, write that as a single-group plan.
Do not assume single intent — verify by reading the hunk headers.

### Phase 3 — Per-file granularity

Choose the staging granularity based on file state and hunk count:

| Condition | Action | Tool Call |
|-----------|--------|-----------|
| `status.type` is `Added`, `Deleted`, or `Renamed` | File-level (required) | `pgs_stage(repo_path, selections=["path"])` |
| `hunks_count` = 1, change belongs to this commit | File-level | `pgs_stage(repo_path, selections=["path"])` |
| `hunks_count` >= 2, all hunks same intent | File-level | `pgs_stage(repo_path, selections=["path"])` |
| `hunks_count` >= 2, hunks belong to different commits | Hunk-level — use `id` from scan | `pgs_stage(repo_path, selections=["a1b2c3d4e5f6"])` |
| Single hunk mixes two intents (rare) | Line-level — use full scan first | `pgs_stage(repo_path, selections=["path:10-20"])` |

How to determine intent from compact scan:
- Read the `header` field — it shows the function/section name (e.g., `@@ -10,5 +10,7 @@ fn authenticate`).
- If headers show different functions, the hunks likely belong to different commits.
- When unsure, call `pgs_scan` with `files: ["path"]` and `full: true` to inspect actual line changes.

Prefer hunk IDs over line ranges. Use line ranges only when a single hunk contains two distinct intents.

### Phase 4 — Using `dry_run` meaningfully

`dry_run: true` on `pgs_stage` confirms selector applicability and reports line counts (via `estimate_lines` at `src/cmd/stage.rs:406`). For exact-content verification before staging, run `pgs stage --dry-run --explain` — the preview shows the resolved line numbers and content that will land in the index, per file, without mutating anything. See `preview_stage` at `src/git/staging.rs`. (The count-only `dry_run` without `--explain` remains a valid sanity check when exact content does not matter.)

---

## 5. Workflows

### Multi-commit workflow (default)

Scenario: `src/auth.rs` has a bug fix (hunk in `fn authenticate`) and a refactor (hunk in `fn validate`). New file `src/utils.rs` was added.

**Step 1 — Scan:**

```
pgs_scan(repo_path="/path/to/repo")
```

Response `pgs.files`:
- `src/auth.rs`: `Modified`, `hunks_count: 2`, hunks `a1b2c3d4e5f6` (fn authenticate) and `f6e5d4c3b2a1` (fn validate)
- `src/utils.rs`: `Added`, `hunks_count: 1`, hunk `1a2b3c4d5e6f`

**Step 2 — Write commit plan:**

```
Commit 1 (fix): src/auth.rs hunk a1b2c3d4e5f6 — bug fix in fn authenticate
  Evidence: header shows fn authenticate, additions only, 2 lines added
Commit 2 (refactor): src/auth.rs hunk f6e5d4c3b2a1 + src/utils.rs — extract validation
  Evidence: header shows fn validate, refactoring; utils.rs supports the extraction
```

Two groups — two commits. Execute one at a time.

**Step 3 — Stage commit 1:**

```
pgs_stage(repo_path="/path/to/repo", selections=["a1b2c3d4e5f6"])
```

**Step 4 — Verify:**

```
pgs_status(repo_path="/path/to/repo")
```

Confirm `pgs.files` shows only `src/auth.rs` with 2 additions.

**Step 5 — Commit:**

```
pgs_commit(repo_path="/path/to/repo", message="fix: correct authentication check\n\nAdd null guard before password validation to prevent panic on empty input.")
```

**Step 6 — Re-scan with fresh IDs:**

```
pgs_scan(repo_path="/path/to/repo")
```

The validate hunk ID has changed (e.g., from `f6e5d4c3b2a1` to `99887766aabb`). This is why re-scanning is mandatory after every commit.

**Step 7 — Stage commit 2 using fresh IDs:**

```
pgs_stage(repo_path="/path/to/repo", selections=["src/auth.rs", "src/utils.rs"])
```

`src/auth.rs` now has 1 hunk (all same intent), so file-level staging is correct.

**Step 8 — Verify and commit:**

```
pgs_status(repo_path="/path/to/repo")
pgs_commit(repo_path="/path/to/repo", message="refactor: extract validation logic to utils\n\nMoves the shared validation helper out of auth.rs to eliminate\nduplication across the module boundary.")
```

### Single-commit shortcut

Use only when all of the following are true:
- Every changed file belongs to the same logical change.
- You verified this by reading the scan output headers (not assumed from file names).
- The commit type is clear.

```
Step 1: pgs_scan(repo_path="/path/to/repo")
        → read all headers, confirm single intent
Step 2: pgs_stage(repo_path="/path/to/repo", selections=["file1", "file2"])
Step 3: pgs_status(repo_path="/path/to/repo")
        → confirm staged content
Step 4: pgs_commit(repo_path="/path/to/repo", message="feat: ...")
```

### When atomicity is impossible

If `pgs_stage` with `dry_run: true` still shows unintended changes after line-range staging, **do not fake atomicity**. Merge the intertwined changes into one honest commit and rename the subject and body to match the actual content. A fictitious atomic split — subject claims one thing, diff contains another — is worse than an honest non-atomic commit, because reviewers and `git bisect` trust the commit boundary to mean something.

Scratch files (local TODO notes, session logs, AI-assistant artifacts) should be left unstaged or added to `.gitignore` — do not include them in intentional commits, and do not spend a commit boundary on them.

---

## 6. Constraints & Error Recovery

- **Whole-file constraint**: `Added`, `Deleted`, and `Renamed` files must be staged as whole files. Hunk or line selections on these produce a `user` error.
- **Binary constraint**: Binary files can only be staged at file level. Hunk or line selections produce a `user` error.
- **Stale hunk IDs**: After any commit, file edit, or index change, all hunk IDs are stale. Always re-scan before staging.
- **Context consistency**: If you pass a custom `context` value, use the same value in both scan and stage calls. Mismatches produce different hunk IDs and a `retryable` error.
- **Different diff bases**: `pgs_scan` diffs Index-to-Workdir. `pgs_unstage` operates on HEAD-to-Index. Hunk IDs from `pgs_scan` are not valid for `pgs_unstage` — use file-level selections with `pgs_unstage`.

### Error recovery table

| `outcome` | `pgs_error.kind` | Meaning | Recovery |
|-----------|------------------|---------|----------|
| `ok` | — | Success | Continue |
| `no_effect` | `no_effect` | Nothing matched | Check repo state; may already be staged or committed |
| `error` | `user` | Bad input | Fix selection (check: binary? whole-file constraint? wrong ID?) |
| `error` | `retryable` | Stale/conflict | Re-call `pgs_scan`, retry with fresh IDs |
| `error` | `internal` | Server error | Retry once; if failure persists, inspect repository state |

---

## 7. Anti-Patterns

| Don't | Do Instead |
|-------|-----------|
| Use Bash for pgs operations | Use pgs MCP tools directly |
| Reuse hunk IDs after `pgs_commit` | Re-call `pgs_scan` to get fresh IDs |
| Call `pgs_scan` with `full: true` on the whole repo | Pass `files: ["path"]` to filter first |
| Skip the commit plan step | Write the plan before any staging call |
| Stage all files without analyzing headers | Read hunk headers, verify intent per file |
| Assume all changes are one commit | Read headers — N distinct groups means N commits |
| Write one-liner commit messages for non-trivial changes | Include a body explaining what and why |

---

## 8. Commit Message Convention

### Format

```
<type>(<optional-scope>): <subject>

<body>
```

**Subject line**: imperative mood, under 72 chars, no period.

**Types**: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`, `perf`, `style`, `ci`, `build`.

**Breaking changes**: `feat!: description` or include `BREAKING CHANGE:` in the body.

### Body (required for non-trivial changes)

- Explain **what** changed and **why**, not how the code works.
- Wrap lines at 72 chars.
- Use bullet points for multiple distinct changes.

Body is required when: 2+ files changed, 10+ lines affected, behavior changes, or breaking change introduced.

Body is optional for: formatting-only changes, dependency bumps, typo fixes.

### Good example

```
feat(scan): add binary file detection to compact output

The compact scan output now includes a `binary` field per file entry
so callers can skip granular staging for binary files without needing
a full scan.

- Add binary field to CompactFileInfo
- Update ScanFileView::from_compact to propagate the flag
- Skip hunk enumeration for binary files
```

### Bad examples

```
fix: fix the bug
feat: add new feature
update files
```

### Check recent style before writing

Call `pgs_log` before writing commit messages to see the style used in this repository:

```
pgs_log(repo_path="/path/to/repo", max_count=10)
```

Read `pgs.commits[].message` and match the format, type vocabulary, and subject style already established.

---

## 9. Repairing a bad split

A "bad split" is any staging session whose commit boundary no longer matches the actual staged diff: wrong files staged, hunks grouped under the wrong intent, or the commit subject describing something different from what landed. Real sessions hit this. Repair steps that fall outside `pgs` are a deliberate scope boundary, not a missing feature.

### Recognition

- `pgs_status` shows files that do not belong to the staged intent, or unexpected line counts.
- The planned commit subject no longer matches the staged content after re-reading the `pgs_status` output.
- A post-commit review reveals the commit message references a change that the commit does not contain.

### Rewind options

- **Pre-commit (staged but not yet committed)**: call `pgs_unstage` with file-level selections to remove the incorrect selections from the index. Re-verify with `pgs_status`. Then re-stage correctly.
- **Post-commit (already committed)**: `git reset --soft <good-commit>` re-opens the index with the bad commit's changes still staged. **This is a git fallback outside pgs's MCP surface.** Once the index is re-opened, use `pgs_unstage` / `pgs_stage` to regroup.

### Rebuild

Re-scan (`pgs_scan`) to get fresh hunk IDs, re-plan the tail of the work, then re-stage. Do not reuse stale IDs from the pre-rewind session.

### Message-only fix

If the staged tree is correct but only the commit *message* is wrong, `git commit --amend` rewrites the message. **This is also outside pgs's MCP surface.** pgs's commit flow ends at `pgs_commit`; anything past that belongs to git.

Repair steps that fall outside `pgs` are a deliberate scope boundary, not a missing feature.
