---
name: git-commit-staging
description: >-
  Stage, unstage, split, and commit changes at file, hunk, or line granularity
  through the pgs MCP server, and build clean atomic commits with well-formed
  messages. Use whenever work involves staging or committing in a git repo — e.g.
  "commit just this function", "split this messy diff into separate commits",
  "stage only the bug fix, not the debug prints", "unstage that file", "amend the
  last commit message", or turning a mixed working tree into atomic commits.
  Prefer it over raw git or `git add -p`, which needs a TTY that agents lack.
allowed-tools:
  - pgs_scan
  - pgs_stage
  - pgs_unstage
  - pgs_status
  - pgs_commit
  - pgs_log
  - pgs_overview
  - pgs_split_hunk
  - pgs_plan_check
  - pgs_plan_diff
---

# Git Commit Staging with pgs

Drive every read, staging action, and commit through the pgs MCP tools — never
Bash, raw git, or the pgs CLI.

## Quick reference

| Goal | Call |
|---|---|
| See unstaged + staged state | `pgs_overview` |
| Get fresh hunk IDs / line detail | `pgs_scan` |
| Understand a mixed hunk | `pgs_split_hunk` |
| Validate a multi-commit plan | `pgs_plan_check` |
| Stage or preview a selection | `pgs_stage` |
| Inspect what will be committed | `pgs_status` |
| Create or amend a commit | `pgs_commit` |

## Mandatory workflow

1. Read state with `pgs_overview`. Use `pgs_scan` when you only need fresh
   unstaged hunk IDs or line-level detail.
2. Plan commit groups by intent, not by filename, before staging anything.
3. For multi-commit work, validate the grouping with `pgs_plan_check`.
4. Stage with the narrowest honest selector:
   - whole file for added, deleted, renamed, binary, or single-intent files;
   - hunk ID for an independent hunk;
   - line range only when one hunk mixes intent — use `pgs_split_hunk` to see
     the addition/deletion runs first.
5. To preview exact staged content without mutating the index, call
   `pgs_stage(dry_run=true, explain=true, limit=200, ...)`.
6. Verify the result with `pgs_status` before every `pgs_commit`.
7. After any commit, unstage, file edit, or index change, re-read state before
   reusing hunk IDs — they go stale.

## Guard against drift

Recommended for multi-step sessions. A file can change between the scan and the
stage. To make staging fail loudly instead of staging the wrong lines, capture
each file's `file_checksum` from `pgs_scan` (reported per file in both compact
and full output) and pass them to the stage call:

```text
pgs_stage(selections=[...], expected_checksums={"src/app.rs": "<file_checksum>"})
```

A `StaleScan` error (retryable) means the file drifted — re-scan and re-plan.

## Message quality gate

Before every `pgs_commit`:

1. Call `pgs_status` and summarize the staged files and line counts.
2. Call `pgs_log(max_count=10)` and use repo style first.
3. If recent history is unclear, use the Conventional Commits fallback —
   `<type>(<optional-scope>): <imperative subject>` plus a body.
4. Confirm the subject and body match the staged content; rewrite if they do not.
5. Body is required for non-trivial commits: 2+ files, 10+ affected lines,
   behavior or public-API changes, or any non-trivial amend.

Full gate and worked examples: `references/commit-message-guide.md`.

## Tool map

- `pgs_overview` — first read of mixed staged/unstaged state.
- `pgs_scan` — fresh unstaged file, hunk, and line data.
- `pgs_stage` — stage selections, or preview with `dry_run=true`.
- `pgs_unstage` — remove staged file, hunk, or line selections.
- `pgs_status` — inspect exactly what would be committed.
- `pgs_commit` — create a commit; `amend=true` rewrites only the current HEAD.
- `pgs_log` — read existing commit-message style.
- `pgs_split_hunk` — classify the addition/deletion/mixed runs inside one hunk.
- `pgs_plan_check` — validate a planned split before staging.
- `pgs_plan_diff` — reconcile a saved plan after edits or commits.

Selectors, the `CommitPlan` shape, the drift guard, and worked examples:
`references/tool-reference.md`.

## Constraints

- Read results from JSON-RPC `structuredContent`; the `content` field is only a
  human-readable summary.
- `repo_path` is required on every call. If you set a custom `context`, use the
  same value for every scan, split, plan, stage, and unstage call in the session.
- Hunk IDs are content-addressed and go stale after edits, commits, or index
  changes. Refresh before reuse; pgs rejects stale selectors, it does not remap
  them.
- `pgs_scan` reads Index→Workdir; `pgs_status` and `pgs_unstage` read
  HEAD→Index. Do not reuse hunk IDs across those two bases.
- Added, deleted, renamed, and binary files require whole-file staging.
- `whitespace_only` is metadata; you still decide whether the change belongs in
  the commit.
- `pgs_split_hunk` is descriptive, not a staging plan; `pgs_plan_check` and
  `pgs_plan_diff` validate and reconcile but never stage; `pgs_overview` is
  read-only and does not replace a fresh scan once hunk IDs are stale.
- A count-only dry run carries no preview lines — pass `explain=true`. Binary or
  unsupported previews return empty preview lines with a reason.

## Recovery

- `outcome="no_effect"`: inspect current state; the change may already be staged,
  unstaged, or committed.
- `pgs_error.kind="user"`: fix the selector or request shape.
- `pgs_error.retryable=true` (including `StaleScan`): re-read with `pgs_scan` or
  `pgs_overview`, then retry with fresh selectors and checksums.
- Wrong staged content before commit: `pgs_unstage`, re-scan, re-plan, and stage
  again.
- Only the HEAD message is wrong and `pgs_status` is clean:
  `pgs_commit(amend=true, message=<full replacement message>)`.
- For any history rewrite beyond a HEAD amend, stop and ask the user. Never use
  raw git from this skill.
