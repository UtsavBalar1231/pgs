# Asset inventory

## GitHub UI icons

The film imports real SVG components from GitHub's official
[`@primer/octicons-react`](https://github.com/primer/octicons) package, pinned to
**19.42.0** in `package.json` and `pnpm-lock.yaml`. The icons are bundled locally;
rendering does not depend on a CDN. They retain GitHub's path geometry and use
explicit sizes and inherited theme colors.

| Element | Official Octicon |
|---------|------------------|
| Branch label | `GitBranchIcon` |
| Commit graph | `GitCommitIcon` |
| Repository and folder | `RepoIcon`, `FileDirectoryIcon` |
| Code and document files | `FileCodeIcon`, `FileIcon` |
| Expanded tree nodes | `ChevronDownIcon` |
| Pending step | `CircleIcon` |
| Active operation | `DotFillIcon` |
| Completed operation | `CheckCircleIcon` |
| Remaining modification | `DiffModifiedIcon` |
| Connected MCP server | `PlugIcon` |
| Flow and call direction | `ArrowRightIcon`, `ArrowDownRightIcon` |
| Install CTA | `ArrowUpRightIcon` |

The package's MIT notice is copied to
[`public/licenses/Octicons-MIT.txt`](public/licenses/Octicons-MIT.txt).
[Official usage documentation](https://primer.style/octicons/code/) describes
these assets and their sizing. No GitHub, agent, or MCP brand logo is fabricated.
Agent integrations are named in text.

## Other artwork and text

The film uses the approved staged-patch icon throughout. `public/pgs-mark.svg`
and `docs/assets/mark.svg` contain its original geometry with a tighter viewBox
for small placements. The full SVG and editable Lottie live in
`docs/assets/pgs-icon/`. `public/pgs-icon-reveal.webp` is a lossless transparent
animation made from native Skottie frame exports; the introduction plays it
once and holds the final frame. See the icon kit's README for motion references.
The pgs icon is not an Octicon. Window controls and rules are simple CSS
geometry; they do not depend on font glyphs. Source diff prefixes and headers
use literal ASCII `+`, `-`, and `@@`. The file-status `M` remains normal Git
notation. Middle dots and the ellipsis are typography, not substitute icons.

Instrument Sans and JetBrains Mono are local OFL font assets, with their license
files in `public/fonts/`. The music is the original synthesized score from v1.
No generated raster assets, stock photos, or sampled music are needed.
