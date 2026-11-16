# Big ideas. Small commits.

A 35-second, dark product film for a tool used by coding agents. The six-part
edit follows the original launch film's pace and electronic score: a mixed diff,
pgs introduction, agent inspection, precise staging, verified result, and install
invitation. The user gives intent; the agent operates pgs through MCP throughout.
There is no manual shell typing or CLI installation command in the film.

## Color and typography

Colors come from [GitHub Primer's dark theme](https://primer.style/product/primitives/color/)
and its [published dark theme CSS](https://unpkg.com/@primer/primitives/dist/css/functional/themes/dark.css).
The authored tokens match the fetched CSS: default `#0d1117`, muted surface
`#151b23`, inset `#010409`, foreground `#f0f6fc`, muted foreground `#9198a1`,
accent `#4493f8`, success `#3fb950`, danger `#f85149`, purple `#ab7df8`, and
border `#3d444d`. The introduction and CTA use GitHub blue `#1f6feb`.
Instrument Sans supplies the large type. JetBrains Mono supplies the workspace;
code ligatures are disabled to preserve literal source and TUI alignment.

This is an independent project's presentation. It does not claim GitHub
endorsement, and the workspace is an illustration of agent/MCP behavior rather
than a new graphical interface shipped by pgs.

## Edit and camera

| Time | Scene | Camera and action |
|------|-------|-------------------|
| 0–4.5s | Big ideas. Messy diff. | Large type beside a drifting, angled agent workspace. |
| 4.5–8s | Meet pgs. | Blue brand cut, large type, selection-to-commit mark. |
| 8–15.5s | The agent inspects and previews | Oblique entry; settle face-on for reading; push into the exact selected line; pull back to the tool inspector. |
| 15.5–22.5s | Only the fix goes in | Reverse-angle entry; selected line crosses from working tree to index; camera tracks the staged result, then returns wide. |
| 22.5–28s | The agent checks the result | Pull out to committed and unstaged results side by side, plus a commit graph. |
| 28–35s | Big ideas. Small commits. | Large type, blue CTA, agent integrations, repository URL. |

Camera paths live on their scene components as explicit timed poses. The shared
camera interpolates translation, zoom, tilt, yaw, and roll. It settles into
frontal views when text needs reading. There is no random shake or continuous
spin. The main macro shot hides the scene title while the workspace fills the
frame. The visual action follows one real selected line through staging.

## Agent workspace

The TUI-style workspace has a repository/branch bar, file explorer, full diff,
content-addressed hunk ID, agent request, ordered activity, and a tool inspector
with the actual MCP argument shape. The staging view shows separate working-tree
and index panes; the result view shows what was committed and what remains local.
The source file does not lose its edit when its change enters the index.

The scenario is verified against the actual stdio MCP server. `pnpm verify-demo`
initializes a session and calls `pgs_scan`, `pgs_stage` with `dry_run` and
`explain`, `pgs_stage`, `pgs_commit`, and `pgs_scan` again. The real hunk ID is
`d46b3c1640d6`; line 2 is the fix and line 6 is the debug statement. Only the
replacement is committed. The final scan contains only the debug addition.
The film uses `/repo` as its illustrative repository path.

## Sound

The original 35-second electronic score is restored. The checked-in AAC track
was recovered directly from the original MP4, preserving the approved sound.
Its kick, ticks, bass, arpeggio, section accents, and final chord follow the
original edit's timing. `scripts/score.mjs` restores the original synthesis
recipe for reproducible future edits. There is no voiceover or licensed sample.

## Research retained in the design

[Ordinary Folk](https://www.ordinaryfolk.co/process) starts with message and
design before animation. [IBM's animation guidance](https://www.ibm.com/design/language/animation/tips-and-techniques/)
emphasizes focus, purposeful camera movement, consistent motion, and pauses.
Those principles inform the edit, while the visual and musical direction
follows the user's preference for the original film.

[Why Slop Matters](https://arxiv.org/abs/2601.06060) discusses superficial
competence hiding a lack of substance. The response here is a specific,
reproducible agent workflow and legible outcomes. These sources do not establish
a universal visual test for human authorship. The film is made with AI assistance.

## Rights

Original artwork, animation source, and synthesized music use the project's MIT
license. Instrument Sans and JetBrains Mono retain OFL licenses alongside their
font files. Remotion's own usage terms apply separately.
