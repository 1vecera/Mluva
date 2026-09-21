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
import { CaptionPhrase, PoppyCaptions } from "./PoppyCaptions";
import "./style.css";

export type EditPlan = {
  duration: number;
  poppy_captions: CaptionPhrase[];
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
      left: 470,
      top: 600,
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
  const key = edit.keys.find((cue) => t >= cue.start && t < cue.end);
  const recorderScene = t >= 25.5 && t < edit.camera.detail_start;
  const presenterLeft = recorderScene ? 116 : 1544;
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
          left: 96,
          top: 0,
          width: 1728,
          height: 1080,
          overflow: "hidden",
          borderRadius: 0,
        }}
      >
        <AbsoluteFill
          style={{ transform: `scale(${camera})`, transformOrigin: "60% 46.3%" }}
        >
          <OffthreadVideo
            src={staticFile("live/desktop-daniel.mp4")}
            muted
            style={{ width: 1728, height: 1080 }}
          />
        </AbsoluteFill>
        {closing > 0 && <Closing opacity={closing} />}
      </div>
      {opening > 0 && <OpeningInfo opacity={opening} />}
      {key && (
        <div
          style={{
            position: "absolute",
            top: 168,
            left: 126,
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
          left: presenterLeft,
          bottom: -14,
          width: 260,
          height: 349,
        }}
      >
        <OffthreadVideo
          src={staticFile("live/daniel-cutout.webm")}
          transparent
          muted
          style={{ position: "relative", width: "100%", height: "100%" }}
        />
      </div>
      {name > 0 && (
        <div
          style={{
            position: "absolute",
            left: presenterLeft - 6,
            bottom: 24,
            width: 272,
            padding: "10px 14px",
            borderRadius: 10,
            background: "rgba(8, 15, 34, .96)",
            border: "1px solid #7183a466",
            fontSize: 25,
            fontFamily: '"Adwaita Sans", sans-serif',
            fontWeight: 700,
            textAlign: "center",
            opacity: name,
          }}
        >
          {edit.name.text}
        </div>
      )}
      <PoppyCaptions phrases={edit.poppy_captions} time={t} />
      <Audio src={staticFile("audio/narration-mix.wav")} />
      <AbsoluteFill
        style={{
          background: "#04091c",
          opacity: ramp(t, edit.duration - 0.3, edit.duration),
        }}
      />
    </AbsoluteFill>
  );
};
