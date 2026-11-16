import { GitBranchIcon, GitCommitIcon } from "@primer/octicons-react";
import { fonts, palette as p } from "../design";
import { Stage } from "../elements";
import { Camera } from "./Camera";
import { Indicator } from "./Indicator";
import { TuiChrome } from "./TuiChrome";

export const Result = () => (
  <Stage label="03 / CHECK">
    <div
      style={{
        position: "absolute",
        top: 128,
        left: 88,
        fontSize: 94,
        fontWeight: 700,
        letterSpacing: -5,
      }}
    >
      The auth fix is <span style={{ color: p.green }}>committed.</span>
    </div>
    <Camera
      poses={[
        { at: 0, x: 20, y: 30, zoom: 0.9, tilt: 5, yaw: -8, roll: 1 },
        { at: 1, x: 0, y: 0, zoom: 1, tilt: 0, yaw: 0, roll: 0 },
        { at: 5.5, x: 0, y: 0, zoom: 1, tilt: 0, yaw: 0, roll: 0 },
      ]}
    >
      <div style={{ position: "absolute", left: 140, top: 330, width: 1640 }}>
        <TuiChrome
          title="agent / commit report"
          status="VERIFIED"
          state="complete"
        >
          <div
            style={{
              display: "grid",
              gridTemplateColumns: "820px 820px",
              height: 395,
            }}
          >
            <div style={{ padding: 35, borderRight: `1px solid ${p.rule}` }}>
              <div
                style={{
                  color: p.green,
                  fontSize: 21,
                  display: "flex",
                  alignItems: "center",
                  gap: 10,
                }}
              >
                <Indicator state="complete" size={22} />
                pgs_commit
              </div>
              <div style={{ fontSize: 34, marginTop: 33, lineHeight: 1.5 }}>
                fix: reject blank tokens
              </div>
              <div
                style={{
                  marginTop: 33,
                  padding: "20px 15px",
                  background: "#3fb95019",
                  color: "#aff5b4",
                  fontSize: 25,
                }}
              >
                + !token.trim().is_empty()
              </div>
              <div style={{ color: p.muted, fontSize: 18, marginTop: 24 }}>
                1 file changed · 1 insertion · 1 deletion
              </div>
            </div>
            <div style={{ padding: 35 }}>
              <div
                style={{
                  color: "#d29922",
                  fontSize: 21,
                  display: "flex",
                  gap: 10,
                  alignItems: "center",
                }}
              >
                <Indicator state="modified" size={22} />
                pgs_scan / still unstaged
              </div>
              <div style={{ fontSize: 34, marginTop: 33, lineHeight: 1.5 }}>
                Debug line
              </div>
              <div
                style={{
                  marginTop: 33,
                  padding: "20px 15px",
                  background: "#3fb9500d",
                  color: p.muted,
                  fontSize: 24,
                }}
              >
                + eprintln!("debug: auth check");
              </div>
              <div style={{ color: p.muted, fontSize: 18, marginTop: 24 }}>
                Still unstaged in src/auth.rs.
              </div>
            </div>
          </div>
        </TuiChrome>
      </div>
    </Camera>
    <div
      style={{
        position: "absolute",
        left: 158,
        bottom: 122,
        display: "flex",
        gap: 22,
        alignItems: "center",
        fontFamily: fonts.code,
        fontSize: 21,
        color: p.muted,
      }}
    >
      <GitBranchIcon size={25} fill={p.purple} aria-hidden="true" />
      <span style={{ color: p.purple }}>main</span>
      <span style={{ width: 110, height: 2, background: p.rule }} />
      <GitCommitIcon size={29} fill={p.green} aria-hidden="true" />
      <span>fix: reject blank tokens</span>
    </div>
  </Stage>
);
