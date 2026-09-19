import { Composition } from "remotion";
import { MluvaIntro } from "./MluvaIntro";
import { NativeReconstruction, APP_WIDTH, APP_HEIGHT } from "./Surfaces";
import data from "./generated/intro.json";

export const Root: React.FC = () => (
  <>
    <Composition
      id="MluvaIntro"
      component={MluvaIntro}
      durationInFrames={Math.round(data.duration * 60)}
      fps={60}
      width={1920}
      height={1080}
    />
    <Composition
      id="MluvaIntroPreview"
      component={MluvaIntro}
      durationInFrames={Math.round(data.duration * 30)}
      fps={30}
      width={1920}
      height={1080}
    />
    <Composition
      id="NativeReconstruction"
      component={NativeReconstruction}
      durationInFrames={1}
      fps={30}
      width={APP_WIDTH * 2}
      height={APP_HEIGHT * 2}
      defaultProps={{ surface: "polished" as const }}
    />
  </>
);
