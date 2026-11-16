import {
  AbsoluteFill,
  AnimatedImage,
  staticFile,
  useVideoConfig,
} from "remotion";
import { fonts, palette as p } from "../design";
import { Reveal } from "../elements";

export const Meet = () => {
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
        LOCAL MCP SERVER
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
        <Reveal>pgs</Reveal>
      </div>
      <div
        style={{
          position: "absolute",
          right: 110,
          top: 160,
        }}
      >
        <AnimatedImage
          src={staticFile("pgs-icon-reveal.webp")}
          width={460}
          height={460}
          style={{ width: 460, height: 460 }}
          premountFor={fps}
          loopBehavior="pause-after-finish"
        />
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
        <Reveal delay={0.25}>Git staging for coding agents.</Reveal>
      </div>
      <div
        style={{ position: "absolute", left: 96, bottom: 136, fontSize: 31 }}
      >
        Select a file, a hunk, or individual lines.
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
        pgs-mcp
      </div>
    </AbsoluteFill>
  );
};
