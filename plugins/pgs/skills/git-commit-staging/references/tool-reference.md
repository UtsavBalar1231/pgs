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

### Line ranges and deleted lines

A line range names workdir line numbers, so a deleted line has no number you can
name. Deletions come along indirectly:

- Where deletions are immediately followed by additions (a replacement), the
  i-th deleted line rides along with the i-th added line and is staged only when
  that added line is in your range. If the replacement deletes more lines than it
  adds, the surplus rides with the last added line.
- Where lines were deleted and nothing was added, the deletion sits in the gap
  between two surviving lines. Your range must cover a surviving line on **each**
  side of the gap. At the top or bottom of the file the gap has one side only, so
  cover the adjacent surviving line plus at least one more line on that side.

Consequences worth planning around: a range naming only unchanged lines stages
nothing and returns `SelectionEmpty`; a range of `1-N` over the whole file always
reproduces the workdir exactly.

Prefer a hunk ID whenever the change involves deletions — hunk IDs and whole-file
selections carry every line of their target, deletions included, with no pairing
rules to reason about.

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

Supply the message through exactly one of `message` or `message_file`:

```text
pgs_commit(
  repo_path="/path/to/repo",
  message="fix(auth): reject empty password\n\nFail authentication cleanly before validation."
)
```

```text
pgs_commit(repo_path="/path/to/repo", message_file="/abs/path/to/msg.txt")
```

Message-only HEAD amend, only when `pgs_status` is clean:

```text
pgs_commit(repo_path="/path/to/repo", amend=true, message="<full replacement message>")
```

Both fields work with `amend=true`. Rules:

- Supplying neither or both is rejected as a JSON-RPC `invalid_params` error
  before the commit runs.
- `message_file="-"` is rejected: an MCP request has no stdin to read from. Pass
  an absolute path or use `message`.
- A `message_file` that is missing, a directory, unreadable, or not valid UTF-8
  returns `input_file_unreadable`. pgs never substitutes replacement characters.
- A message that is empty or whitespace-only is refused
  (`empty_commit_message`). Nothing is written — an `amend` that is refused
  leaves the existing HEAD commit and its message untouched.

pgs normalizes every message before storing it; see
`references/commit-message-guide.md`.

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
