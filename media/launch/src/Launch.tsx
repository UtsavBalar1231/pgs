import { Audio } from "@remotion/media";
import { AbsoluteFill, Series, staticFile, useVideoConfig } from "remotion";
import { End } from "./cinematic/End";
import { Inspect } from "./cinematic/Inspect";
import { Meet } from "./cinematic/Meet";
import { Opening } from "./cinematic/Opening";
import { Result } from "./cinematic/Result";
import { Select } from "./cinematic/Select";

export const Launch = () => {
  const { fps } = useVideoConfig();
  return (
    <AbsoluteFill>
      <Audio src={staticFile("score.m4a")} premountFor={fps} volume={1} />
      <Series>
        <Series.Sequence
          name="Big ideas, messy diff"
          durationInFrames={4.5 * fps}
          premountFor={fps}
        >
          <Opening />
        </Series.Sequence>
        <Series.Sequence
          name="Meet pgs"
          durationInFrames={3.5 * fps}
          premountFor={fps}
        >
          <Meet />
        </Series.Sequence>
        <Series.Sequence
          name="Agent inspects and previews"
          durationInFrames={7.5 * fps}
          premountFor={fps}
        >
          <Inspect />
        </Series.Sequence>
        <Series.Sequence
          name="Agent stages the fix"
          durationInFrames={7 * fps}
          premountFor={fps}
        >
          <Select />
        </Series.Sequence>
        <Series.Sequence
          name="Agent verifies the commit"
          durationInFrames={5.5 * fps}
          premountFor={fps}
        >
          <Result />
        </Series.Sequence>
        <Series.Sequence
          name="Add pgs to your agent"
          durationInFrames={7 * fps}
          premountFor={fps}
        >
          <End />
        </Series.Sequence>
      </Series>
    </AbsoluteFill>
  );
};
