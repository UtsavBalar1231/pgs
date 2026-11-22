---
name: git-commit-staging
description: >-
  Stage, unstage, split, and commit changes at file, hunk, or line granularity
  through the pgs MCP server, and build clean atomic commits with well-formed
  messages. Use whenever work involves staging or committing in a git repo, e.g.
  "commit just this function", "split this messy diff into separate commits",
  "stage only the bug fix, not the debug prints", "unstage that file", "amend the
  last commit message", or turning a mixed working tree into atomic commits.
  Prefer it over raw git or `git add -p`: piped `git add -p` answers are
  positional in diff order, a short answer list exits 0 with only part of the
  change staged, and hunks have no stable names to select by.
allowed-tools: >-
  mcp__pgs__pgs_scan mcp__pgs__pgs_stage mcp__pgs__pgs_unstage
  mcp__pgs__pgs_status mcp__pgs__pgs_commit mcp__pgs__pgs_log
  mcp__pgs__pgs_overview mcp__pgs__pgs_split_hunk mcp__pgs__pgs_plan_check
  mcp__pgs__pgs_plan_diff mcp__pgs__pgs_plan_apply mcp__plugin_pgs_pgs__pgs_scan
  mcp__plugin_pgs_pgs__pgs_stage mcp__plugin_pgs_pgs__pgs_unstage
  mcp__plugin_pgs_pgs__pgs_status mcp__plugin_pgs_pgs__pgs_commit
  mcp__plugin_pgs_pgs__pgs_log mcp__plugin_pgs_pgs__pgs_overview
  mcp__plugin_pgs_pgs__pgs_split_hunk mcp__plugin_pgs_pgs__pgs_plan_check
  mcp__plugin_pgs_pgs__pgs_plan_diff mcp__plugin_pgs_pgs__pgs_plan_apply
---

# Git Commit Staging with pgs

Use the pgs MCP tools for inspection, staging, and ordinary commits. Use Git
when commits require hooks, signing, or finishing a merge.

## Quick reference

| Goal | Call |
|---|---|
| See unstaged + staged state | `pgs_overview` |
| Get fresh hunk IDs / line detail | `pgs_scan` |
| Understand a mixed hunk | `pgs_split_hunk` |
| Validate a multi-commit plan | `pgs_plan_check` |
| Commit a validated plan in one call | `pgs_plan_apply` |
| Stage or preview a selection | `pgs_stage` |
| Inspect what will be committed | `pgs_status` |
| Create or amend a commit | `pgs_commit` |

## Workflow

Only stage, unstage, commit, or amend when the user has authorized that action.
For inspection, planning, and preview requests, use read-only operations such
as `pgs_plan_check` and `pgs_stage(dry_run=true, ...)`.

1. Read state with `pgs_overview`. Use `pgs_scan` when you only need fresh
   unstaged hunk IDs or line-level detail. Check `pgs.skipped` in scan output
   when expected paths are missing.
2. Group changes by intent before staging. For multiple commits, follow the
   plan workflow below.
3. Stage with the narrowest selector that covers the intended change:
   - whole file for added, deleted, renamed, binary, or single-intent files;
   - hunk ID for an independent hunk;
   - line range only when one hunk mixes intent — use `pgs_split_hunk` to see
     the addition/deletion runs first.

   A line range names workdir line numbers, so a deleted line has no number to
   name and comes along only indirectly. Prefer a hunk ID whenever the change
   involves deletions; see `references/tool-reference.md` for the pairing rules
   before writing a range over one.
4. To preview exact staged content without mutating the index, call
   `pgs_stage(dry_run=true, explain=true, limit=200, ...)`.
5. Inspect all staged changes with `pgs_status(full=true)` before `pgs_commit`.
   `pgs_commit(selections=[...], message=...)` adds the selections to the index
   and commits everything staged. Use it only after reviewing the selections
   and confirming all existing staged changes belong in the commit. If unrelated
   changes are staged, leave them in place and resolve the scope with the user.
6. After any commit, unstage, file edit, or index change, re-read state before
   reusing hunk IDs — they go stale.

## Plan multiple commits

1. Run `pgs_plan_check` and inspect `structuredContent.pgs` even when
   `outcome="ok"`. Resolve `overlaps`, `unsafe_selectors`, `unknown_paths`, and
   `unknown_hunk_ids`. Review `uncovered` hunks and leave them unstaged only if
   that matches the intended scope.
2. Draft a `message` for each commit using recent history. Run
   `pgs_plan_apply(dry_run=true, ...)` and review its per-commit selections and
   `pgs.warnings`. An active hook produces a warning during preview. Check the
   draft messages against the preview using the message gate below.
3. Apply the plan only after the user has authorized commits, the preview
   matches the intended scope, and nothing is staged. If hooks or signing are
   required, stage, verify, and commit each group in order using Git for the
   commit. Refresh selectors between commits.

## Include ignored files explicitly

`pgs_scan` may report ignored files or directories in `structuredContent.pgs.skipped`.
To inspect them, call `pgs_scan(files=["dist/"], include_ignored=true, ...)` with
explicit file or directory paths. To stage an intentionally ignored file, use
`pgs_stage(selections=["dist/app.js"], force=true, ...)` with exact whole-file
paths. Preview with `dry_run=true` before staging.

## Guard against drift

Recommended for multi-step sessions. A file can change between the scan and the
stage. To make staging fail loudly instead of staging the wrong lines, capture
each file's `checksum` from `pgs_scan` (the file-level field, reported per file
in both compact and full output) and pass them to the stage call:

```text
pgs_stage(selections=[...], expected_checksums={"src/app.rs": "<checksum>"})
```

A `StaleScan` error (retryable) means the file drifted — re-scan and re-plan.

## Message quality gate

Before committing:

1. Inspect `pgs_status(full=true)` for staged changes, or the plan preview for
   planned commits. Summarize the files and line counts for each commit.
2. Call `pgs_log(max_count=10)` and use repo style first.
3. If recent history is unclear, use the Conventional Commits fallback —
   `<type>(<optional-scope>): <imperative subject>` plus a body.
4. Confirm the subject and body match each commit's content.
5. Body is required for non-trivial commits: 2+ files, 10+ affected lines,
   behavior or public-API changes, or any non-trivial amend.

Pass the message as `message`, or as `message_file` pointing at an absolute path
— exactly one of the two. pgs normalizes whitespace in every message before
storing it, so do not hand-pad the text.

Full gate, normalization rules, and worked examples:
`references/commit-message-guide.md`.

## Tool map

- `pgs_overview` — first read of mixed staged/unstaged state.
- `pgs_scan` — fresh unstaged file, hunk, and line data.
- `pgs_stage` — stage selections, or preview with `dry_run=true`.
- `pgs_unstage` — remove staged file, hunk, or line selections. Get staged
  hunk IDs from `pgs_status(full=true)`; `pgs_scan` IDs are unstaged hunks.
- `pgs_status` — inspect exactly what would be committed.
- `pgs_commit` — create a commit; `amend=true` rewrites only the current HEAD.
- `pgs_log` — read existing commit-message style.
- `pgs_split_hunk` — classify the addition/deletion/mixed runs inside one hunk.
- `pgs_plan_check` — validate a planned split before staging.
- `pgs_plan_diff` — reconcile a saved plan after edits or commits.
- `pgs_plan_apply` — stage and commit every planned commit of a clean plan.

Selectors, the `CommitPlan` shape, the drift guard, and worked examples:
`references/tool-reference.md`.

## Constraints

- Read command output from `structuredContent.pgs` and errors from
  `structuredContent.pgs_error`. `content` is a human-readable summary.
- `repo_path` is required on every call. If you set a custom `context`, use the
  same value for every scan, split, plan, stage, and unstage call in the session.
- Hunk IDs are content-addressed and go stale after edits, commits, or index
  changes. Refresh before reuse; pgs rejects stale selectors, it does not remap
  them.
- `pgs_scan` reads Index→Workdir; `pgs_status` and `pgs_unstage` read
  HEAD→Index. Do not reuse hunk IDs across those two bases.
- Added, deleted, renamed, and binary files, and submodule pointers, require
  whole-file staging.
- `whitespace_only` is metadata; you still decide whether the change belongs in
  the commit.
- `pgs_split_hunk` is descriptive, not a staging plan; `pgs_plan_check` and
  `pgs_plan_diff` validate and reconcile but never stage; `pgs_overview` is
  read-only and does not replace a fresh scan once hunk IDs are stale.
- Without `dry_run`, `pgs_plan_apply` writes history. It refuses before writing
  when changes are already staged, a planned commit has no message, or
  `pgs_plan_check` reports overlaps, unsafe selectors, or unknown paths or hunk IDs.
- For exact lines in a stage preview, pass both `dry_run=true` and `explain=true`.
  Binary or unsupported previews return empty preview lines with a reason.

## Recovery

- `pgs_error.code="commit_hooks_present"` or `"commit_signing_required"`: the
  call refuses before staging. Inspect the index, stage and verify the intended
  changes, then commit with Git so hooks or signing run. Preserve `--amend` when
  the user requested an amend. `no_verify` skips hooks only, and requires an
  explicit request to skip them.
- `pgs_error.code="merge_in_progress"`: finish the merge with Git when authorized.

- `pgs_plan_apply` returns `status="partial"` with `isError=true`: the
  `commits` it lists were created and stay; `failed` names the planned commit
  that stopped the run and its error, and the index is back to empty. Fix that
  commit, drop the applied ones from the plan, re-scan for fresh hunk IDs, and
  apply the rest.

- `outcome="no_effect"`: inspect current state; the change may already be staged,
  unstaged, or committed. A line range that names only unchanged lines is also a
  no-op, not a silent success — it returns `SelectionEmpty` and leaves the index
  alone. Widen the range or select the hunk by ID.
- `pgs_error.kind="user"`: fix the selector or request shape.
- `empty_commit_message`: the message was empty or whitespace-only after
  normalization. Nothing was written; a refused amend left HEAD intact. Write a
  real message and retry.
- `input_file_unreadable`: the `message_file` path is missing, a directory,
  unreadable, or not valid UTF-8. Fix the path or pass `message` inline.
- `invalid_params` on `pgs_commit`: you supplied neither or both of `message` and
  `message_file`, or passed `message_file="-"`, which has no stdin over MCP.
  Resend with exactly one source and an absolute path.
- `pgs_error.retryable=true` (including `StaleScan`): re-read with `pgs_scan` or
  `pgs_overview`, then retry with fresh selectors and checksums.
- Wrong staged content before commit: `pgs_unstage`, re-scan, re-plan, and stage
  again.
- Only the HEAD message is wrong and `pgs_status` is clean:
  `pgs_commit(amend=true, message=<full replacement message>)`.
- For any history rewrite beyond a HEAD amend, stop and ask the user.
