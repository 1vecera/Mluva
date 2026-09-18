import { AbsoluteFill, Sequence, useVideoConfig } from "remotion";
import { Captions } from "./components/Captions";
import { SceneFade } from "./components/Camera";
import { WaterBackground } from "./components/WaterBackground";
import { S01Desktop } from "./scenes/S01Desktop";
import { S02Brand } from "./scenes/S02Brand";
import { S03F9 } from "./scenes/S03F9";
import { S04Providers } from "./scenes/S04Providers";
import { S05Tiling } from "./scenes/S05Tiling";
import { S06Polish } from "./scenes/S06Polish";
import { S07History } from "./scenes/S07History";
import { S08Themes } from "./scenes/S08Themes";
import { S09Live } from "./scenes/S09Live";
import { S10TaskSpec } from "./scenes/S10TaskSpec";
import { S11Outro } from "./scenes/S11Outro";
import { Soundtrack } from "./Soundtrack";
import { FONT, NORD } from "./theme";
import { TIMING, type SceneTiming } from "./timing";

const SCENES: Record<string, React.FC<{ scene: SceneTiming }>> = {
  S01: S01Desktop,
  S02: S02Brand,
  S03: S03F9,
  S04: S04Providers,
  S05: S05Tiling,
  S06: S06Polish,
  S07: S07History,
  S08: S08Themes,
  S09: S09Live,
  S10: S10TaskSpec,
  S11: S11Outro,
};

/** The one-minute Mluva film. Scenes are placed by the transcript-driven cue sheet in src/generated/timing.json. */
export const MluvaLaunch: React.FC = () => {
  const { fps } = useVideoConfig();
  const f = (s: number) => Math.round(s * fps);
  return (
    <AbsoluteFill style={{ backgroundColor: NORD.deep, fontFamily: FONT, color: NORD.fg }}>
      <WaterBackground />
      {TIMING.scenes.map((scene, i) => {
        const Scene = SCENES[scene.id];
        return (
          <Sequence key={scene.id} name={scene.id} from={f(scene.sceneStart)} durationInFrames={Math.max(1, f(scene.sceneEnd) - f(scene.sceneStart))} premountFor={fps}>
            <SceneFade dir={i % 2 === 0 ? 1 : -1}>
              <Scene scene={scene} />
            </SceneFade>
          </Sequence>
        );
      })}
      <Captions timing={TIMING} />
      <Soundtrack />
    </AbsoluteFill>
  );
};
