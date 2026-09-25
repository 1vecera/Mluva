import { AbsoluteFill, Img, staticFile, useCurrentFrame } from "remotion";
import { C, easeIn, easeInOut, easeOut, mono, ramp } from "../theme";
import { Callout } from "../parts/Callout";
import { Keycap } from "../parts/Keycap";
import { hBlur } from "../parts/Blur";
import { scramble } from "../parts/Scramble";

// Geometry of tmp/showreel/widget/widget-take.json at QT_SCALE_FACTOR=3 (capture pixels).
const REGION = { w: 1644, h: 704 };
const RECORDING = { x: 72, y: 183, w: 1500, h: 450 };
const K = 0.8;
const LEFT = 960 - (RECORDING.x + RECORDING.w / 2) * K;
const TOP = 500 - (RECORDING.y + RECORDING.h / 2) * K;
const at = (cx: number, cy: number) => ({ x: LEFT + cx * K, y: TOP + cy * K });

// Capture time = 0.75 s + chapter time × 1.35: words stream from local frame 14,
// "Transcribing…" at 82 and Review at 96 (see widget-take.json timeline).
const TRIM = 45;
const RATE = 1.35;
const LAST = 307;
const LENGTH = 113;
const captureFrame = (frame: number) => Math.min(LAST, Math.round(TRIM + frame * RATE));

export const C02Widget: React.FC = () => {
  const frame = useCurrentFrame();
  const open = ramp(frame, 0, 12, easeOut);
  const exit = ramp(frame, LENGTH - 9, LENGTH, easeIn);
  const tilt = ramp(frame, 0, 16, easeOut);
  const dot = at(96, 213);
  const timer = at(1545, 213);
  const review = at(96, 102);
  const startPress = frame < 8 ? Math.sin((frame / 8) * Math.PI) : 0;
  const stopPress = frame >= 68 && frame < 76 ? Math.sin(((frame - 68) / 8) * Math.PI) : 0;
  const stopped = frame >= 68;
  const keyIn = ramp(frame, 0, 8);
  const keyLabel = stopped ? "STOP" : "START";
  const bottom = TOP + (RECORDING.y + RECORDING.h) * K;

  return (
    <AbsoluteFill>
    <AbsoluteFill style={{ background: "#000", opacity: open * (1 - exit) }} />
    <AbsoluteFill
      style={{
        transform: `translateX(${-900 * exit}px)`,
        filter: hBlur(60 * exit * (1 - exit) * 4),
        opacity: 1 - exit * 0.4,
      }}
    >
      <div
        style={{
          position: "absolute",
          left: 360,
          top: bottom - 40,
          width: 1200,
          height: 120,
          background: "radial-gradient(ellipse 50% 50% at 50% 50%, rgba(0,0,0,0.65), transparent 70%)",
          filter: "blur(10px)",
          opacity: open,
        }}
      />
      <div
        style={{
          position: "absolute",
          left: 360,
          top: TOP + RECORDING.y * K - 120,
          width: 1200,
          height: 600,
          borderRadius: "50%",
          background: "radial-gradient(ellipse 50% 50% at 50% 50%, rgba(233,27,39,0.07), transparent 70%)",
          opacity: open,
        }}
      />
      <AbsoluteFill style={{ perspective: 2400 }}>
        <div
          style={{
            position: "absolute",
            left: LEFT,
            top: TOP,
            width: REGION.w * K,
            height: REGION.h * K,
            transformOrigin: "50% 58%",
            transform: tilt < 1 ? `rotateX(${12 * (1 - tilt)}deg) rotateY(${-8 * (1 - tilt)}deg) scale(${0.96 + 0.04 * open})` : undefined,
            clipPath: open < 1 ? `inset(${(1 - open) * 50}% 0 ${(1 - open) * 50}% 0)` : undefined,
          }}
        >
          <Img src={staticFile(`showreel/widget/${String(captureFrame(frame)).padStart(4, "0")}.png`)} style={{ width: "100%", height: "100%" }} />
        </div>
      </AbsoluteFill>
      <div
        style={{
          position: "absolute",
          left: 960 - 600 * (1 - open) - 10,
          top: 500 - 2,
          width: 1200 * (1 - open) + 20,
          height: 4,
          background: "#fff",
          boxShadow: "0 0 24px 4px rgba(255,255,255,0.45)",
          opacity: 1 - open,
        }}
      />
      <Callout frame={frame} at={14} until={64} x={dot.x} y={dot.y} dx={-40} dy={-78} label="RECORDING LIGHT" align="right" />
      <Callout frame={frame} at={22} until={64} x={timer.x} y={timer.y} dx={40} dy={-78} label="LIVE TIMER" />
      <Callout frame={frame} at={98} x={review.x} y={review.y} dx={-40} dy={-60} label="COPIED TO CLIPBOARD" align="right" />
      <div
        style={{
          position: "absolute",
          left: 960 - 44,
          top: bottom + 42,
          opacity: keyIn,
          transform: `translateY(${(1 - keyIn) * 20}px)`,
        }}
      >
        <Keycap label="F9" size={80} press={Math.max(startPress, stopPress)} lit={Math.max(startPress, stopPress)} />
        <div style={{ ...mono(14, C.ink2), position: "absolute", left: 104, top: 38 }}>
          {scramble(keyLabel, ramp(frame, stopped ? 68 : 2, stopped ? 80 : 14), keyLabel, frame)}
        </div>
      </div>
    </AbsoluteFill>
    </AbsoluteFill>
  );
};
