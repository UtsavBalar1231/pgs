import type { CSSProperties, ReactNode } from "react";
import {
  AbsoluteFill,
  interpolate,
  useCurrentFrame,
  useVideoConfig,
} from "remotion";
import { fonts, motion, palette as p } from "./design";

export const Mark = ({
  size = 56,
  color = p.accent,
}: {
  readonly size?: number;
  readonly color?: string;
}) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 80 80"
    fill="none"
    aria-label="pgs selection mark"
  >
    <title>Selected changes become a focused commit</title>
    <path
      d="M8 18H30L52 40H66M8 40H66M8 62H30L52 40"
      stroke={color}
      strokeWidth="7"
    />
    <rect x="59" y="32" width="16" height="16" rx="3" fill={color} />
  </svg>
);

export const Stage = ({
  children,
  label,
}: {
  readonly children: ReactNode;
  readonly label: string;
}) => (
  <AbsoluteFill
    style={{
      background: p.ink,
      color: p.paper,
      fontFamily: fonts.display,
      overflow: "hidden",
    }}
  >
    <AbsoluteFill
      style={{
        backgroundImage: "radial-gradient(#30363d 1px, transparent 1px)",
        backgroundSize: "48px 48px",
        opacity: 0.2,
      }}
    />
    <div
      style={{
        position: "absolute",
        top: 40,
        left: 80,
        right: 80,
        display: "flex",
        justifyContent: "space-between",
        alignItems: "center",
        fontFamily: fonts.code,
        fontSize: 19,
        color: p.muted,
      }}
    >
      <span
        style={{
          display: "flex",
          gap: 12,
          alignItems: "center",
          color: p.paper,
          fontSize: 26,
          fontWeight: 600,
        }}
      >
        <Mark size={36} />
        pgs
      </span>
      <span>{label}</span>
    </div>
    {children}
  </AbsoluteFill>
);

export const Reveal = ({
  children,
  delay = 0,
  style,
}: {
  readonly children: ReactNode;
  readonly delay?: number;
  readonly style?: CSSProperties;
}) => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  return (
    <div style={{ overflow: "hidden", paddingBottom: "0.08em", ...style }}>
      <div
        style={{
          translate: `0 ${interpolate(frame - delay * fps, [0, fps * 0.55], [110, 0], motion)}%`,
        }}
      >
        {children}
      </div>
    </div>
  );
};
