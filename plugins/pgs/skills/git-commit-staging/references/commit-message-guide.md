# Commit Message Guide

Use repo style first. If `pgs_log(max_count=10)` shows a clear local convention,
match that convention. If it does not, use the Conventional Commits fallback.

## Conventional Commits fallback

```text
<type>(<optional-scope>): <imperative subject under 72 chars>

<body explaining what changed and why>
```

Types: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`, `perf`, `style`,
`ci`, `build`.

Body is required when any of these are true:

- 2+ files changed;
- 10+ lines affected;
- behavior changed;
- public API, schema, or tool contract changed;
- the commit amends a non-trivial change;
- the subject would otherwise hide mixed intent.

Body is optional for formatting-only edits, typo fixes, or mechanical metadata
bumps that are obvious from the subject and staged diff.

## Gate Checklist

Before `pgs_commit`:

1. Read `pgs_status`.
2. Read `pgs_log(max_count=10)`.
3. Choose repo style first, or the Conventional Commits fallback.
4. Verify the subject names the staged intent.
5. Verify the body explains what changed and why.
6. Reject vague messages such as `update files`, `fix bug`, or `misc changes`.

## Examples

Good:

```text
feat(stage): expose exact preview through MCP

Add explain and limit fields to pgs_stage so agents can request the existing
dry-run preview without leaving the MCP workflow.
```

Good:

```text
docs(skill): enforce commit message gate

Require agents to inspect staged content and recent history before writing a
commit message, then use repo style before falling back to Conventional Commits.
```

Bad:

```text
fix: fix stuff
```

Bad:

```text
update files
```

