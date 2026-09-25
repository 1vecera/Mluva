import { AbsoluteFill, Audio, Sequence, staticFile, useCurrentFrame } from "remotion";
import { Background, Grain } from "./parts/Background";
import { BlurDefs } from "./parts/Blur";
import { Hud } from "./parts/Hud";
import { chapterAt, CHAPTERS } from "./timing";
import { easeInOut, ramp } from "./theme";
import { C01Kinetic } from "./chapters/C01Kinetic";
import { C02Widget } from "./chapters/C02Widget";
import { C03Pipeline } from "./chapters/C03Pipeline";
import { C04Themes } from "./chapters/C04Themes";
import { C05Logo } from "./chapters/C05Logo";
import { C06Engines } from "./chapters/C06Engines";
import { C07Montage } from "./chapters/C07Montage";
import { C08EndCard } from "./chapters/C08EndCard";
import cues from "./cues.json";

const VIEWS = [C01Kinetic, C02Widget, C03Pipeline, C04Themes, C05Logo, C06Engines, C07Montage, C08EndCard];
// Where each chapter's ambient red glow sits (fractions of the frame).
const GLOW = [
  [0.5, 0.46],
  [0.5, 0.46],
  [0.5, 0.52],
  [0.64, 0.5],
  [0.5, 0.46],
  [0.69, 0.52],
  [0.5, 0.5],
  [0.5, 0.43],
];

// Studio preview only; renders are muted and showreel/mix.py masters the same cue sheet.
const Soundtrack: React.FC = () => (
  <>
    <Audio src={staticFile(`showreel/audio/${cues.score.file}`)} volume={10 ** (cues.score.gain_db / 20)} />
    {cues.cues.map((cue, i) => {
      const peak = cues.sfx_peaks_s[cue.sfx as keyof typeof cues.sfx_peaks_s];
      return (
        <Sequence key={i} from={Math.round(cue.frame - peak * cues.fps)} layout="none">
          <Audio src={staticFile(`showreel/audio/${cue.sfx}.mp3`)} volume={10 ** (cue.gain_db / 20)} />
        </Sequence>
      );
    })}
  </>
);

export const MluvaShowreel: React.FC<{ audio?: boolean }> = ({ audio = true }) => {
  const frame = useCurrentFrame();
  const chapter = chapterAt(frame);
  const i = chapter.index - 1;
  const blend = ramp(frame, chapter.from, chapter.from + 14, easeInOut);
  const prev = GLOW[Math.max(0, i - 1)];
  const glowX = prev[0] + (GLOW[i][0] - prev[0]) * blend;
  const glowY = prev[1] + (GLOW[i][1] - prev[1]) * blend;
  return (
    <AbsoluteFill style={{ backgroundColor: "#000" }}>
      <BlurDefs />
      <Background glowX={glowX} glowY={glowY} />
      {CHAPTERS.map((c, index) => {
        const View = VIEWS[index];
        return (
          <Sequence key={c.index} from={c.from} durationInFrames={c.to - c.from} layout="none">
            <AbsoluteFill>
              <View />
            </AbsoluteFill>
          </Sequence>
        );
      })}
      <Hud />
      <Grain />
      {audio ? <Soundtrack /> : null}
    </AbsoluteFill>
  );
};
