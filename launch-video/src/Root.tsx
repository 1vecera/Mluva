import { CalculateMetadataFunction, Composition, staticFile } from "remotion";
import { EditPlan, IntroProps, MluvaIntro } from "./MluvaIntro";
import { MluvaShowreel } from "./showreel/Showreel";
import { LockupCheck, LockupCheckMode } from "./showreel/LockupCheck";
import { DURATION, FPS, HEIGHT, WIDTH } from "./showreel/timing";

const metadata: CalculateMetadataFunction<IntroProps> = async () => {
  const response = await fetch(staticFile("live/edit.json"));
  if (!response.ok) throw new Error("Restore public/live/edit.json from the local media archive.");
  const edit = (await response.json()) as EditPlan;
  if (!Number.isFinite(edit.duration) || edit.duration <= 0 || !edit.poppy_captions?.length) {
    throw new Error("The local narration edit plan is incomplete.");
  }
  return { durationInFrames: Math.round(edit.duration * 60), props: { edit } };
};

export const Root: React.FC = () => (
  <>
    <Composition
      id="MluvaIntro"
      component={MluvaIntro}
      calculateMetadata={metadata}
      defaultProps={{ edit: null }}
      durationInFrames={60}
      fps={60}
      width={1920}
      height={1080}
    />
    <Composition
      id="MluvaShowreel"
      component={MluvaShowreel}
      durationInFrames={DURATION}
      fps={FPS}
      width={WIDTH}
      height={HEIGHT}
    />
    <Composition
      id="ShowreelLockupCheck"
      component={LockupCheck}
      defaultProps={{ mode: "logo-svg" as LockupCheckMode }}
      durationInFrames={1}
      fps={FPS}
      width={WIDTH}
      height={HEIGHT}
    />
  </>
);
