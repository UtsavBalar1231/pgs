# pgs

pgs stages git changes at file, hunk, and line granularity without a TTY, so an
AI agent can build clean, atomic commits from a messy working tree. `git add -p`
needs an interactive terminal; pgs gives every change a content-addressed hunk ID
and stages it directly into the index, with an automatic backup that is restored
if an operation fails.

This plugin installs the `pgs-mcp` MCP server and the `git-commit-staging` skill.

## MCP tools

| Tool | Purpose |
|------|---------|
| `pgs_scan` | List unstaged changes with hunk IDs and file checksums |
| `pgs_status` | Show what is staged (HEAD to index) |
| `pgs_overview` | Unstaged and staged changes in one call |
| `pgs_stage` | Stage a file, a hunk ID, or a line range, with optional dry-run preview |
| `pgs_unstage` | Remove a file, hunk, or line range from the index |
| `pgs_commit` | Commit the staged changes, or amend HEAD |
| `pgs_log` | Read recent history to match the repository's message style |
| `pgs_split_hunk` | Classify the addition, deletion, and mixed runs inside a hunk |
| `pgs_plan_check` | Validate a commit plan against the current changes |
| `pgs_plan_diff` | Reconcile a saved commit plan after the tree has changed |
| `pgs_plan_apply` | Stage and commit every planned commit of a clean plan, in order |

## Bundled skill

`git-commit-staging` teaches the agent the scan, stage, and commit workflow, how
to pick the narrowest selector, and how to write well-formed commit messages. It
is preferred over raw `git add -p`, which an agent cannot drive.

## First launch

The plugin ships no binary. On first launch it downloads the prebuilt `pgs-mcp`
that matches the plugin version from this repository's GitHub Releases and caches
it in the plugin data directory. The directory is the first of these that is set:

1. `PGS_PLUGIN_DATA`
2. `PLUGIN_DATA`
3. `CLAUDE_PLUGIN_DATA`
4. `$XDG_DATA_HOME/pgs-plugin`, which defaults to `~/.local/share/pgs-plugin`

Later launches reuse the cached binary until the plugin version changes. The
launcher needs `curl` and a POSIX `sh`.

## Platforms

- Linux: x86_64 and aarch64
- macOS: universal binary for Intel and Apple silicon
- Windows: x86_64 and arm64, launched through the `sh` that ships with Git Bash

## Privacy

pgs runs locally over stdio and never sends data anywhere. The only network
access is the one-time binary download from GitHub Releases described above. After
that, every operation is a local read or write of the repository you point it at.

## Links and license

Source, CLI documentation, and issue tracker:
<https://github.com/UtsavBalar1231/pgs>

Released under the MIT License. See `LICENSE`.
