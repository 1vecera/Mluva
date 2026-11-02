import {
  AbsoluteFill,
  Easing,
  Img,
  interpolate,
  OffthreadVideo,
  staticFile,
  useCurrentFrame,
  useVideoConfig,
} from "remotion";
import { loadFont } from "@remotion/fonts";
import { CaptionPhrase, PoppyCaptions } from "../PoppyCaptions";
import "../style.css";

export type KeyCue = { start: number; end: number; key: string; label: string };
export type CameraKey = { t: number; scale: number; x: number; y: number };
export type PresenterKey = { t: number; x: number; bottom: number; height: number };
export type Dropped = { text: string; start: number };
export type FilmPlan = {
  duration: number;
  poppy_captions: CaptionPhrase[];
  keys: KeyCue[];
  camera: CameraKey[];
  presenter: PresenterKey[];
  name: { start: number; end: number; text: string };
  logo: { start: number; end: number };
  dropped: { items: Dropped[]; strike: number; end: number; dim_start: number; dim_end: number };
  still: { start: number; end: number; src: string; x: number; y: number; width: number };
  closing: {
    start: number;
    words: { text: string; start: number }[];
    url_start: number;
    fade_start: number;
  };
};
export type FilmProps = { plan: FilmPlan | null };

loadFont({ family: "JetBrains Mono", url: staticFile("fonts/JetBrainsMono-Regular.ttf"), weight: "400" });
loadFont({ family: "Adwaita Sans", url: staticFile("fonts/AdwaitaSans-Regular.ttf"), weight: "100 900" });

const ease = Easing.bezier(0.42, 0, 0.2, 1);
const clamp = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const;
const ramp = (t: number, a: number, b: number, from = 0, to = 1) =>
  interpolate(t, [a, b], [from, to], { easing: ease, ...clamp });
const envelope = (t: number, start: number, end: number, fade = 0.14) =>
  ramp(t, start, start + fade) * ramp(t, end - fade, end, 1, 0);

// Between two keys the value holds, then eases over the last `move` seconds before the later key.
function track<T extends { t: number }>(keys: T[], t: number, field: keyof T, move: number): number {
  let value = keys[0][field] as unknown as number;
  for (let i = 1; i < keys.length; i++) {
    const next = keys[i][field] as unknown as number;
    if (t >= keys[i].t) value = next;
    else if (t >= keys[i].t - move) {
      return interpolate(t, [keys[i].t - move, keys[i].t], [value, next], { easing: ease, ...clamp });
    } else return value;
  }
  return value;
}

const KeyToken: React.FC<{ cue: KeyCue; t: number }> = ({ cue, t }) => {
  const pop = interpolate(t - cue.start, [0, 0.07, 0.2], [0.94, 1.04, 1], clamp);
  const pressed = interpolate(t - cue.start, [0, 0.05, 0.16], [0, 3, 0], clamp);
  return (
    <div
      style={{
        position: "absolute",
        left: 56,
        bottom: 52,
        display: "flex",
        alignItems: "center",
        gap: 18,
        opacity: envelope(t, cue.start, cue.end, 0.1),
        transform: `scale(${pop})`,
        transformOrigin: "0 100%",
        padding: "13px 24px 13px 14px",
        borderRadius: 16,
        background: "rgba(10, 14, 30, .9)",
        border: "1px solid #7183a455",
        boxShadow: "0 10px 34px #0006",
        fontFamily: '"JetBrains Mono", monospace',
      }}
    >
      <span
        style={{
          padding: "7px 15px 8px",
          borderRadius: 10,
          background: "#edf0f5",
          color: "#0c1022",
          fontSize: 30,
          fontWeight: 700,
          transform: `translateY(${pressed}px)`,
          boxShadow: `0 ${4 - pressed}px 0 #8390ad`,
        }}
      >
        {cue.key}
      </span>
      <span style={{ fontSize: 28, color: "#edf0f5" }}>{cue.label}</span>
    </div>
  );
};

const DroppedFeatures: React.FC<{ plan: FilmPlan; t: number }> = ({ plan, t }) => {
  const { items, strike, end } = plan.dropped;
  const leave = ramp(t, end - 0.5, end, 1, 0);
  if (t < items[0].start - 0.1 || leave <= 0) return null;
  return (
    <AbsoluteFill
      style={{
        alignItems: "center",
        justifyContent: "center",
        flexDirection: "column",
        gap: 14,
        opacity: leave,
        fontFamily: '"Adwaita Sans", sans-serif',
      }}
    >
      {items.map((item, index) => {
        const enter = ramp(t, item.start - 0.02, item.start + 0.2);
        const line = ramp(t, strike + index * 0.13, strike + index * 0.13 + 0.34);
        return (
          <div
            key={item.text}
            style={{
              position: "relative",
              fontSize: 104,
              fontWeight: 800,
              letterSpacing: -2.6,
              lineHeight: 1.16,
              color: line > 0.5 ? "#8d97ad" : "#f4f6fb",
              opacity: enter,
              transform: `translateY(${(1 - enter) * 16}px)`,
            }}
          >
            {item.text}
            <div
              style={{
                position: "absolute",
                left: -14,
                right: -14,
                top: "54%",
                height: 9,
                borderRadius: 5,
                background: "oklch(0.74 0.19 28)",
                transform: `scaleX(${line})`,
                transformOrigin: "0 50%",
              }}
            />
          </div>
        );
      })}
    </AbsoluteFill>
  );
};

const Closing: React.FC<{ plan: FilmPlan; t: number }> = ({ plan, t }) => {
  const { closing } = plan;
  const open = ramp(t, closing.start - 0.5, closing.start + 0.2);
  if (open <= 0) return null;
  const spoken = closing.words.filter((word) => t >= word.start);
  return (
    <div
      style={{
        position: "absolute",
        left: 120,
        top: 330,
        width: 900,
        opacity: open,
        display: "flex",
        flexDirection: "column",
        gap: 38,
      }}
    >
      <Img
        src={staticFile("brand/mluva-logo-large-mark-on-dark.svg")}
        style={{ width: 560, height: 210, objectFit: "contain", objectPosition: "0 50%" }}
      />
      <div style={{ display: "flex", gap: 30, minHeight: 74, fontFamily: '"Adwaita Sans", sans-serif' }}>
        {closing.words.map((word) => {
          const seen = spoken.includes(word);
          const pop = seen ? interpolate(t - word.start, [0, 0.08, 0.22], [0.94, 1.05, 1], clamp) : 1;
          return (
            <span
              key={word.text}
              style={{
                fontSize: 58,
                fontWeight: 800,
                letterSpacing: -1.4,
                color: "#f4f6fb",
                opacity: seen ? 1 : 0,
                transform: `scale(${pop})`,
              }}
            >
              {word.text}
            </span>
          );
        })}
      </div>
      <div
        style={{
          fontFamily: '"JetBrains Mono", monospace',
          fontSize: 34,
          color: "#c9d4e6",
          opacity: ramp(t, closing.url_start, closing.url_start + 0.3),
        }}
      >
        github.com/1vecera/Mluva
      </div>
    </div>
  );
};

export const MluvaFilm: React.FC<FilmProps> = ({ plan }) => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  if (!plan) throw new Error("Restore public/film/plan.json from the local media before rendering.");
  const t = frame / fps;
  const key = plan.keys.find((cue) => t >= cue.start && t < cue.end);
  const scale = track(plan.camera, t, "scale", 0.8);
  const camX = track(plan.camera, t, "x", 0.8);
  const camY = track(plan.camera, t, "y", 0.8);
  const height = track(plan.presenter, t, "height", 0.7);
  const left = track(plan.presenter, t, "x", 0.7);
  const bottom = track(plan.presenter, t, "bottom", 0.7);
  const dim = envelope(t, plan.dropped.dim_start, plan.dropped.dim_end, 0.45);
  const closingDim = ramp(t, plan.closing.start - 0.6, plan.closing.start + 0.2);
  const logo = envelope(t, plan.logo.start, plan.logo.end, 0.22);
  const name = envelope(t, plan.name.start, plan.name.end, 0.25);
  const still = envelope(t, plan.still.start, plan.still.end, 0.25);
  return (
    <AbsoluteFill style={{ background: "#04091c", overflow: "hidden", color: "#edf0f5" }}>
      <AbsoluteFill
        style={{
          transform: `scale(${scale})`,
          transformOrigin: `${camX}% ${camY}%`,
          filter: dim > 0 ? `blur(${dim * 14}px) brightness(${1 - dim * 0.5})` : undefined,
        }}
      >
        <OffthreadVideo src={staticFile("film/desktop.mp4")} muted style={{ width: 1920, height: 1080 }} />
      </AbsoluteFill>
      {closingDim > 0 && <AbsoluteFill style={{ background: `rgba(4, 9, 28, ${closingDim * 0.34})` }} />}
      <DroppedFeatures plan={plan} t={t} />
      {still > 0 && (
        <Img
          src={staticFile(plan.still.src)}
          style={{
            position: "absolute",
            left: plan.still.x,
            top: plan.still.y,
            width: plan.still.width,
            borderRadius: 14,
            border: "1px solid #ffffff30",
            boxShadow: "0 24px 70px #000a",
            opacity: still,
            transform: `scale(${0.96 + still * 0.04})`,
          }}
        />
      )}
      {logo > 0 && (
        <Img
          src={staticFile("brand/mluva-logo-large-mark-on-dark.svg")}
          style={{
            position: "absolute",
            left: 150,
            top: 400,
            width: 640,
            height: 240,
            objectFit: "contain",
            opacity: logo,
            transform: `scale(${0.97 + logo * 0.03})`,
          }}
        />
      )}
      <Closing plan={plan} t={t} />
      <div style={{ position: "absolute", left, bottom, height, width: height * (900 / 1200) }}>
        <OffthreadVideo
          src={staticFile("film/daniel.webm")}
          transparent
          muted
          style={{ width: "100%", height: "100%" }}
        />
      </div>
      {name > 0 && (
        <div
          style={{
            position: "absolute",
            left: 150,
            top: 650,
            fontFamily: '"Adwaita Sans", sans-serif',
            fontSize: 44,
            fontWeight: 700,
            letterSpacing: -0.8,
            opacity: name,
          }}
        >
          {plan.name.text}
        </div>
      )}
      {key && <KeyToken cue={key} t={t} />}
      <PoppyCaptions phrases={plan.poppy_captions} time={t} />
      <AbsoluteFill
        style={{ background: "#04091c", opacity: ramp(t, plan.closing.fade_start, plan.duration) }}
      />
    </AbsoluteFill>
  );
};
