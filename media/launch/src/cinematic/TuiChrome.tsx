import type { ReactNode } from "react";
import { fonts, palette as p } from "../design";
import { Indicator } from "./Indicator";

export const TuiChrome = ({
  children,
  title = "agent workspace",
  status = "MCP CONNECTED",
  state = "connected",
}: {
  readonly children: ReactNode;
  readonly title?: string;
  readonly status?: string;
  readonly state?: "connected" | "running" | "complete";
}) => (
  <div
    style={{
      background: p.ink,
      border: `1px solid ${p.rule}`,
      borderRadius: 16,
      overflow: "hidden",
      boxShadow: "0 36px 100px #010409cc, 0 0 0 7px #ffffff03",
      fontFamily: fonts.code,
      fontVariantLigatures: "none",
    }}
  >
    <div
      style={{
        height: 58,
        background: p.panel,
        borderBottom: `1px solid ${p.rule}`,
        display: "flex",
        justifyContent: "space-between",
        alignItems: "center",
        padding: "0 25px",
        fontSize: 20,
      }}
    >
      <span style={{ display: "flex", gap: 9, alignItems: "center" }}>
        {[p.red, "#d29922", p.green].map((color) => (
          <span
            key={color}
            aria-hidden="true"
            style={{
              width: 12,
              height: 12,
              borderRadius: "50%",
              background: color,
            }}
          />
        ))}
        <span style={{ marginLeft: 23, color: p.muted }}>{title}</span>
      </span>
      <span
        style={{
          display: "flex",
          gap: 9,
          alignItems: "center",
          fontSize: 16,
          color: state === "running" ? p.accent : p.green,
        }}
      >
        <Indicator state={state} size={16} />
        {status}
      </span>
    </div>
    {children}
  </div>
);

export const TuiLine = ({
  number,
  text,
  kind = "context",
  selected = false,
}: {
  readonly number: string;
  readonly text: string;
  readonly kind?: "context" | "addition" | "deletion";
  readonly selected?: boolean;
}) => {
  const colors = {
    context: p.paper,
    addition: "#aff5b4",
    deletion: "#ffdcd7",
  } as const;
  const backgrounds = {
    context: "transparent",
    addition: "#3fb95019",
    deletion: "#f8514919",
  } as const;
  const symbols = { context: " ", addition: "+", deletion: "-" } as const;
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        whiteSpace: "pre",
        height: 48,
        background: selected ? p.selection : backgrounds[kind],
        color: colors[kind],
        borderLeft: `3px solid ${selected ? p.accent : "transparent"}`,
        fontSize: 23,
      }}
    >
      <span style={{ width: 55, textAlign: "right", color: p.muted }}>
        {number}
      </span>
      <span style={{ width: 46, textAlign: "center" }}>{symbols[kind]}</span>
      <span>{text}</span>
      {selected ? (
        <span
          style={{
            marginLeft: "auto",
            paddingRight: 17,
            fontSize: 14,
            color: p.accent,
          }}
        >
          SELECTED
        </span>
      ) : null}
    </div>
  );
};
