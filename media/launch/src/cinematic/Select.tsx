import { palette as p } from "../design";
import { Stage } from "../elements";
import { Camera } from "./Camera";
import { StagingDesk } from "./StagingDesk";

export const Select = () => (
  <Stage label="02 / THE AGENT STAGES THE FIX">
    <div
      style={{
        position: "absolute",
        left: 88,
        top: 132,
        fontSize: 87,
        fontWeight: 700,
        letterSpacing: -4,
      }}
    >
      Only the fix <span style={{ color: p.green }}>goes in.</span>
    </div>
    <Camera
      poses={[
        { at: 0, x: 0, y: 36, zoom: 0.91, tilt: 6, yaw: 12, roll: -2 },
        { at: 1.2, x: 0, y: 24, zoom: 1, tilt: 0, yaw: 0, roll: 0 },
        { at: 3.0, x: 0, y: 24, zoom: 1, tilt: 0, yaw: 0, roll: 0 },
        { at: 3.8, x: -185, y: 10, zoom: 1.15, tilt: 0, yaw: 2, roll: 0 },
        { at: 5.2, x: -185, y: 10, zoom: 1.15, tilt: 0, yaw: 2, roll: 0 },
        { at: 6.1, x: 0, y: 24, zoom: 1, tilt: 0, yaw: 0, roll: 0 },
        { at: 7, x: 0, y: 24, zoom: 1, tilt: 0, yaw: 0, roll: 0 },
      ]}
    >
      <div style={{ position: "absolute", left: 140, top: 283, width: 1640 }}>
        <StagingDesk />
      </div>
    </Camera>
  </Stage>
);
