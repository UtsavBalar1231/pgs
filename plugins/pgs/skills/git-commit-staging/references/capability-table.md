# pgs Capability Table

Use this table to avoid inventing pgs behavior during commit staging.

| Capability | Boundary |
|---|---|
| Content-addressed hunk IDs via `compute_hunk_id` at `src/git/diff.rs:377`. | IDs are stable only while path, hunk positions, and hunk content stay unchanged. |
| Hunk extraction and whitespace metadata via `extract_hunks` at `src/git/diff.rs:270` and `HunkInfo.whitespace_only` at `src/models/scan.rs:101`. | `whitespace_only` is metadata only; the agent still decides whether to stage it. |
| Descriptive hunk run classification via `suggest_splits` at `src/git/diff.rs:211`, exposed as `pgs_split_hunk`. | Split output is descriptive, not an automatic staging plan. |
| Freshness validation via `validate_freshness` at `src/selection/resolve.rs:248`. | pgs does not automatically remap stale selectors after content changes. |
| JSON-RPC `structuredContent` results via `structured_tool_result` at `src/mcp/contract.rs:712` and typed envelopes from `define_tool_output!` at `src/mcp/contract.rs:320`. | Do not parse the human `content` summary. |
| Exact stage preview via `preview_stage` at `src/git/staging.rs:250`, returned through `pgs_stage(dry_run=true, explain=true)`. | Count-only dry runs do not include preview lines. |
| Unified state read via `pgs_overview`, backed by `cmd::overview::execute` at `src/cmd/overview.rs:9`. | It is read-only and does not replace a fresh scan when hunk IDs have gone stale. |
| Plan validation via `pgs_plan_check`, backed by `cmd::plan_check::execute` at `src/cmd/plan_check.rs:35` and `CommitPlan` at `src/models/plan.rs:11`. | Reports overlaps, uncovered hunks, unsafe selectors, and unknown paths; it does not rewrite plans. |
| Plan reconciliation via `pgs_plan_diff`, backed by `cmd::plan_diff::execute` at `src/cmd/plan_diff.rs:36`. | Reports `still_valid`, `shifted`, and `gone`; shift detection is descriptive. |
| HEAD amend/message rewrite via `pgs_commit(amend=true)`, using `CommitArgs.amend` at `src/cmd/commit.rs:15` and `CommitToolInput.amend` at `src/mcp/contract.rs:162`. | No history editing beyond current HEAD amend: no rebase, reset, cherry-pick, or older-commit rewrite. |
| Multiline commit messages pass through `args.message` at `src/cmd/commit.rs:62` and `src/cmd/commit.rs:70`. | pgs does not judge message quality; the skill must enforce the gate. |
| Stage preview schema is `OperationPreview` at `src/models/preview.rs:14`. | Binary or unsupported preview entries may return an empty preview with a reason. |

