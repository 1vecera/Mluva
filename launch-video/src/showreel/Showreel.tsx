import { AbsoluteFill, Audio, Sequence, staticFile, useCurrentFrame } from "remotion";
import { Background, Grain } from "./parts/Background";
import { BlurDefs } from "./parts/Blur";
import { Hud } from "./parts/Hud";
import { beatFrame, chapterAt, CHAPTERS } from "./timing";
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

// A slow camera: 2.5 % push and a few pixels of drift across the bar. Chapters that show real UI
// (02), the brand lockup (05, 08) or run their own cuts (07) keep a locked frame.
const CAMERA: Record<number, [number, number]> = { 1: [10, -4], 3: [-12, 5], 4: [12, 4], 6: [-10, -5] };
const Camera: React.FC<{ index: number; length: number; children: React.ReactNode }> = ({ index, length, children }) => {
  const frame = useCurrentFrame();
  const drift = CAMERA[index];
  if (!drift) return <AbsoluteFill>{children}</AbsoluteFill>;
  const p = Math.min(1, Math.max(0, frame / length));
  return (
    <AbsoluteFill style={{ transform: `translate(${drift[0] * (p - 0.5)}px, ${drift[1] * (p - 0.5)}px) scale(${1 + 0.025 * p})` }}>
      {children}
    </AbsoluteFill>
  );
};

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
            <Camera index={c.index} length={c.to - c.from}>
              <View />
            </Camera>
          </Sequence>
        );
      })}
      <Hud onAccent={frame >= beatFrame(26) && frame < beatFrame(26.5)} quiet={ramp(frame, beatFrame(28) + 2, beatFrame(28) + 18, easeInOut)} />
      <Grain />
      {audio ? <Soundtrack /> : null}
    </AbsoluteFill>
  );
};
