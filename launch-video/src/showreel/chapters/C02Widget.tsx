import { AbsoluteFill, Easing, Img, staticFile, useCurrentFrame } from "remotion";
import { C, easeIn, easeOut, LABEL, mono, ramp } from "../theme";
import { Callout } from "../parts/Callout";
import { Keycap } from "../parts/Keycap";
import { hBlur } from "../parts/Blur";
import { scramble } from "../parts/Scramble";

// Geometry of tmp/showreel/widget/widget-take.json at QT_SCALE_FACTOR=3 (capture pixels).
// The widget grows upward for Review: recording spans y 183–633, Review y 72–633.
const REGION = { w: 1644, h: 704 };
const WIDGET = { x: 72, w: 1500, bottom: 633, recordingTop: 183, reviewTop: 72 };
const K = 0.8;
const LEFT = 960 - (WIDGET.x + WIDGET.w / 2) * K;
const TOP = 500 - ((WIDGET.recordingTop + WIDGET.bottom) / 2) * K;
const ORIGIN = { x: REGION.w * K * 0.5, y: REGION.h * K * 0.58 };
const PERSPECTIVE = 2200;

// Capture frame = 46 + chapter frame × 1.513: words stream from chapter frame 12, "Transcribing…"
// appears at 73 and Review at 85 (beat 7); the take's timer runs at natural speech pace.
const TRIM = 46;
const RATE = 1.513;
const LAST = 307;
const REVIEW_FRAME = 175;
const LENGTH = 113;
const captureFrame = (frame: number) => Math.min(LAST, Math.round(TRIM + frame * RATE));

const stage = (frame: number) => {
  const t = Easing.inOut(Easing.cubic)(Math.min(1, Math.max(0, frame / LENGTH)));
  return { rx: 9 - 7 * t, ry: -12 + 9 * t, s: 0.93 + 0.07 * t };
};

// Project a capture-pixel point on the tilted plane to the screen (CSS: rotateX · rotateY · scale).
const project = (frame: number, cx: number, cy: number) => {
  const { rx, ry, s } = stage(frame);
  const ax = (rx * Math.PI) / 180;
  const ay = (ry * Math.PI) / 180;
  let x = (cx * K - ORIGIN.x) * s;
  let y = (cy * K - ORIGIN.y) * s;
  let z = 0;
  [x, z] = [x * Math.cos(ay) + z * Math.sin(ay), -x * Math.sin(ay) + z * Math.cos(ay)];
  [y, z] = [y * Math.cos(ax) - z * Math.sin(ax), y * Math.sin(ax) + z * Math.cos(ax)];
  const px = LEFT + ORIGIN.x + x;
  const py = TOP + ORIGIN.y + y;
  const k = PERSPECTIVE / (PERSPECTIVE - z);
  return { x: 960 + (px - 960) * k, y: 540 + (py - 540) * k };
};

export const C02Widget: React.FC = () => {
  const frame = useCurrentFrame();
  const open = ramp(frame, 0, 12, easeOut);
  const exit = ramp(frame, LENGTH - 8, LENGTH, easeIn);
  const index = captureFrame(frame);
  const top = index >= REVIEW_FRAME ? WIDGET.reviewTop : WIDGET.recordingTop;
  const { rx, ry, s } = stage(frame);
  const mid = (top + WIDGET.bottom) / 2;
  const half = ((WIDGET.bottom - top) / 2) * open;
  const clip = `inset(${((mid - half) / REGION.h) * 100}% ${((REGION.w - WIDGET.x - WIDGET.w) / REGION.w) * 100}% ${
    ((REGION.h - mid - half) / REGION.h) * 100
  }% ${(WIDGET.x / REGION.w) * 100}%)`;
  // Leaders stop outside the widget, 14 px from its edge, so no captured pixel is covered.
  const gap = 14 / K;
  const dot = project(frame, WIDGET.x - gap, 213);
  const timer = project(frame, WIDGET.x + WIDGET.w + gap, 213);
  const review = project(frame, WIDGET.x - gap, 102);
  const startPress = frame < 8 ? Math.sin((frame / 8) * Math.PI) : 0;
  const stopPress = frame >= 67 && frame < 75 ? Math.sin(((frame - 67) / 8) * Math.PI) : 0;
  const stopped = frame >= 67;
  const keyIn = ramp(frame, 0, 8);
  const keyLabel = stopped ? "STOP" : "START";
  const bottom = project(frame, WIDGET.x + WIDGET.w / 2, WIDGET.bottom);

  return (
    <AbsoluteFill
      style={{
        transform: `translateX(${-900 * exit}px)`,
        filter: exit > 0 ? hBlur(60 * exit * (1 - exit) * 4) : undefined,
        opacity: 1 - exit * 0.4,
      }}
    >
      <AbsoluteFill style={{ perspective: PERSPECTIVE }}>
        <div
          style={{
            position: "absolute",
            left: LEFT,
            top: TOP,
            width: REGION.w * K,
            height: REGION.h * K,
            transformOrigin: `${ORIGIN.x}px ${ORIGIN.y}px`,
            transform: `rotateX(${rx}deg) rotateY(${ry}deg) scale(${s})`,
          }}
        >
          <div
            style={{
              position: "absolute",
              left: WIDGET.x * K,
              top: top * K,
              width: WIDGET.w * K,
              height: (WIDGET.bottom - top) * K,
              boxShadow: "0 60px 140px rgba(0,0,0,0.75), 0 0 0 1px rgba(245,245,245,0.04)",
              opacity: open,
            }}
          />
          <Img
            src={staticFile(`showreel/widget/${String(index).padStart(4, "0")}.png`)}
            style={{ position: "absolute", inset: 0, width: "100%", height: "100%", clipPath: clip }}
          />
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
      <Callout frame={frame} at={14} until={62} x={dot.x} y={dot.y} dx={-36} dy={-70} label="RECORDING LIGHT" align="right" />
      <Callout frame={frame} at={22} until={62} x={timer.x} y={timer.y} dx={36} dy={-70} label="TIME AS SPOKEN" />
      <Callout frame={frame} at={86} x={review.x} y={review.y} dx={-36} dy={-56} label="COPIED TO CLIPBOARD" align="right" />
      <div
        style={{
          position: "absolute",
          left: bottom.x - 40,
          top: bottom.y + 40,
          opacity: keyIn,
          transform: `translateY(${(1 - keyIn) * 20}px)`,
        }}
      >
        <Keycap label="F9" size={80} press={Math.max(startPress, stopPress)} lit={Math.max(startPress, stopPress)} />
        <div style={{ ...mono(15, LABEL), position: "absolute", left: 104, top: 36 }}>
          {scramble(keyLabel, ramp(frame, stopped ? 67 : 2, stopped ? 71 : 6), keyLabel, frame)}
        </div>
      </div>
      <div style={{ ...mono(15, C.ink3), position: "absolute", right: 94, top: 150, opacity: ramp(frame, 20, 30) * (1 - exit) }}>
        REAL CAPTURE · TIME-LAPSE
      </div>
    </AbsoluteFill>
  );
};
