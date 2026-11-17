# pgs launch film

A dark product film showing a coding agent
using pgs MCP tools to commit a fix and leave a debug statement local.

https://github.com/user-attachments/assets/b5857677-9546-4bdc-925d-8ab0b2f0b629

[12-second square cut](https://github.com/user-attachments/assets/ee51b137-6307-4a8f-aeb8-b6f4cadf6cae) ·
[Creative direction and research](CREATIVE.md)

[On-screen transcript](TRANSCRIPT.md)

![The agent previews the selected fix in its workspace.](../../docs/assets/launch-poster.png)

The web exports are 1280×720 (35 seconds, 2.38 MB) and 720×720
(12 seconds, 0.52 MB), at 30 fps with H.264/AAC and fast-start metadata.
The editable masters remain 1920×1080 and 1080×1080.
Both have original synthesized sound and music, with no
voiceover. All essential information is visible with sound off.

## Edit and preview

Requires Node.js 20.19+, 22.13+, or 24+ and pnpm. FFmpeg is needed for web
exports and to regenerate the audio. The checked-in audio is ready to use.

```bash
cd media/launch
pnpm install --frozen-lockfile
pnpm dev
```

Open <http://localhost:3210/PgsLaunch>. `PgsSocial`, `SocialCard`, and the six
individual scene compositions are available in the sidebar. Frame-based
animation makes scrubbing deterministic. Press Space to play or pause.

## Export

```bash
pnpm check
pnpm verify-demo    # requires a built ../../target/debug/pgs-mcp
pnpm render        # out/pgs-launch.mp4
pnpm social-video  # out/pgs-social.mp4
pnpm web           # optimized MP4s in ../../docs/assets/ (requires both masters)
pnpm social        # ../../docs/assets/social-card.png
pnpm poster        # ../../docs/assets/launch-poster.png
pnpm audio         # regenerate public/score.m4a from scripts/score.mjs
```

Run `pnpm audio` before rendering if you change the sound source. The optional
first render may download Remotion's headless Chromium browser. `pnpm web`
scales the masters to 720p, caps video bitrate, adds two-second keyframes,
encodes audio at 96 kb/s, and places the MP4 index before the media data.
Masters and intermediate files stay in `out/`.
If Cargo uses a custom target directory, set `PGS_MCP_BINARY` to the freshly
built server's absolute path when running `pnpm verify-demo`.

The README player uses a GitHub video attachment. After replacing an export,
upload it through GitHub's Markdown editor and paste its attachment URL on a
line by itself. A repository-file link opens the file viewer instead of an
inline player. Keep the optimized files in `docs/assets/` for direct reuse.

`src/design.ts` owns the palette, fonts, and motion curve. Each scene owns its
layout and timing. `src/Launch.tsx` owns the edit; `src/Social.tsx` is composed
specifically for a square frame. It is not a crop of the landscape film.

[Presentation kit](../../docs/PRESENTATION.md) · [Validation](QA.md)

## Animated icon

The [staged-patch icon kit](../../docs/assets/pgs-icon/README.md) includes an
SVG, transparent PNG, self-contained Lottie JSON, and playback previews.
`scripts/staged-patch.mjs` authors the geometry and motion. It writes the
editable scene to `out/skottie-player/public/projects/pgs/scene-7/lottie.json`
and an export copy under `out/staged-patch/`. The official Skottie player is
installed separately under `out/skottie-player/` for previewing and color edits.
The film plays `public/pgs-icon-reveal.webp`, exported from Skottie, in its
introduction. Its other placements use `public/pgs-mark.svg`. Re-export these
assets after editing the icon's geometry, palette, or animation.
