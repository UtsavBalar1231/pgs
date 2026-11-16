import { agentCalls } from "../agent-tools";
import { palette as p } from "../design";
import { Indicator } from "./Indicator";

export type Phase = keyof typeof agentCalls;
const steps = { scan: 0, preview: 1, stage: 2, commit: 3 } as const;
const labels = {
  scan: "Read the diff",
  preview: "Preview the auth fix",
  stage: "Stage line 2",
  commit: "Commit the auth fix",
} as const;

export const ToolPanel = ({ phase }: { readonly phase: Phase }) => (
  <div
    style={{
      background: p.inset,
      height: "100%",
      padding: "24px 22px",
      boxSizing: "border-box",
    }}
  >
    <div style={{ fontSize: 15, letterSpacing: 2, color: p.muted }}>
      AGENT ACTIVITY
    </div>
    <div
      style={{
        borderLeft: `2px solid ${p.purple}`,
        padding: "13px 16px",
        background: "#ab7df80b",
        marginTop: 21,
        fontSize: 19,
        lineHeight: 1.65,
      }}
    >
      Commit the auth fix.
      <br />
      Leave the debug line unstaged.
    </div>
    <div style={{ marginTop: 26 }}>
      {(["scan", "preview", "stage", "commit"] as const).map((step) => (
        <div
          key={step}
          style={{
            padding: "12px 0",
            display: "flex",
            alignItems: "center",
            gap: 11,
            fontSize: 18,
            color: steps[step] <= steps[phase] ? p.paper : p.muted,
          }}
        >
          <Indicator
            state={
              steps[step] < steps[phase]
                ? "complete"
                : steps[step] === steps[phase]
                  ? "running"
                  : "pending"
            }
            size={18}
          />
          {labels[step]}
        </div>
      ))}
    </div>
    <div
      style={{
        marginTop: 24,
        border: `1px solid ${p.rule}`,
        borderRadius: 8,
        overflow: "hidden",
      }}
    >
      <div
        style={{
          background: p.panel,
          padding: "13px 16px",
          fontSize: 22,
          color: p.accent,
        }}
      >
        {agentCalls[phase].name}
      </div>
      <div
        style={{
          margin: 0,
          padding: 16,
          fontSize: 16,
          lineHeight: 1.8,
          color: p.muted,
          whiteSpace: "pre-wrap",
          overflowWrap: "anywhere",
        }}
      >
        {Object.entries(agentCalls[phase].arguments).map(([key, value]) => (
          <div key={key}>
            <span style={{ color: p.accent }}>{key}</span>:{" "}
            {JSON.stringify(value)}
          </div>
        ))}
      </div>
    </div>
  </div>
);
