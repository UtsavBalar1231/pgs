import { loadFont } from "@remotion/fonts";
import { Easing, staticFile } from "remotion";

export const palette = {
  ink: "#0d1117",
  panel: "#151b23",
  inset: "#010409",
  rule: "#3d444d",
  muted: "#9198a1",
  paper: "#f0f6fc",
  accent: "#4493f8",
  blue: "#1f6feb",
  green: "#3fb950",
  red: "#f85149",
  purple: "#ab7df8",
  selection: "#1f6feb33",
} as const;
export const fonts = {
  display: "Instrument Sans",
  code: "JetBrains Mono",
} as const;
export const motion = {
  extrapolateLeft: "clamp",
  extrapolateRight: "clamp",
  easing: Easing.bezier(0.65, 0, 0.2, 1),
} as const;
void loadFont({
  family: fonts.display,
  url: staticFile("fonts/InstrumentSans.ttf"),
  weight: "400 700",
});
void loadFont({
  family: fonts.code,
  url: staticFile("fonts/JetBrainsMono.ttf"),
  weight: "400 700",
});
