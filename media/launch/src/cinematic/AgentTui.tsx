import {
  ChevronDownIcon,
  FileCodeIcon,
  FileDirectoryIcon,
  FileIcon,
  GitBranchIcon,
  RepoIcon,
} from "@primer/octicons-react";
import { palette as p } from "../design";
import { Indicator } from "./Indicator";
import { type Phase, ToolPanel } from "./ToolPanel";
import { TuiChrome, TuiLine } from "./TuiChrome";

export const AgentTui = ({ phase = "scan" }: { readonly phase?: Phase }) => (
  <TuiChrome title="auth-service / agent review">
    <div
      style={{
        height: 48,
        display: "flex",
        alignItems: "center",
        gap: 38,
        borderBottom: `1px solid ${p.rule}`,
        padding: "0 25px",
        fontSize: 18,
        color: p.muted,
      }}
    >
      <span
        style={{
          color: p.purple,
          display: "flex",
          alignItems: "center",
          gap: 9,
        }}
      >
        <GitBranchIcon size={20} aria-hidden="true" />
        main
      </span>
      <span>1 modified file</span>
      <span style={{ color: p.green }}>+2</span>
      <span style={{ color: p.red }}>-1</span>
      <span style={{ marginLeft: "auto" }}>LOCAL REPOSITORY</span>
    </div>
    <div
      style={{
        display: "grid",
        gridTemplateColumns: "240px 900px 500px",
        height: 638,
      }}
    >
      <div
        style={{
          padding: "28px 20px",
          borderRight: `1px solid ${p.rule}`,
          fontSize: 19,
          color: p.muted,
          lineHeight: 2.5,
        }}
      >
        <div style={{ fontSize: 14, letterSpacing: 2, marginBottom: 20 }}>
          EXPLORER
        </div>
        {[
          {
            label: "auth-service",
            depth: 0,
            Icon: RepoIcon,
            expanded: true,
            modified: false,
          },
          {
            label: "src",
            depth: 1,
            Icon: FileDirectoryIcon,
            expanded: true,
            modified: false,
          },
          {
            label: "auth.rs",
            depth: 2,
            Icon: FileCodeIcon,
            expanded: false,
            modified: true,
          },
          {
            label: "Cargo.toml",
            depth: 1,
            Icon: FileIcon,
            expanded: false,
            modified: false,
          },
          {
            label: "README.md",
            depth: 1,
            Icon: FileIcon,
            expanded: false,
            modified: false,
          },
        ].map(({ label, depth, Icon, expanded, modified }) => (
          <div
            key={label}
            style={{
              margin: "0 -20px",
              paddingLeft: 14 + depth * 16,
              paddingRight: 13,
              display: "flex",
              alignItems: "center",
              gap: 8,
              fontSize: 18,
              color: modified ? p.paper : p.muted,
              background: modified ? p.selection : "transparent",
            }}
          >
            <span style={{ display: "inline-flex", width: 14, flexShrink: 0 }}>
              {expanded ? (
                <ChevronDownIcon size={14} aria-hidden="true" />
              ) : null}
            </span>
            <Icon size={18} aria-hidden="true" />
            <span>{label}</span>
            {modified ? (
              <span
                style={{ marginLeft: "auto", color: "#d29922", fontSize: 16 }}
              >
                M
              </span>
            ) : null}
          </div>
        ))}
        <div style={{ marginTop: 124, fontSize: 14, letterSpacing: 1 }}>
          PGS / MCP
        </div>
        <div
          style={{
            fontSize: 16,
            color: p.green,
            display: "flex",
            alignItems: "center",
            gap: 8,
          }}
        >
          <Indicator state="connected" size={16} />
          tools connected
        </div>
        <div style={{ fontSize: 16 }}>11 tools available</div>
      </div>
      <div style={{ borderRight: `1px solid ${p.rule}` }}>
        <div
          style={{
            padding: "20px 24px",
            borderBottom: `1px solid ${p.rule}`,
            fontSize: 19,
            display: "flex",
            justifyContent: "space-between",
          }}
        >
          <span>src/auth.rs</span>
          <span style={{ color: p.muted, fontSize: 15 }}>SCAN SNAPSHOT</span>
        </div>
        <div
          style={{
            padding: "16px 24px",
            color: p.accent,
            fontSize: 17,
            background: "#1f6feb0b",
          }}
        >
          @@ -1,7 +1,8 @@ <span style={{ color: p.muted }}>d46b3c1640d6</span>
        </div>
        <TuiLine number="1" text="fn is_valid(token: &str) -> bool {" />
        <TuiLine number="2" text="    token.len() > 0" kind="deletion" />
        <TuiLine
          number="2"
          text="    !token.trim().is_empty()"
          kind="addition"
          selected={phase !== "scan"}
        />
        <TuiLine number="3" text="}" />
        <TuiLine number="4" text="" />
        <TuiLine number="5" text="fn main() {" />
        <TuiLine
          number="6"
          text={'    eprintln!("debug: auth check");'}
          kind="addition"
        />
        <TuiLine number="7" text="    run();" />
        <TuiLine number="8" text="}" />
        <div style={{ padding: "25px 24px", color: p.muted, fontSize: 16 }}>
          1 hunk ·
          {phase === "scan"
            ? "auth fix + debug log"
            : "line 2 selected / debug excluded"}
        </div>
      </div>
      <ToolPanel phase={phase} />
    </div>
    <div
      style={{
        display: "flex",
        justifyContent: "space-between",
        padding: "14px 24px",
        background: p.panel,
        borderTop: `1px solid ${p.rule}`,
        color: p.muted,
        fontSize: 15,
      }}
    >
      <span>pgs / local MCP server</span>
      <span style={{ color: p.accent }}>
        {phase === "scan"
          ? "INSPECT"
          : phase === "preview"
            ? "DRY RUN · INDEX UNCHANGED"
            : "LINE 2 SELECTED"}
      </span>
      <span>UTF-8 · Rust · LF</span>
    </div>
  </TuiChrome>
);
