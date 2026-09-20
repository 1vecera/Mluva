import { Composition } from "remotion";
import { MluvaIntro } from "./MluvaIntro";
export const Root: React.FC = () => (
  <Composition
    id="MluvaIntro"
    component={MluvaIntro}
    durationInFrames={59 * 60}
    fps={60}
    width={1920}
    height={1080}
  />
);
