import { AbsoluteFill } from "remotion";
import { fonts, palette as p } from "./design";
import { Mark } from "./elements";

export const Brand = () => (
  <AbsoluteFill
    style={{
      background: p.ink,
      color: p.paper,
      fontFamily: fonts.display,
      padding: "64px 72px",
    }}
  >
    <div
      style={{
        display: "flex",
        gap: 14,
        alignItems: "center",
        fontSize: 32,
        fontWeight: 700,
      }}
    >
      <Mark size={43} />
      pgs
    </div>
    <div
      style={{
        marginTop: 62,
        fontSize: 112,
        lineHeight: 1,
        fontWeight: 700,
        letterSpacing: -6,
      }}
    >
      Big ideas.
      <br />
      <span style={{ color: p.green }}>Small commits.</span>
    </div>
    <div style={{ position: "absolute", right: 75, top: 202 }}>
      <Mark size={240} />
    </div>
    <div
      style={{
        position: "absolute",
        bottom: 66,
        left: 72,
        right: 72,
        borderTop: `1px solid ${p.rule}`,
        paddingTop: 27,
        display: "flex",
        justifyContent: "space-between",
        alignItems: "center",
      }}
    >
      <span style={{ fontSize: 25 }}>Precise Git tools for coding agents.</span>
      <span style={{ fontFamily: fonts.code, fontSize: 17, color: p.muted }}>
        MCP + AGENT SKILL
      </span>
    </div>
  </AbsoluteFill>
);
