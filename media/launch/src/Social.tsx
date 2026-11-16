import { ArrowRightIcon, ArrowUpRightIcon } from "@primer/octicons-react";
import { Audio } from "@remotion/media";
import {
  AbsoluteFill,
  interpolate,
  staticFile,
  useCurrentFrame,
  useVideoConfig,
} from "remotion";
import { AgentTui } from "./cinematic/AgentTui";
import { Indicator } from "./cinematic/Indicator";
import { fonts, motion, palette as p } from "./design";
import { Mark } from "./elements";

export const Social = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const ending = frame >= 9 * fps;
  const scanning = frame < 3 * fps;
  const staged = frame >= 6 * fps;
  return (
    <AbsoluteFill
      style={{
        background: p.ink,
        color: p.paper,
        fontFamily: fonts.display,
        overflow: "hidden",
      }}
    >
      <Audio
        src={staticFile("score.m4a")}
        premountFor={fps}
        volume={interpolate(frame, [11 * fps, 12 * fps], [1, 0], motion)}
      />
      <div
        style={{
          position: "absolute",
          left: 60,
          top: 47,
          display: "flex",
          alignItems: "center",
          gap: 14,
          fontSize: 34,
          fontWeight: 700,
        }}
      >
        <Mark size={44} />
        pgs
      </div>
      {ending ? (
        <>
          <div
            style={{
              position: "absolute",
              top: 218,
              left: 60,
              fontSize: 121,
              lineHeight: 1.02,
              letterSpacing: -7,
              fontWeight: 700,
            }}
          >
            Big ideas.
            <br />
            <span style={{ color: p.green }}>
              Small
              <br />
              commits.
            </span>
          </div>
          <div
            style={{
              position: "absolute",
              left: 60,
              right: 60,
              top: 777,
              padding: "27px 30px",
              background: p.blue,
              borderRadius: 10,
              display: "flex",
              alignItems: "center",
              gap: 15,
              fontSize: 35,
              fontWeight: 600,
            }}
          >
            Add pgs to your agent.{" "}
            <ArrowUpRightIcon size={33} aria-hidden="true" />
          </div>
          <div
            style={{
              position: "absolute",
              left: 60,
              top: 913,
              fontFamily: fonts.code,
              fontSize: 24,
            }}
          >
            github.com/UtsavBalar1231/pgs
          </div>
        </>
      ) : (
        <>
          <div
            style={{
              position: "absolute",
              top: 160,
              left: 60,
              fontSize: 109,
              lineHeight: 1.02,
              letterSpacing: -6,
              fontWeight: 700,
            }}
          >
            {scanning ? (
              <>
                Big ideas.
                <br />
                <span style={{ color: p.accent }}>Messy diff.</span>
              </>
            ) : (
              <>
                Your agent.
                <br />
                <span style={{ color: p.accent }}>Precise Git.</span>
              </>
            )}
          </div>
          <div
            style={{
              position: "absolute",
              left: 71,
              top: 453,
              width: 1640,
              perspective: 1800,
            }}
          >
            <div
              style={{
                transformOrigin: "left top",
                transform: `scale(0.565) rotateY(${interpolate(frame, [0, 3 * fps], [-8, 0], motion)}deg)`,
              }}
            >
              <AgentTui
                phase={scanning ? "scan" : staged ? "stage" : "preview"}
              />
            </div>
          </div>
          <div
            style={{
              position: "absolute",
              top: 961,
              left: 65,
              right: 65,
              fontFamily: fonts.code,
              fontSize: 25,
              color: staged ? p.green : p.muted,
              display: "flex",
              alignItems: "center",
              gap: 12,
            }}
          >
            {scanning ? (
              <>
                <span>Your intent</span>
                <ArrowRightIcon size={23} aria-hidden="true" />
                <span>your agent</span>
                <ArrowRightIcon size={23} aria-hidden="true" />
                <span>pgs</span>
              </>
            ) : staged ? (
              <>
                <Indicator state="complete" size={25} />
                <span>fix selected · debug stays local</span>
              </>
            ) : (
              <>
                <span>pgs_stage</span>
                <ArrowRightIcon size={23} aria-hidden="true" />
                <span>preview exact lines</span>
              </>
            )}
          </div>
        </>
      )}
    </AbsoluteFill>
  );
};
