import {
  AbsoluteFill,
  interpolate,
  useCurrentFrame,
  useVideoConfig,
} from "remotion";
import { fonts, motion, palette as p } from "../design";
import { Mark, Reveal } from "../elements";

export const Meet = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  return (
    <AbsoluteFill
      style={{ background: p.blue, color: p.paper, fontFamily: fonts.display }}
    >
      <div
        style={{
          position: "absolute",
          left: 88,
          top: 61,
          fontFamily: fonts.code,
          fontSize: 20,
        }}
      >
        PRECISE GIT TOOLS FOR CODING AGENTS
      </div>
      <div
        style={{
          position: "absolute",
          left: 75,
          top: 204,
          fontSize: 295,
          fontWeight: 700,
          letterSpacing: -19,
          lineHeight: 1,
        }}
      >
        <Reveal>meet pgs.</Reveal>
      </div>
      <div
        style={{
          position: "absolute",
          right: 110,
          top: 243,
          scale: interpolate(frame, [0, 0.7 * fps], [0.3, 1], motion),
          rotate: `${interpolate(frame, [0, 0.7 * fps], [-15, 0], motion)}deg`,
        }}
      >
        <Mark size={280} color={p.paper} />
      </div>
      <div
        style={{
          position: "absolute",
          left: 92,
          top: 624,
          fontSize: 82,
          fontWeight: 600,
          letterSpacing: -4,
        }}
      >
        <Reveal delay={0.25}>Inspect. Select. Commit.</Reveal>
      </div>
      <div
        style={{ position: "absolute", left: 96, bottom: 136, fontSize: 31 }}
      >
        Your agent gets precise control. You get focused commits.
      </div>
      <div
        style={{
          position: "absolute",
          right: 90,
          bottom: 57,
          fontFamily: fonts.code,
          fontSize: 18,
        }}
      >
        FILES / HUNKS / LINES
      </div>
    </AbsoluteFill>
  );
};
