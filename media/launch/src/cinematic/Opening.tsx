import { fonts, palette as p } from "../design";
import { Reveal, Stage } from "../elements";
import { AgentTui } from "./AgentTui";
import { Camera } from "./Camera";

export const Opening = () => (
  <Stage label="YOUR AGENT HAS BEEN BUSY.">
    <Camera
      poses={[
        { at: 0, x: 40, y: 70, zoom: 0.87, tilt: 11, yaw: -24, roll: 7 },
        { at: 4.5, x: -50, y: 0, zoom: 0.95, tilt: 7, yaw: -15, roll: 3 },
      ]}
    >
      <div style={{ position: "absolute", left: 1080, top: 166, width: 1640 }}>
        <AgentTui />
      </div>
    </Camera>
    <div
      style={{
        position: "absolute",
        left: 80,
        top: 230,
        fontSize: 144,
        lineHeight: 1.02,
        fontWeight: 700,
        letterSpacing: -8,
      }}
    >
      <div>Big ideas.</div>
      <Reveal delay={0.45}>
        <span style={{ color: p.accent }}>Messy diff.</span>
      </Reveal>
    </div>
    <div
      style={{
        position: "absolute",
        left: 88,
        top: 628,
        color: p.muted,
        fontSize: 33,
        lineHeight: 1.6,
      }}
    >
      Your agent fixed the bug.
      <br />
      The debug line came along.
    </div>
    <div
      style={{
        position: "absolute",
        left: 88,
        bottom: 96,
        fontFamily: fonts.code,
        fontSize: 20,
        color: p.muted,
      }}
    >
      1 FILE / 1 HUNK / 2 INTENTIONS
    </div>
  </Stage>
);
