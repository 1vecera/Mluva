import {
  AbsoluteFill,
  Audio,
  Easing,
  Img,
  interpolate,
  Loop,
  OffthreadVideo,
  Sequence,
  staticFile,
  useCurrentFrame,
  useVideoConfig,
} from "remotion";
import data from "./generated/intro.json";
import { APP_HEIGHT, APP_WIDTH, NativeApp, type Surface } from "./Surfaces";
import "./style.css";
import { loadFont } from "@remotion/fonts";

loadFont({
  family: "JetBrains Mono",
  url: staticFile("fonts/JetBrainsMono-Regular.ttf"),
  weight: "400",
});

const ease = Easing.bezier(0.42, 0, 0.2, 1);
const move = (
  t: number,
  start: number,
  end: number,
  from: number,
  to: number,
) =>
  interpolate(t, [start, end], [from, to], {
    easing: ease,
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });
type Shot = { id: string; start: number; end: number; label: string };
export const SHOTS: Shot[] = data.shots;

const Brand: React.FC<{ t: number; closing?: boolean }> = ({
  t,
  closing = false,
}) => {
  const enter = closing ? 1 : move(t, 0, 0.85, 0, 1);
  return (
    <div
      style={{
        position: "absolute",
        inset: 0,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        flexDirection: "column",
        gap: 42,
        opacity: enter,
        transform: `translateY(${(1 - enter) * 28}px)`,
      }}
    >
      <Img
        src={staticFile("brand/mluva-logo-large-mark-on-dark.svg")}
        style={{ width: 700, height: 270, objectFit: "contain" }}
      />
      <div
        style={{
          fontSize: closing ? 48 : 40,
          letterSpacing: -1.5,
          color: "#edf0f5",
          textAlign: "center",
          lineHeight: 1.35,
        }}
      >
        {closing
          ? "Talk it through. Make it useful."
          : "A home for your spoken thoughts."}
      </div>
      <div
        style={{
          fontSize: 19,
          letterSpacing: 1.5,
          color: "#a0abba",
          opacity: move(t, 1.1, 1.55, 0, 1),
        }}
      >
        {closing
          ? "github.com/1vecera/Mluva"
          : "DICTATION · LIVE REWRITE · OMARCHY"}
      </div>
    </div>
  );
};

const AppShot: React.FC<{ shot: Shot; t: number }> = ({ shot, t }) => {
  const { fps } = useVideoConfig();
  let surface: Surface = "original";
  let zoom = 1;
  let focusY = APP_HEIGHT / 2;
  const captureTime = t - (data.actions.key - shot.start);
  if (shot.id === "record")
    surface =
      captureTime < 0
        ? "empty"
        : captureTime < 0.45
          ? "dictation-empty"
          : captureTime < 1.1
            ? "dictation-1"
            : captureTime < 1.75
              ? "dictation-2"
              : captureTime < 2.7
                ? "dictation-full"
                : "original";
  if (shot.id === "edit") {
    surface =
      t < 0.7
        ? "original"
        : t < data.actions.polish - shot.start
          ? "original-editable"
          : t < data.actions.structure - shot.start
            ? "polished"
            : "structured";
    zoom = move(t, 1.75, 2.55, 1, 1.3);
    focusY = move(t, 1.75, 2.55, APP_HEIGHT / 2, 245);
  }
  if (shot.id === "live") {
    surface =
      t < 2.3
        ? "live-empty"
        : t < 3.4
          ? "live-progress-1"
          : t < data.actions.draft - shot.start
            ? "live-progress-3"
            : "grilling";
    zoom = move(t, 3.4, 4.4, 1, 1.3);
    focusY = move(t, 3.4, 4.4, APP_HEIGHT / 2, 245);
  }
  if (shot.id === "history")
    surface =
      t < data.actions.search - shot.start ? "history" : "history-search";
  const themeAt = data.actions.theme - shot.start;
  if (shot.id === "models") {
    surface =
      t < themeAt
        ? "provider-settings"
        : t < themeAt + 0.7
          ? "theme-nord"
          : t < themeAt + 1.45
            ? "theme-tokyo-night"
            : "theme-rose-pine";
    zoom = move(t, themeAt - 0.45, themeAt, 1.15, 1);
    focusY = move(t, themeAt - 0.45, themeAt, 280, APP_HEIGHT / 2);
  }
  const entering = shot.id === "record" ? move(t, 0, 0.6, 0, 1) : 1;
  const scale = 1.28 * zoom;
  return (
    <>
      <div
        style={{
          position: "absolute",
          left: 104,
          top: 62,
          fontSize: 32,
          letterSpacing: -0.9,
          opacity: entering,
          transform: `translateY(${(1 - entering) * 12}px)`,
        }}
      >
        {shot.label}
      </div>
      <div
        style={{
          position: "absolute",
          right: 108,
          top: 64,
          fontSize: 17,
          letterSpacing: 2,
          color: "#9cabbf",
          opacity: entering,
        }}
      >
        MLUVA
      </div>
      {shot.id === "live" ? (
        <div
          style={{
            position: "absolute",
            left: 106,
            top: 108,
            fontSize: 16,
            color: "#b9aab7",
          }}
        >
          Live rewrite · Experimental
        </div>
      ) : null}
      <div
        style={{
          position: "absolute",
          left: 64,
          top: 126,
          width: 1792,
          height: 830,
          overflow: "hidden",
          borderRadius: 18,
        }}
      >
        <div
          style={{
            position: "absolute",
            left: 896 - (APP_WIDTH * scale) / 2,
            top: 414 - focusY * scale + (1 - entering) * 38,
            transform: `scale(${scale})`,
            transformOrigin: "top left",
            opacity: entering,
            filter: "drop-shadow(0px 24px 42px #0009)",
          }}
        >
          {shot.id === "history" && t < 1.2 ? (
            <Sequence from={Math.ceil(shot.start * fps)} layout="none">
              <OffthreadVideo
                src={staticFile("ui/history-open.mp4")}
                muted
                style={{ width: APP_WIDTH, height: APP_HEIGHT }}
              />
            </Sequence>
          ) : (
            <NativeApp
              surface={surface}
              clock={
                shot.id === "live"
                  ? Math.floor(t)
                  : shot.id === "record" && surface.startsWith("dictation")
                    ? Math.floor(captureTime)
                    : undefined
              }
            />
          )}
          {shot.id === "models" && t >= themeAt + 1.45 && t < themeAt + 1.77 ? (
            <div
              style={{
                position: "absolute",
                inset: 0,
                opacity: move(t, themeAt + 1.45, themeAt + 1.77, 1, 0),
              }}
            >
              <NativeApp surface="theme-tokyo-night" />
            </div>
          ) : null}
          {shot.id === "edit"
            ? [
                { at: data.actions.polish - shot.start, x: 65 },
                { at: data.actions.structure - shot.start, x: 199 },
              ].map(({ at, x }) =>
                t >= at - 0.55 && t < at + 0.3 ? (
                  <svg
                    key={at}
                    width="20"
                    height="26"
                    viewBox="0 0 20 26"
                    style={{
                      position: "absolute",
                      left: x + move(t, at - 0.55, at - 0.08, 35, 0),
                      top: 434 + move(t, at - 0.55, at - 0.08, 25, 0),
                      opacity: Math.min(
                        move(t, at - 0.55, at - 0.4, 0, 1),
                        move(t, at + 0.1, at + 0.3, 1, 0),
                      ),
                      transform: `scale(${t >= at && t < at + 0.1 ? 0.88 : 1})`,
                      transformOrigin: "2px 2px",
                      filter: "drop-shadow(0 2px 2px #0008)",
                    }}
                  >
                    <path
                      d="M2 1 L2 19 L7 15 L11 24 L15 22 L11 14 L18 14 Z"
                      fill="#f2f5f9"
                      stroke="#121821"
                      strokeWidth="1.5"
                    />
                  </svg>
                ) : null,
              )
            : null}
        </div>
      </div>
      {shot.id === "record" && captureTime >= -0.7 && captureTime < 1.3 ? (
        <div
          style={{
            position: "absolute",
            left: 1710,
            top: 795,
            textAlign: "center",
            opacity: Math.min(
              move(captureTime, -0.7, -0.4, 0, 1),
              move(captureTime, 1, 1.3, 1, 0),
            ),
          }}
        >
          <div
            style={{
              background: "#171e2a",
              border: "1px solid #8b9db6",
              borderRadius: 14,
              padding: "20px 30px",
              fontSize: 36,
              boxShadow: "0 14px 28px #0008",
              transform: `scale(${captureTime >= 0 && captureTime < 0.12 ? 0.94 : 1})`,
            }}
          >
            F9
          </div>
          <div
            style={{
              fontSize: 14,
              letterSpacing: 1.5,
              marginTop: 16,
              color: "#b5c2d3",
            }}
          >
            TO TALK
          </div>
        </div>
      ) : null}
      {shot.id === "models" && t >= themeAt ? (
        <div
          style={{
            position: "absolute",
            right: 104,
            top: 102,
            fontSize: 18,
            color: "#a8b3c5",
          }}
        >
          {surface === "theme-nord"
            ? "Nord"
            : surface === "theme-tokyo-night"
              ? "Tokyo Night"
              : "Rosé Pine"}{" "}
          · Omarchy
        </div>
      ) : null}
    </>
  );
};

export const MluvaIntro: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const t = frame / fps;
  const shot =
    SHOTS.find((s) => t >= s.start && t < s.end) ?? SHOTS[SHOTS.length - 1];
  const local = t - shot.start;
  const caption = data.captions.find((c) => t >= c.start && t < c.end);
  const brandWeight =
    shot.id === "intro"
      ? 1
      : shot.id === "record"
        ? move(local, 0, 0.5, 1, 0)
        : shot.id === "outro"
          ? move(local, 0, 0.4, 0, 1)
          : 0;
  return (
    <AbsoluteFill
      style={{
        background: "#080e17",
        color: "#f0f3f8",
        fontFamily: "JetBrains Mono",
      }}
    >
      <div
        style={{
          position: "absolute",
          inset: 0,
          opacity: 0.18 + brandWeight * 0.42,
        }}
      >
        <Loop durationInFrames={12 * fps}>
          <OffthreadVideo
            src={staticFile("background.mp4")}
            muted
            playbackRate={0.5}
            style={{ width: "100%", height: "100%", objectFit: "cover" }}
          />
        </Loop>
      </div>
      <AbsoluteFill
        style={{
          background:
            "radial-gradient(ellipse at center, transparent 25%, #050b1399 100%)",
        }}
      />
      {brandWeight > 0 ? (
        <AbsoluteFill
          style={{
            opacity: brandWeight,
            background:
              "radial-gradient(ellipse 58% 42% at 50% 48%, #050b1380 0%, #050b1330 68%, transparent 100%)",
          }}
        />
      ) : null}
      {shot.id === "record" && local < 0.5 ? (
        <AbsoluteFill style={{ opacity: move(local, 0, 0.5, 1, 0) }}>
          <Brand t={shot.start} />
        </AbsoluteFill>
      ) : null}
      {shot.id === "intro" || shot.id === "outro" ? (
        <AbsoluteFill style={{ opacity: brandWeight }}>
          <Brand key={shot.id} t={local} closing={shot.id === "outro"} />
        </AbsoluteFill>
      ) : (
        <AppShot shot={shot} t={local} />
      )}
      {shot.id === "outro" && local < 0.4 ? (
        <AbsoluteFill style={{ opacity: 1 - brandWeight }}>
          <AppShot shot={SHOTS[5]} t={SHOTS[5].end - SHOTS[5].start} />
        </AbsoluteFill>
      ) : null}
      {caption && shot.id !== "outro" ? (
        <div
          style={{
            position: "absolute",
            left: 180,
            right: 180,
            bottom: 35,
            textAlign: "center",
            fontSize: 26,
            lineHeight: 1.35,
            color: "#edf1f7",
            textShadow: "0 2px 8px #000",
          }}
        >
          {caption.text}
        </div>
      ) : null}
      <Sequence from={Math.round(data.voiceStart * fps)}>
        <Audio src={staticFile("audio/intro-voice.wav")} volume={0.82} />
      </Sequence>
      <Audio
        src={staticFile("audio/music-a.mp3")}
        volume={(f) => {
          const time = f / fps;
          const ending = move(
            time,
            data.voiceStart + data.voiceDuration,
            data.voiceStart + data.voiceDuration + 0.6,
            0.065,
            0.13,
          );
          return (
            move(time, 0, 0.8, 0, 1) *
            ending *
            move(time, data.duration - 1.8, data.duration, 1, 0)
          );
        }}
      />
      {[
        data.actions.key,
        data.actions.polish,
        data.actions.structure,
        data.actions.search,
      ].map((at) => (
        <Sequence key={at} from={Math.round(at * fps)}>
          <Audio src={staticFile("sfx/mouse-click.wav")} volume={0.06} />
        </Sequence>
      ))}
      {shot.id === "outro" ? (
        <div
          style={{
            position: "absolute",
            bottom: 50,
            width: "100%",
            textAlign: "center",
            fontSize: 17,
            color: "#9aa7b8",
          }}
        >
          Demonstration content · timings edited
        </div>
      ) : null}
      <AbsoluteFill
        style={{
          background: "#080e17",
          opacity: move(t, data.duration - 0.65, data.duration, 0, 1),
          pointerEvents: "none",
        }}
      />
    </AbsoluteFill>
  );
};
