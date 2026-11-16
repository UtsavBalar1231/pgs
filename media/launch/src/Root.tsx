import { Composition, Folder, Still } from "remotion";
import { Brand } from "./Brand";
import { End } from "./cinematic/End";
import { Inspect } from "./cinematic/Inspect";
import { Meet } from "./cinematic/Meet";
import { Opening } from "./cinematic/Opening";
import { Result } from "./cinematic/Result";
import { Select } from "./cinematic/Select";
import { Launch } from "./Launch";
import { Social } from "./Social";

export const RemotionRoot = () => (
  <>
    <Composition
      id="PgsLaunch"
      component={Launch}
      durationInFrames={1050}
      fps={30}
      width={1920}
      height={1080}
    />
    <Composition
      id="PgsSocial"
      component={Social}
      durationInFrames={360}
      fps={30}
      width={1080}
      height={1080}
    />
    <Still id="SocialCard" component={Brand} width={1280} height={640} />
    <Folder name="Scenes">
      <Composition
        id="Opening"
        component={Opening}
        durationInFrames={135}
        fps={30}
        width={1920}
        height={1080}
      />
      <Composition
        id="Meet"
        component={Meet}
        durationInFrames={105}
        fps={30}
        width={1920}
        height={1080}
      />
      <Composition
        id="Inspect"
        component={Inspect}
        durationInFrames={225}
        fps={30}
        width={1920}
        height={1080}
      />
      <Composition
        id="Select"
        component={Select}
        durationInFrames={210}
        fps={30}
        width={1920}
        height={1080}
      />
      <Composition
        id="Result"
        component={Result}
        durationInFrames={165}
        fps={30}
        width={1920}
        height={1080}
      />
      <Composition
        id="End"
        component={End}
        durationInFrames={210}
        fps={30}
        width={1920}
        height={1080}
      />
    </Folder>
  </>
);
