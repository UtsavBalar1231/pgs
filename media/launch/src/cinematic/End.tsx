import { ArrowUpRightIcon } from "@primer/octicons-react";
import { fonts, palette as p } from "../design";
import { Mark, Reveal, Stage } from "../elements";

export const End = () => (
  <Stage label="GIT TOOLS FOR YOUR CODING AGENT">
    <div
      style={{
        position: "absolute",
        left: 82,
        top: 160,
        fontSize: 155,
        fontWeight: 700,
        letterSpacing: -9,
        lineHeight: 1.05,
      }}
    >
      <Reveal>Big ideas.</Reveal>
      <Reveal delay={0.15}>
        <span style={{ color: p.green }}>Small commits.</span>
      </Reveal>
    </div>
    <div style={{ position: "absolute", right: 130, top: 235 }}>
      <Mark size={285} />
    </div>
    <div
      style={{
        position: "absolute",
        left: 94,
        top: 562,
        fontSize: 31,
        color: p.muted,
      }}
    >
      Give your agent precise Git tools. Keep every commit focused.
    </div>
    <div style={{ position: "absolute", top: 711, left: 88, right: 88 }}>
      <div
        style={{
          padding: "28px 34px",
          background: p.blue,
          border: `1px solid ${p.accent}`,
          borderRadius: 10,
          fontSize: 42,
          fontWeight: 600,
          display: "flex",
          alignItems: "center",
          gap: 18,
        }}
      >
        Add pgs to your coding agent.{" "}
        <ArrowUpRightIcon size={40} aria-hidden="true" />
      </div>
      <div
        style={{
          marginTop: 26,
          fontFamily: fonts.code,
          fontSize: 23,
          color: p.muted,
        }}
      >
        Claude Code · Codex · Gemini CLI · Copilot · Cursor
      </div>
      <div
        style={{
          marginTop: 40,
          display: "flex",
          justifyContent: "space-between",
          fontSize: 27,
        }}
      >
        <span>github.com/UtsavBalar1231/pgs</span>
        <span style={{ color: p.muted }}>
          Local MCP server + agent skill / MIT
        </span>
      </div>
    </div>
  </Stage>
);
