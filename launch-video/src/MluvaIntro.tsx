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
import "./style.css";

export type EditPlan = {
  duration: number;
  captions: { start: number; end: number; text: string }[];
  opening_end: number;
  closing_start: number;
  name: { start: number; end: number; text: string };
  camera: { welcome_end: number; detail_start: number; detail_end: number };
  keys: { start: number; end: number; key: string; label: string }[];
};
export type IntroProps = { edit: EditPlan | null };

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
const envelope = (t: number, start: number, end: number, fade = 0.2) =>
  ramp(t, start, start + fade) * ramp(t, end - fade, end, 1, 0);

const OpeningInfo: React.FC<{ opacity: number }> = ({ opacity }) => (
  <div
    style={{
      position: "absolute",
      left: 315,
      top: 558,
      width: 980,
      padding: "24px 40px 28px",
      opacity,
      display: "flex",
      alignItems: "center",
      flexDirection: "column",
      gap: 14,
      borderRadius: 18,
      background: "rgba(4, 9, 28, 0.95)",
      border: "1px solid #7183a455",
      boxShadow: "0 16px 48px #0004",
      textAlign: "center",
    }}
  >
    <Img
      src={staticFile("brand/mluva-logo-large-mark-on-dark.svg")}
      style={{ width: 260, height: 96, objectFit: "contain" }}
    />
    <div style={{ fontSize: 30, lineHeight: 1.35, letterSpacing: -0.8 }}>
      The most delightful dictation for Omarchy.
    </div>
    <div style={{ fontSize: 22, color: "#b7c5d9" }}>
      Local or cloud. Your choice of tools.
    </div>
  </div>
);

const Closing: React.FC<{ opacity: number }> = ({ opacity }) => (
  <AbsoluteFill
    style={{
      opacity,
      alignItems: "center",
      justifyContent: "center",
      background: "rgba(4, 9, 28, 0.92)",
      gap: 32,
    }}
  >
    <Img
      src={staticFile("brand/mluva-logo-large-mark-on-dark.svg")}
      style={{ width: 600, height: 225, objectFit: "contain" }}
    />
    <div style={{ fontSize: 32, letterSpacing: -1 }}>
      A little more delight every day.
    </div>
    <div style={{ color: "#b7c5d9", textAlign: "center", lineHeight: 2 }}>
      <div style={{ fontSize: 25 }}>Free · Open source</div>
      <div style={{ fontSize: 23 }}>github.com/1vecera/Mluva</div>
    </div>
  </AbsoluteFill>
);

export const MluvaIntro: React.FC<IntroProps> = ({ edit }) => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  if (!edit) throw new Error("Restore the local human-narration edit plan before rendering.");
  const t = frame / fps;
  const subtitle = edit.captions.find((cue) => t >= cue.start && t < cue.end);
  const key = edit.keys.find((cue) => t >= cue.start && t < cue.end);
  const camera =
    1 +
    0.24 * ramp(t, edit.camera.welcome_end - 0.7, edit.camera.welcome_end, 1, 0) +
    0.42 * envelope(t, edit.camera.detail_start, edit.camera.detail_end + 0.4, 0.7);
  const opening = ramp(t, edit.opening_end - 0.45, edit.opening_end, 1, 0);
  const closing = ramp(t, edit.closing_start, edit.closing_start + 0.5);
  const name = envelope(t, edit.name.start, edit.name.end, 0.25);
  return (
    <AbsoluteFill
      style={{
        background: "#04091c",
        overflow: "hidden",
        color: "#edf0f5",
        fontFamily: '"JetBrains Mono", monospace',
      }}
    >
      <div
        style={{
          position: "absolute",
          left: 40,
          top: 24,
          width: 1536,
          height: 960,
          overflow: "hidden",
          borderRadius: 10,
        }}
      >
        <AbsoluteFill
          style={{ transform: `scale(${camera})`, transformOrigin: "50% 46.3%" }}
        >
          <OffthreadVideo
            src={staticFile("live/desktop-daniel.mp4")}
            muted
            style={{ width: 1536, height: 960 }}
          />
        </AbsoluteFill>
        {closing > 0 && <Closing opacity={closing} />}
      </div>
      {opening > 0 && <OpeningInfo opacity={opening} />}
      {key && (
        <div
          style={{
            position: "absolute",
            top: 46,
            left: 808,
            transform: "translateX(-50%)",
            display: "flex",
            gap: 18,
            opacity: envelope(t, key.start, key.end, 0.1),
            fontSize: 23,
            padding: "12px 20px",
            borderRadius: 12,
            background: "rgba(8, 15, 34, .94)",
            border: "1px solid #7183a455",
          }}
        >
          <span style={{ color: "#a9d5ee" }}>{key.key}</span>
          <span>{key.label}</span>
        </div>
      )}
      <div
        style={{
          position: "absolute",
          right: 18,
          bottom: 112,
          width: 350,
          height: 470,
          filter: "drop-shadow(0 6px 12px #0006)",
          maskImage:
            "linear-gradient(to bottom, black 0%, black 88%, transparent 100%), linear-gradient(to right, transparent, black 4%, black 96%, transparent)",
          maskComposite: "intersect",
        }}
      >
        <OffthreadVideo
          src={staticFile("live/daniel-cutout.webm")}
          transparent
          muted
          style={{ width: "100%", height: "100%" }}
        />
      </div>
      {name > 0 && (
        <div
          style={{
            position: "absolute",
            right: 24,
            bottom: 130,
            width: 334,
            padding: "12px 16px",
            borderRadius: 10,
            background: "rgba(8, 15, 34, .96)",
            border: "1px solid #7183a466",
            fontSize: 27,
            textAlign: "center",
            opacity: name,
          }}
        >
          {edit.name.text}
        </div>
      )}
      {subtitle && (
        <div
          style={{
            position: "absolute",
            bottom: 22,
            width: "100%",
            textAlign: "center",
            padding: "0 90px",
          }}
        >
          <span
            style={{
              display: "inline-block",
              maxWidth: 1650,
              padding: "12px 24px",
              borderRadius: 8,
              background: "rgba(4, 9, 22, 0.96)",
              color: "#fff",
              fontSize: 31,
              lineHeight: 1.3,
            }}
          >
            {subtitle.text}
          </span>
        </div>
      )}
      <Audio src={staticFile("live/daniel.wav")} />
      <Audio
        src={staticFile("audio/music-bed.wav")}
        volume={(f) =>
          0.2 * ramp(f / fps, 0, 1.2) * ramp(f / fps, edit.duration - 1.4, edit.duration, 1, 0)
        }
      />
      <AbsoluteFill
        style={{
          background: "#04091c",
          opacity: ramp(t, edit.duration - 0.3, edit.duration),
        }}
      />
    </AbsoluteFill>
  );
};
