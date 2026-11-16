# Why pgs?

[← Project overview](../README.md)

`git add -p` can be driven headlessly by piping answers into it, so lack of a
TTY is not the problem. The problem is what the answers mean: they are
positional, one per hunk in diff order, so any change to the diff shifts every
later answer. A short answer list stops early, exits 0, and leaves only a subset
staged. Hunks have no stable names, so a selection made from one `git diff`
cannot be replayed against the next. Building a patch by hand for
`git apply --cached --unidiff-zero` has its own failure: a patch whose header
names the wrong line is applied at that line, silently, with exit 0.

`pgs` names every hunk by a 12-hex content hash, so a selection means the same
thing until the file changes, and an ID from a stale scan is refused. `--expect`
checksums extend that check to line ranges and refuse stale input with exit 3.
It builds each index blob in-process with libgit2 rather than shelling out to
`git`, backs up the index before every mutation, and restores it when anything
fails. The MCP server and the skill let an agent do all of this without
touching a shell.

## Related tools

| Tool | Granularity | How you select | Notes |
|------|-------------|----------------|-------|
| `git add -p` piped, `git add -e`, `git apply --cached` | hunk, line | positional answers, or a hand-built patch or edit | Scriptable, but answers and patch headers carry no stable names and a wrong position can apply with exit 0. |
| GitButler `but` | hunk | hunk IDs | Works inside GitButler's workspace model rather than the plain git index. |
| Jujutsu `jj split` | file, when run non-interactively | paths | Operates on jj changes. Hunk-level splits need the interactive diff editor. |
| [git-surgeon](https://github.com/raine/git-surgeon) | hunk, line | hunk IDs hashed from file path and content, `id:range` | Also commits, splits, folds, and rewords. |
| [wkentaro/git-hunk](https://github.com/wkentaro/git-hunk) | hunk, line | SHA-256 hunk IDs, line numbers | Ships an agent skill and a published agent eval. |
| [Roasbeef/hunk](https://github.com/Roasbeef/hunk) | line | `file:range` | Builds a unified diff for the range and applies it with `git apply --cached`. |
| pgs | file, hunk, line | path, 12-hex hunk ID, or `path:range` | Builds index blobs in-process, refuses stale input, ships an MCP server and a skill. |

This table describes what each project documents about itself. Check their
repositories for current behavior.
