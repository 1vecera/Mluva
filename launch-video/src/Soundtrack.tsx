import { Audio } from "@remotion/media";
import { Sequence, interpolate, staticFile, useVideoConfig } from "remotion";
import { TIMING } from "./timing";

const MUSIC = "audio/music-a.mp3";

/**
 * Narration is one Fish Audio take cut into eleven segments by the transcript cue sheet;
 * each segment is placed where its scene wants it. Music ducks under the voice and swells
 * for the outro. Scene sound effects live inside the scenes themselves.
 */
export const Soundtrack: React.FC = () => {
  const { fps, durationInFrames } = useVideoConfig();
  const f = (s: number) => Math.round(s * fps);
  const last = TIMING.scenes[TIMING.scenes.length - 1];
  const total = durationInFrames / fps;
  return (
    <>
      {TIMING.scenes.map((s) => (
        <Sequence key={s.id} from={f(s.lineStart)} durationInFrames={Math.max(1, f(s.srcEnd - s.srcStart))} layout="none" name={`voice ${s.id}`}>
          <Audio src={staticFile(TIMING.narrationFile)} trimBefore={f(s.srcStart)} trimAfter={f(s.srcEnd)} volume={0.85} />
        </Sequence>
      ))}
      <Audio
        src={staticFile(MUSIC)}
        trimAfter={durationInFrames}
        volume={(frame) =>
          interpolate(
            frame / fps,
            [0, 0.6, TIMING.scenes[0].lineStart, last.lineEnd, last.lineEnd + 1.0, total - 1.8, total],
            [0.34, 0.4, 0.12, 0.12, 0.3, 0.3, 0],
            { extrapolateLeft: "clamp", extrapolateRight: "clamp" },
          )
        }
      />
    </>
  );
};
