import { ArrowDownRightIcon, ArrowRightIcon } from "@primer/octicons-react";
import { interpolate, useCurrentFrame, useVideoConfig } from "remotion";
import { fonts, motion, palette as p } from "../design";
import { Indicator } from "./Indicator";
import { TuiChrome, TuiLine } from "./TuiChrome";

export const StagingDesk = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const progress = interpolate(frame, [1.45 * fps, 2.45 * fps], [0, 1], motion);
  const staged = progress === 1;
  return (
    <TuiChrome
      title="agent / precise staging"
      status={staged ? "SELECTION STAGED" : "PGS_STAGE"}
      state={staged ? "complete" : "running"}
    >
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "820px 820px",
          position: "relative",
          height: 535,
        }}
      >
        <div style={{ borderRight: `1px solid ${p.rule}`, paddingTop: 24 }}>
          <div style={{ padding: "0 29px 25px", color: p.muted, fontSize: 18 }}>
            WORKING TREE / src/auth.rs
          </div>
          <TuiLine number="1" text="fn is_valid(token: &str) -> bool {" />
          <TuiLine
            number="2"
            text="    !token.trim().is_empty()"
            selected={!staged}
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
          <div
            style={{
              padding: "18px 29px",
              fontSize: 17,
              color: "#d29922",
              display: "flex",
              gap: 10,
              alignItems: "center",
            }}
          >
            <ArrowDownRightIcon size={19} aria-hidden="true" />
            debug statement stays local
          </div>
        </div>
        <div style={{ paddingTop: 24 }}>
          <div style={{ padding: "0 29px 25px", color: p.muted, fontSize: 18 }}>
            GIT INDEX / {staged ? "selected changes" : "before staging"}
          </div>
          <TuiLine number="1" text="fn is_valid(token: &str) -> bool {" />
          <div style={{ opacity: progress > 0.7 && !staged ? 0 : 1 }}>
            <TuiLine
              number="2"
              text={
                staged ? "    !token.trim().is_empty()" : "    token.len() > 0"
              }
              kind={staged ? "addition" : "context"}
            />
          </div>
          <TuiLine number="3" text="}" />
          <div
            style={{
              margin: "60px 29px 0",
              border: `1px solid ${staged ? p.green : p.rule}`,
              borderRadius: 8,
              padding: 28,
              opacity: interpolate(
                frame,
                [2.45 * fps, 2.9 * fps],
                [0, 1],
                motion,
              ),
            }}
          >
            <div
              style={{
                color: p.green,
                fontSize: 27,
                display: "flex",
                gap: 12,
                alignItems: "center",
              }}
            >
              <Indicator state="complete" size={25} />
              One intention staged
            </div>
            <div style={{ color: p.muted, fontSize: 19, marginTop: 18 }}>
              1 replacement / debug excluded
            </div>
          </div>
        </div>
        {progress > 0 && !staged ? (
          <div
            style={{
              position: "absolute",
              left: progress * 820,
              top: 118,
              width: 815,
              height: 48,
              background: p.selection,
              borderLeft: `3px solid ${p.accent}`,
              color: "#aff5b4",
              fontSize: 23,
              display: "flex",
              alignItems: "center",
              whiteSpace: "pre",
              boxShadow: "0 12px 30px #01040988",
            }}
          >
            <span style={{ width: 55, textAlign: "right", color: p.muted }}>
              2
            </span>
            <span style={{ width: 46, textAlign: "center" }}>+</span>
            {"    !token.trim().is_empty()"}
          </div>
        ) : null}
      </div>
      <div
        style={{
          background: p.panel,
          padding: "22px 28px",
          display: "flex",
          alignItems: "center",
          gap: 30,
          borderTop: `1px solid ${p.rule}`,
        }}
      >
        <span
          style={{
            color: p.accent,
            fontSize: 25,
            display: "flex",
            alignItems: "center",
            gap: 12,
          }}
        >
          AGENT <ArrowRightIcon size={24} aria-hidden="true" />
          pgs_stage
        </span>
        <span style={{ fontFamily: fonts.code, color: p.muted, fontSize: 20 }}>
          selections: ["src/auth.rs:2-2"]
        </span>
        <span
          style={{
            marginLeft: "auto",
            color: staged ? p.green : p.accent,
            fontSize: 18,
            display: "flex",
            gap: 8,
            alignItems: "center",
          }}
        >
          <Indicator state={staged ? "complete" : "running"} size={18} />
          {staged ? "done" : "selecting…"}
        </span>
      </div>
    </TuiChrome>
  );
};
