# Validation record

The current deliverable uses the original film's 35-second edit and electronic
score, GitHub dark colors, a detailed TUI-style agent workspace, and authored
camera moves. The agent performs the MCP workflow. Neither film shows manual
CLI typing or shell installation commands.

| Check | Result | Evidence |
|-------|--------|----------|
| Real MCP workflow | PASS | `pnpm verify-demo`: actual stdio scan, preview, stage, commit, rescan. Only the fix is committed; debug remains unstaged. |
| Palette | PASS | The named dark tokens match the fetched GitHub Primer CSS. URLs and values are in `CREATIVE.md`. |
| Original sound | PASS | AAC recovered directly from `out/pgs-launch-v1.mp4`, played at unity gain. Final mean -23.2 dB and peak -6.6 dB match the original mix. |
| Type and lint | PASS | `pnpm check`: Biome and strict `tsc --noEmit`. |
| Packaging | PASS | `cargo test --test test_codex_plugin`: 8 passed; Codex manifest mirrors match. |
| Main export | PASS | Web: H.264, 1280×720, 30 fps, 35 seconds, AAC audio. Local master: 1920×1080. |
| Social export | PASS | Web: H.264, 720×720, 30 fps, 12 seconds, AAC audio. Local master: 1080×1080. |
| Visual review | PASS | Final one-second contact sheets cover both films; full-size inspector, selected-line macro, staging transfer, result, ending, poster, and social card checked. |
| Reading views | PASS | Camera settles face-on at native scale; tool arguments fit inside their sidebar; the title clears the macro shot. |
| Motion | PASS | Wide oblique opening, frontal inspection, selected-line push-in, reverse staging view, tracked index, pullback, result reveal. |
| Determinism | PASS | Two independent renders of Inspect frame 180 are byte-identical. |
| Decode and sound | PASS | FFmpeg decodes the complete final file with no errors or clipping; no fully black intervals at a 0.01 pixel threshold. |
| Playback | PASS | Final MP4 plays in Chromium after a user click; intrinsic 1920px width and 35-second duration; no media error. |
| Docs and assets | PASS | Relative links and anchors resolve; film durations, poster, hero, mark, and plugin color match the current revision. |
| Whitespace | PASS | `git diff --check`. |

The workspace is an explanatory visualization, not a new pgs TUI or a recording
of an existing agent client. Macro views deliberately crop peripheral panes;
the selected code remains in frame. The square film has its own composition.

`out/evidence/` contains local render logs, MCP responses, Git proof, contact
sheets, frames, browser captures, and media metadata. It is ignored by Git.

## Source review

The camera owns pose interpolation. Each scene owns its shot. The workspace
chrome, diff lines, agent activity, and staging visualization have separate
responsibilities. Shared tokens own color and type. Every TypeScript file stays
below 250 nonblank lines, with strict indexing and optional-property checks.
The authored source uses no `any`, unchecked casts, non-null assertions, or type
suppression directives. Animation is driven by frames, with no random jitter.

The core Rust implementation is unchanged. Validation uses its existing MCP
server plus targeted packaging tests. The original MP4 is retained locally in
`out/`.

## UI asset review and final timing correction

The film now uses the official GitHub Octicons package for branch, repository,
file, operation-state, commit, and direction icons. Window controls are CSS
circles. Diff headers and deletion prefixes use ASCII minus signs. Active
operations use blue; successful operations turn green after completion.
`ASSETS.md` records the exact icons, package version, and license notice.

Fresh review captures cover the inspector, staging while running, staging when
complete, results, and both full contact sheets. The success-card fade begins
at 2.45 seconds of the staging scene, matching progress completion. Boundary
frames 538 and 539 were independently decoded and checked. Independent reviews
of the UI assets and the final timing correction found no blocking issues.

The staged-patch Lottie was separately rendered through the official Skottie
player at 512×512. The opening is transparent; the settled icon's alpha bounds
are `(108, 105)` to `(420, 430)`, within the canvas. Native frame exports were
used for the PNG, GIF, and MP4. `node --check scripts/staged-patch.mjs` and
`pnpm check` passed after packaging.

The publication check rebuilt `pgs-mcp` from the current source and reran the
MCP scenario against that executable. Its hunk ID is `d46b3c1640d6`; the film,
poster, and social exports were regenerated after updating that displayed ID.

## Final branding and copy

The staged-patch icon appears in every film logo position, the poster, and the
social card. The introduction plays its transparent Skottie frame export.
Two independent renders of Meet frame 32 are byte-identical, including the
moving patch. Both full exports decode without errors. Chromium plays the
landscape MP4 at 1920×1080 for 35.008 seconds without a media error.

The final contact sheets cover both cuts. Native-size opening and ending
frames show the revised copy fitting its layouts. `TRANSCRIPT.md` records
the on-screen wording; technical commands, selectors, and code retain their
literal values. The score, six-scene timings, and workspace cameras are
unchanged. `pnpm check`, Markdown link resolution, and `git diff --check` pass.

## Web delivery

`pnpm web` exports the launch film at 2,380,272 bytes (previously 5,471,050)
and the square cut at 523,918 bytes (previously 1,274,827). Both use H.264 Main
Level 3.1, yuv420p, 30 fps, a maximum two-second keyframe interval, and AAC
at 96 kb/s. The MP4 index precedes the media data. The existing masters remain
in `out/`; rendering and web export are separate commands.

Both complete exports decode without errors. The landscape export has SSIM
0.9983 against the master resized to the same dimensions. The native 720p
inspection frame was visually checked for readable code and UI labels.
GitHub's Markdown API renders the bare attachment URL as a native video
player with controls. Its signed media URL returns HTTP 206 for byte ranges.
The initial MP4s already had fast-start metadata; the repository-file README
link was the cause of opening the file viewer instead of an inline player.

`out/evidence/web-delivery/` records export logs, metadata, comparison frames,
Markdown rendering, and browser playback evidence. `pnpm check`, shell syntax,
and `git diff --check` pass. No animation, copy, logo, or score was changed.
