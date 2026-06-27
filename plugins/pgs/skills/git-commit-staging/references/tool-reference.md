# pgs MCP tool reference

Workflow detail that the tool schemas do not capture. Every tool requires
`repo_path`; if you set a custom `context`, keep the same value across the
session. Read each result from `structuredContent`, not the `content` summary.

## Selectors

`selections` (and `exclude`) entries are auto-detected:

- File path — `src/main.rs`
- Directory path — `src/`
- Hunk ID — `a1b2c3d4e5f6` (12 hex, content-addressed, from `pgs_scan`)
- Line range — `src/main.rs:10-20,30-40` (1-indexed, inclusive)

If a real file path is exactly 12 hex characters, prefix it with `./` so it is
read as a path, not a hunk ID.

## Guard against drift

Pass `expected_checksums` to `pgs_stage` to reject staging when a file changed
since you scanned it:

```text
pgs_stage(
  repo_path="/path/to/repo",
  selections=["src/main.rs:10-20"],
  expected_checksums={"src/main.rs": "<file_checksum from pgs_scan>"}
)
```

`file_checksum` is reported per file by `pgs_scan` in both compact and full
output. A `StaleScan` error (retryable) means re-scan and re-plan.

## Preview exact staged content

```text
pgs_stage(
  repo_path="/path/to/repo",
  selections=["src/main.rs:10-20"],
  dry_run=true,
  explain=true,
  limit=200
)
```

A plain `dry_run` (no `explain`) returns counts only, with no preview lines.

## Commit and amend

```text
pgs_commit(
  repo_path="/path/to/repo",
  message="fix(auth): reject empty password\n\nFail authentication cleanly before validation."
)
```

Message-only HEAD amend, only when `pgs_status` is clean:

```text
pgs_commit(repo_path="/path/to/repo", amend=true, message="<full replacement message>")
```

## CommitPlan shape

`pgs_plan_check` and `pgs_plan_diff` take a `CommitPlan` you hand-construct:

```json
{
  "commits": [
    {
      "id": "c1",
      "title": "fix(auth): reject empty password",
      "selections": ["src/auth.rs:a1b2c3d4e5f6"],
      "message": "fix(auth): reject empty password\n\nFail cleanly before validation."
    }
  ]
}
```

Plan tools validate and reconcile a plan; they never stage, unstage, or commit.
