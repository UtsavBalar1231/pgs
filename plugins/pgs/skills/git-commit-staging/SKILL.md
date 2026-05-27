---
name: git-commit-staging
description: >
  Use when staging, unstaging, splitting commits, creating atomic commits,
  amending HEAD messages, or committing specific files, hunks, or line ranges
  through the pgs MCP server.
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

Use pgs MCP tools for repository diff, staging, unstaging, commit planning, and
commit creation. Do not use Bash, raw git, or pgs CLI commands for this workflow.

## Mandatory Workflow

1. Start with `pgs_overview(repo_path, context?)` to see both unstaged and staged
   changes. Use `pgs_scan` when you only need fresh unstaged hunk IDs.
2. Plan commit groups before staging. Group by intent, not by filename.
3. For complex or multi-commit work, validate the plan with `pgs_plan_check`.
4. Stage with the narrowest honest selector:
   - whole file for added, deleted, renamed, binary, or single-intent files;
   - hunk ID for independent hunks;
   - line range only when one hunk contains mixed intent.
5. For exact content preview, call
   `pgs_stage(dry_run=true, explain=true, limit=200, ...)`. Never use a CLI
   preview fallback.
6. Verify staged content with `pgs_status` before every `pgs_commit`.
7. After every `pgs_commit`, `pgs_unstage`, file edit, or index-changing
   operation, refresh state before reusing hunk IDs.

## Message quality gate

Before every `pgs_commit`, run this gate:

1. Call `pgs_status` and summarize the staged files and line counts.
2. Call `pgs_log(max_count=10)` and use repo style first.
3. If recent history is unclear, use the Conventional Commits fallback.
4. Compare the subject and body to the staged content. If they do not match,
   rewrite the message before committing.
5. Body is required for non-trivial commits: 2+ files, 10+ affected lines,
   behavior changes, public API changes, or any non-trivial amend.

Subject format fallback:

```text
<type>(<optional-scope>): <imperative subject under 72 chars>

<body explaining what changed and why>
```

Allowed fallback types: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`,
`perf`, `style`, `ci`, `build`.

## Tool Selection

- `pgs_overview`: first read of mixed staged/unstaged state.
- `pgs_scan`: fresh unstaged hunk IDs and optional full line inspection.
- `pgs_stage`: mutate the index, or preview with `dry_run=true`.
- `pgs_unstage`: remove staged file, hunk, or line selections from the index.
- `pgs_status`: inspect exactly what would be committed.
- `pgs_commit`: create a commit; use `amend=true` only to rewrite current HEAD.
- `pgs_log`: match existing commit-message style.
- `pgs_split_hunk`: classify contiguous runs in a mixed hunk.
- `pgs_plan_check`: validate a planned split before staging.
- `pgs_plan_diff`: reconcile a saved plan after edits or commits.

Full tool details: `references/tool-reference.md`.
Capability boundaries: `references/capability-table.md`.
Commit-message examples: `references/commit-message-guide.md`.

## Core Constraints

- Read MCP JSON-RPC data from `structuredContent`; `content` is only a human
  summary.
- `repo_path` is required for every tool call.
- If you pass custom `context`, use the same value for scan, split, plan, stage,
  and unstage calls in that planning session.
- Hunk IDs are content-addressed and stale after edits, commits, or index
  changes. Refresh before reuse.
- `pgs_scan` reads Index-to-Workdir. `pgs_status` and `pgs_unstage` operate on
  HEAD-to-Index. Do not mix hunk IDs across those bases.
- `Added`, `Deleted`, `Renamed`, and binary files require whole-file staging.
- `whitespace_only` is metadata. The agent still decides whether the change
  belongs in the commit.

## Recovery Rules

- `outcome="no_effect"`: inspect current state; the change may already be
  staged, unstaged, or committed.
- `pgs_error.kind="user"`: fix the selector or request shape.
- `pgs_error.retryable=true`: refresh with `pgs_scan` or `pgs_overview`, then
  retry with fresh selectors.
- If staged content is wrong before commit, use `pgs_unstage`, re-scan, re-plan,
  and stage again.
- If only the current HEAD message is wrong and `pgs_status` is clean, call
  `pgs_commit(amend=true, message=<full replacement message>)`.
- For history rewrites beyond HEAD amend, stop and ask the user. Do not use raw
  git from this skill.

