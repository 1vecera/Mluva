import { AbsoluteFill, Audio, Sequence, staticFile, useCurrentFrame } from "remotion";
import { Background, Grain } from "../showreel/parts/Background";
import { BlurDefs } from "../showreel/parts/Blur";
import { easeInOut, ramp } from "../showreel/theme";
import { Hud } from "./parts/Hud";
import { chapterAt, CHAPTERS } from "./timing";
import { U01Kinetic } from "./chapters/U01Kinetic";
import { U02Before } from "./chapters/U02Before";
import { U03Model } from "./chapters/U03Model";
import { U04Wheel } from "./chapters/U04Wheel";
import { U05Deliver } from "./chapters/U05Deliver";
import { U06Montage } from "./chapters/U06Montage";
import { U07End } from "./chapters/U07End";
import cues from "./cues.json";

const VIEWS = [U01Kinetic, U02Before, U03Model, U04Wheel, U05Deliver, U06Montage, U07End];
// Where each chapter's ambient red glow sits (fractions of the frame).
const GLOW = [
  [0.5, 0.46],
  [0.3, 0.52],
  [0.5, 0.5],
  [0.5, 0.5],
  [0.5, 0.5],
  [0.5, 0.5],
  [0.5, 0.44],
];

// A slow camera push on the designed chapters. Real captures (04, 05), the montage's own cuts
// (06) and the lockup (07) keep a locked frame.
const CAMERA: Record<number, [number, number, number]> = { 1: [10, -4, 0.02], 2: [-8, -3, 0.015], 3: [-6, 4, 0.02] };
const Camera: React.FC<{ index: number; length: number; children: React.ReactNode }> = ({ index, length, children }) => {
  const frame = useCurrentFrame();
  const drift = CAMERA[index];
  if (!drift) return <AbsoluteFill>{children}</AbsoluteFill>;
  const p = Math.min(1, Math.max(0, frame / length));
  return (
    <AbsoluteFill style={{ transform: `translate(${drift[0] * (p - 0.5)}px, ${drift[1] * (p - 0.5)}px) scale(${1 + drift[2] * p})` }}>
      {children}
    </AbsoluteFill>
  );
};

// Studio preview only: renders are muted, and showreel/master-universal.sh mixes the same cue sheet.
const Soundtrack: React.FC = () => (
  <>
    <Audio src={staticFile(`universal/audio/${cues.score.file}`)} volume={10 ** (cues.score.gain_db / 20)} />
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

export const MluvaShowreelUniversal: React.FC<{ audio?: boolean }> = ({ audio = true }) => {
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
      <Hud quiet={ramp(frame, CHAPTERS[6].from + 2, CHAPTERS[6].from + 18, easeInOut)} />
      <Grain />
      {audio ? <Soundtrack /> : null}
    </AbsoluteFill>
  );
};
