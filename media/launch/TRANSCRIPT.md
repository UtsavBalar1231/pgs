# On-screen transcript

The launch film runs for 35 seconds. It has music and sound effects, with no
voiceover. Line breaks below describe the layout; the headlines read as
sentences across those breaks.

| Time | Headline | Supporting copy |
|------|----------|-----------------|
| 0 to 4.5s | That log can wait. | The auth fix and a debug statement ended up in the same hunk. 2 changes in the same hunk. |
| 4.5 to 8s | pgs | Local MCP server. Git staging for coding agents. Select a file, a hunk, or individual lines. |
| 8 to 15.5s | The agent previews the auth fix. | Commit the auth fix. Leave the debug line unstaged. Dry run · index unchanged. |
| 15.5 to 22.5s | The agent stages line 2. | Auth fix staged. The debug line is still unstaged. |
| 22.5 to 28s | The auth fix is committed. | Debug line: still unstaged in src/auth.rs. |
| 28 to 35s | Let your agent stage the fix. | pgs can stage a single line from a mixed diff. Install pgs. |

## Agent workspace

The request reads: "Commit the auth fix. Leave the debug line unstaged."
The activity list shows the actions in order:

- Read the diff
- Preview the auth fix
- Stage line 2
- Commit the auth fix

The inspector displays the actual tool name and arguments for each action:
`pgs_scan`, `pgs_stage` with `dry_run` and `explain`, `pgs_stage`, and
`pgs_commit`. The selected line is `src/auth.rs:2-2`. The scan hunk ID is
`d46b3c1640d6`. The preview status reads "Dry run · index unchanged."

The commit message is `fix: reject blank tokens`. The result shows one file
changed, one insertion, and one deletion. The debug statement remains in the
working tree. The closing screen names Claude Code, Codex, Gemini CLI,
Copilot, and Cursor, and shows `github.com/UtsavBalar1231/pgs` with
"Local MCP server + agent skill / MIT."

## Square cut

| Time | Headline | Supporting copy |
|------|----------|-----------------|
| 0 to 3s | That log can wait. | The auth fix and debug log share a hunk. |
| 3 to 9s | Stage the auth fix. | Preview exact lines. Auth fix staged; debug line still unstaged. |
| 9 to 12s | Let your agent stage the fix. | Install pgs. github.com/UtsavBalar1231/pgs |

The square cut shows the same MCP workspace and source changes. Code,
filenames, tool arguments, and Git notation are literal technical content.
