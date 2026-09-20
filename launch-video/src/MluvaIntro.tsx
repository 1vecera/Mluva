import {
  AbsoluteFill,
  Audio,
  Easing,
  Img,
  interpolate,
  OffthreadVideo,
  staticFile,
  useCurrentFrame,
  useVideoConfig,
} from "remotion";
import { loadFont } from "@remotion/fonts";
import cues from "../reference/narration-cues.json";
import "./style.css";

loadFont({
  family: "JetBrains Mono",
  url: staticFile("fonts/JetBrainsMono-Regular.ttf"),
  weight: "400",
});
const ease = Easing.bezier(0.42, 0, 0.2, 1);
const ramp = (t: number, a: number, b: number, from = 0, to = 1) =>
  interpolate(t, [a, b], [from, to], {
    easing: ease,
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });
const envelope = (t: number, start: number, end: number, fade = 0.22) =>
  ramp(t, start, start + fade) * ramp(t, end - fade, end, 1, 0);
// Reframe native desktop footage; the app remains its original recorded pixels.
const camera = (t: number) =>
  1 +
  0.53 * envelope(t, 5.55, 11.95, 0.6) +
  0.53 * envelope(t, 26.5, 55.35, 0.65);

const Brand: React.FC<{ opacity: number; closing?: boolean }> = ({
  opacity,
  closing = false,
}) => (
  <AbsoluteFill
    style={{
      opacity,
      alignItems: "center",
      justifyContent: "center",
      background: "rgba(4, 9, 28, 0.88)",
      gap: 36,
    }}
  >
    <Img
      src={staticFile("brand/mluva-logo-large-mark-on-dark.svg")}
      style={{
        width: 690,
        height: 260,
        objectFit: "contain",
        transform: `translateY(${(1 - opacity) * 14}px)`,
      }}
    />
    <div
      style={{
        fontSize: 36,
        letterSpacing: -1.1,
        textAlign: "center",
        color: "#edf0f5",
      }}
    >
      {closing
        ? "A little more delight every day."
        : "Most delightful dictation for Omarchy"}
    </div>
    {closing && (
      <div
        style={{
          textAlign: "center",
          lineHeight: 2,
          fontSize: 22,
          color: "#b7c5d9",
        }}
      >
        github.com/1vecera/Mluva
        <br />
        <span style={{ fontSize: 18, color: "#91a2bc" }}>
          Open source · Apache-2.0
        </span>
      </div>
    )}
  </AbsoluteFill>
);
const keys = [
  { start: 12.18, end: 13.75, key: "F9", label: "Talk" },
  { start: 18.57, end: 19.23, key: "F9", label: "Finish" },
  { start: 19.23, end: 20.9, key: "Ctrl + V", label: "Paste" },
  { start: 21.95, end: 23.45, key: "Super + T", label: "Tile" },
  { start: 24.05, end: 25.6, key: "Super + T", label: "Float" },
  { start: 27.55, end: 29.15, key: "Ctrl + P", label: "Commands" },
];
export const MluvaIntro: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const t = frame / fps;
  const opening = envelope(t, 3, 6.05, 0.28);
  const closing = ramp(t, 55, 55.45);
  const key = keys.find((cue) => t >= cue.start && t < cue.end);
  const musicVolume = (f: number) => {
    const time = f / fps;
    const voice = Math.max(
      0,
      ...cues.cues.map((c) =>
        envelope(time, c.film_start - 0.18, c.film_end + 0.25, 0.18),
      ),
    );
    return (
      (0.32 - 0.17 * voice) * ramp(time, 0, 0.6) * ramp(time, 57.5, 59, 1, 0)
    );
  };
  return (
    <AbsoluteFill
      style={{
        background: "#04091c",
        overflow: "hidden",
        color: "#edf0f5",
        fontFamily: '"JetBrains Mono", monospace',
      }}
    >
      <AbsoluteFill
        style={{ transform: `scale(${camera(t)})`, transformOrigin: "50% 50%" }}
      >
        <OffthreadVideo
          src={staticFile("live/desktop.mp4")}
          muted
          style={{
            position: "absolute",
            left: 96,
            top: 0,
            width: 1728,
            height: 1080,
          }}
        />
      </AbsoluteFill>
      {key && (
        <div
          style={{
            position: "absolute",
            top: 46,
            left: "50%",
            transform: "translateX(-50%)",
            display: "flex",
            alignItems: "center",
            gap: 18,
            opacity: envelope(t, key.start, key.end, 0.12),
            fontSize: 24,
            padding: "13px 22px",
            borderRadius: 12,
            background: "rgba(8, 15, 34, .92)",
            border: "1px solid #7183a455",
          }}
        >
          <span style={{ color: "#a9d5ee" }}>{key.key}</span>
          <span>{key.label}</span>
        </div>
      )}
      {opening > 0 && <Brand opacity={opening} />}
      {closing > 0 && <Brand opacity={closing} closing />}
      <Audio src={staticFile("live/sarah.wav")} />
      <Audio src={staticFile("audio/music-bed.wav")} volume={musicVolume} />
    </AbsoluteFill>
  );
};
