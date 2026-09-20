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
  1 + 0.33 * ramp(t, 17.15, 18, 1, 0) + 0.58 * envelope(t, 44, 79.4, 0.85);

const OpeningInfo: React.FC<{ opacity: number }> = ({ opacity }) => (
  <div
    style={{
      position: "absolute",
      left: 460,
      top: 578,
      width: 1000,
      padding: "24px 40px 28px",
      opacity,
      display: "flex",
      alignItems: "center",
      flexDirection: "column",
      gap: 16,
      borderRadius: 18,
      background: "rgba(4, 9, 28, 0.95)",
      border: "1px solid #7183a455",
      boxShadow: "0 16px 48px #0004",
      textAlign: "center",
    }}
  >
    <Img
      src={staticFile("brand/mluva-logo-large-mark-on-dark.svg")}
      style={{ width: 280, height: 104, objectFit: "contain" }}
    />
    <div style={{ fontSize: 32, lineHeight: 1.35, letterSpacing: -0.8 }}>
      The most delightful dictation for Omarchy.
    </div>
    <div style={{ fontSize: 22, color: "#b7c5d9" }}>
      Local or cloud. Your choice of tools.
    </div>
  </div>
);

const Brand: React.FC<{ opacity: number }> = ({ opacity }) => (
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
      A little more delight every day.
    </div>
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
  </AbsoluteFill>
);
const keys = [
  { start: 18.3, end: 20.3, key: "F9", label: "Talk" },
  { start: 26.8, end: 27.6, key: "F9", label: "Finish" },
  { start: 27.6, end: 29.8, key: "Ctrl + V", label: "Paste" },
  { start: 33.05, end: 35.3, key: "Super + T", label: "Tile" },
  { start: 36.05, end: 38.15, key: "Super + T", label: "Float" },
  { start: 38.8, end: 41.8, key: "Super + 2", label: "Next desktop" },
  { start: 44.5, end: 46.3, key: "Ctrl + P", label: "Commands" },
];
export const MluvaIntro: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const t = frame / fps;
  const opening = ramp(t, 5.8, 6.4, 1, 0);
  const closing = ramp(t, 79, 79.55);
  const subtitle = cues.cues.find(
    (cue) => t >= cue.film_start && t < cue.subtitle_end,
  );
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
      (0.6 - 0.25 * voice) * ramp(time, 0, 1.2) * ramp(time, 81.5, 84, 1, 0)
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
        style={{
          transform: `scale(${camera(t)})`,
          transformOrigin: "50% 46.3%",
        }}
      >
        <OffthreadVideo
          src={staticFile("live/desktop.mp4")}
          muted
          style={{
            position: "absolute",
            left: 160,
            top: 0,
            width: 1600,
            height: 1000,
          }}
        />
      </AbsoluteFill>
      {opening > 0 && <OpeningInfo opacity={opening} />}
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
      {closing > 0 && <Brand opacity={closing} />}
      {subtitle && (
        <div
          style={{
            position: "absolute",
            bottom: 24,
            width: "100%",
            textAlign: "center",
            padding: "0 120px",
          }}
        >
          <span
            style={{
              display: "inline-block",
              maxWidth: 1440,
              padding: "12px 24px",
              borderRadius: 8,
              background: "rgba(4, 9, 22, 0.94)",
              color: "#ffffff",
              fontSize: 34,
              lineHeight: 1.3,
              fontFamily: '"JetBrains Mono", monospace',
              boxShadow: "0 2px 18px #0006",
            }}
          >
            {subtitle.text}
          </span>
        </div>
      )}
      <Audio src={staticFile("live/sarah.wav")} />
      <Audio src={staticFile("audio/music-bed.wav")} volume={musicVolume} />
    </AbsoluteFill>
  );
};
