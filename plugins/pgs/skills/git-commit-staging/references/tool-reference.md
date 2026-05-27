# pgs MCP Tool Reference

All tools require `repo_path`. Optional `context` defaults to pgs's standard
diff context. If you set `context`, keep it consistent through the session.

| Tool | Required | Optional | Use |
|---|---|---|---|
| `pgs_overview` | `repo_path` | `context` | First read of unstaged plus staged state. |
| `pgs_scan` | `repo_path` | `files`, `full`, `context` | Fresh unstaged file, hunk, and line data. |
| `pgs_stage` | `repo_path`, `selections` | `exclude`, `dry_run`, `explain`, `limit`, `context` | Stage selections, or exact preview with `dry_run=true` and `explain=true`. |
| `pgs_unstage` | `repo_path`, `selections` | `exclude`, `dry_run`, `context` | Remove staged file, hunk, or line selections. |
| `pgs_status` | `repo_path` | `context` | Inspect HEAD-to-Index staged content before committing. |
| `pgs_commit` | `repo_path`, `message` | `amend` | Commit staged changes or amend current HEAD. |
| `pgs_log` | `repo_path` | `max_count`, `paths` | Learn repo commit-message style. |
| `pgs_split_hunk` | `repo_path`, `hunk_id` | `context` | Classify contiguous addition/deletion/mixed runs inside one hunk. |
| `pgs_plan_check` | `repo_path`, `plan` | `context` | Validate a planned split against a fresh scan. |
| `pgs_plan_diff` | `repo_path`, `plan` | `context` | Reconcile a saved plan after edits or commits. |

## Selector Rules

- File path: `src/main.rs`
- Directory path: `src/`
- Hunk ID: `a1b2c3d4e5f6`
- Line range: `src/main.rs:10-20,30-40`
- If a real file path is exactly 12 hex chars, prefix it with `./`.

## Recommended Calls

Initial state:

```text
pgs_overview(repo_path="/path/to/repo")
```

Preview exact staged content without mutation:

```text
pgs_stage(
  repo_path="/path/to/repo",
  selections=["src/main.rs:10-20"],
  dry_run=true,
  explain=true,
  limit=200
)
```

Commit with a body:

```text
pgs_commit(
  repo_path="/path/to/repo",
  message="fix(auth): handle empty password\n\nReject empty passwords before validation so authentication fails cleanly."
)
```

Message-only HEAD amend:

```text
pgs_commit(
  repo_path="/path/to/repo",
  amend=true,
  message="docs(skill): clarify pgs workflow\n\nExplain the MCP-only staging flow and message gate."
)
```

## CommitPlan Shape

Use this shape for `pgs_plan_check` and `pgs_plan_diff`:

```json
{
  "commits": [
    {
      "id": "c1",
      "title": "fix(auth): handle empty password",
      "selections": ["src/auth.rs:a1b2c3d4e5f6"],
      "message": "fix(auth): handle empty password\n\nReject empty passwords before validation."
    }
  ]
}
```

Plan tools validate and reconcile; they do not stage, unstage, or commit.

