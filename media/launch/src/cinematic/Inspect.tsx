import { interpolate, useCurrentFrame, useVideoConfig } from "remotion";
import { motion, palette as p } from "../design";
import { Stage } from "../elements";
import { AgentTui } from "./AgentTui";
import { Camera } from "./Camera";

export const Inspect = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  return (
    <Stage label="01 / THE AGENT INSPECTS & PREVIEWS">
      <div
        style={{
          position: "absolute",
          left: 88,
          top: 123,
          fontSize: 66,
          fontWeight: 600,
          letterSpacing: -3,
          opacity: interpolate(
            frame / fps,
            [0, 2.7, 3.15, 5.25, 5.7, 7.5],
            [1, 1, 0, 0, 1, 1],
            motion,
          ),
        }}
      >
        The whole diff.{" "}
        <span style={{ color: p.accent }}>One precise selection.</span>
      </div>
      <Camera
        poses={[
          { at: 0, x: -30, y: 0, zoom: 0.87, tilt: 8, yaw: -11, roll: 2 },
          { at: 1.1, x: 0, y: 0, zoom: 1, tilt: 0, yaw: 0, roll: 0 },
          { at: 2.6, x: 0, y: 0, zoom: 1, tilt: 0, yaw: 0, roll: 0 },
          { at: 3.35, x: 200, y: -40, zoom: 1.33, tilt: 0, yaw: -2, roll: 0 },
          { at: 4.7, x: 200, y: -40, zoom: 1.33, tilt: 0, yaw: -2, roll: 0 },
          { at: 5.7, x: 0, y: 0, zoom: 1, tilt: 0, yaw: 0, roll: 0 },
          { at: 7.5, x: 0, y: 0, zoom: 1, tilt: 0, yaw: 0, roll: 0 },
        ]}
      >
        <div style={{ position: "absolute", left: 140, top: 237, width: 1640 }}>
          <AgentTui phase={frame >= 2.4 * fps ? "preview" : "scan"} />
        </div>
      </Camera>
    </Stage>
  );
};
