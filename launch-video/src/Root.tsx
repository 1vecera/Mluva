import { Composition, Folder } from "remotion";
import { MluvaLaunch } from "./MluvaLaunch";
import { TIMING } from "./timing";

const WIDTH = 1920;
const HEIGHT = 1080;

export const Root: React.FC = () => {
  return (
    <>
      <Composition
        id="MluvaLaunch"
        component={MluvaLaunch}
        durationInFrames={Math.round(TIMING.totalSeconds * 60)}
        fps={60}
        width={WIDTH}
        height={HEIGHT}
      />
      <Folder name="Preview">
        <Composition
          id="MluvaLaunchPreview"
          component={MluvaLaunch}
          durationInFrames={Math.round(TIMING.totalSeconds * 30)}
          fps={30}
          width={WIDTH}
          height={HEIGHT}
        />
      </Folder>
    </>
  );
};
