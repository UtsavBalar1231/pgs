import type { ReactNode } from "react";
import { interpolate, useCurrentFrame, useVideoConfig } from "remotion";
import { motion } from "../design";

type Pose = {
  readonly at: number;
  readonly x: number;
  readonly y: number;
  readonly zoom: number;
  readonly tilt: number;
  readonly yaw: number;
  readonly roll: number;
};

export const Camera = ({
  children,
  poses,
}: {
  readonly children: ReactNode;
  readonly poses: readonly Pose[];
}) => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const time = frame / fps;
  const at = poses.map((pose) => pose.at);
  const x = interpolate(
    time,
    at,
    poses.map((pose) => pose.x),
    motion,
  );
  const y = interpolate(
    time,
    at,
    poses.map((pose) => pose.y),
    motion,
  );
  const zoom = interpolate(
    time,
    at,
    poses.map((pose) => pose.zoom),
    motion,
  );
  const tilt = interpolate(
    time,
    at,
    poses.map((pose) => pose.tilt),
    motion,
  );
  const yaw = interpolate(
    time,
    at,
    poses.map((pose) => pose.yaw),
    motion,
  );
  const roll = interpolate(
    time,
    at,
    poses.map((pose) => pose.roll),
    motion,
  );
  return (
    <div style={{ position: "absolute", inset: 0, perspective: 2400 }}>
      <div
        style={{
          position: "absolute",
          inset: 0,
          transformOrigin: "50% 50%",
          transform: `translate3d(${x}px,${y}px,0) scale(${zoom}) rotateX(${tilt}deg) rotateY(${yaw}deg) rotateZ(${roll}deg)`,
        }}
      >
        {children}
      </div>
    </div>
  );
};
