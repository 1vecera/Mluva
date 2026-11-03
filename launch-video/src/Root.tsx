import { CalculateMetadataFunction, Composition, staticFile } from "remotion";
import { EditPlan, IntroProps, MluvaIntro } from "./MluvaIntro";
import { FilmPlan, FilmProps, MluvaFilm } from "./film/MluvaFilm";
import { MluvaShowreel } from "./showreel/Showreel";
import { HIGHLIGHT_DURATION, MluvaRustHighlight } from "./highlight/Highlight";
import { LockupCheck, LockupCheckMode } from "./showreel/LockupCheck";
import { DURATION, FPS, HEIGHT, WIDTH } from "./showreel/timing";
import { MluvaShowreelUniversal } from "./universal/Universal";
import { DURATION as UNIVERSAL_DURATION } from "./universal/timing";
import { UniversalLockupCheck, UniversalLockupCheckMode } from "./universal/LockupCheck";

const metadata: CalculateMetadataFunction<IntroProps> = async () => {
  const response = await fetch(staticFile("live/edit.json"));
  if (!response.ok) throw new Error("Restore public/live/edit.json from the local media archive.");
  const edit = (await response.json()) as EditPlan;
  if (!Number.isFinite(edit.duration) || edit.duration <= 0 || !edit.poppy_captions?.length) {
    throw new Error("The local narration edit plan is incomplete.");
  }
  return { durationInFrames: Math.round(edit.duration * 60), props: { edit } };
};

const filmMetadata: CalculateMetadataFunction<FilmProps> = async () => {
  const response = await fetch(staticFile("film/plan.json"));
  if (!response.ok) throw new Error("Restore public/film/plan.json from the local media.");
  const plan = (await response.json()) as FilmPlan;
  return { durationInFrames: Math.round(plan.duration * 60), props: { plan } };
};

export const Root: React.FC = () => (
  <>
    <Composition
      id="MluvaFilm"
      component={MluvaFilm}
      calculateMetadata={filmMetadata}
      defaultProps={{ plan: null }}
      durationInFrames={60}
      fps={60}
      width={1920}
      height={1080}
    />
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
      id="MluvaShowreelUniversal"
      component={MluvaShowreelUniversal}
      durationInFrames={UNIVERSAL_DURATION}
      fps={FPS}
      width={WIDTH}
      height={HEIGHT}
    />
    <Composition
      id="UniversalLockupCheck"
      component={UniversalLockupCheck}
      defaultProps={{ mode: "svg" as UniversalLockupCheckMode }}
      durationInFrames={1}
      fps={FPS}
      width={WIDTH}
      height={HEIGHT}
    />
    <Composition
      id="MluvaRustHighlight"
      component={MluvaRustHighlight}
      durationInFrames={HIGHLIGHT_DURATION}
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
