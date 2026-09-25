import { AbsoluteFill, Img, useCurrentFrame } from "remotion";
import { noise2D } from "@remotion/noise";
import { CameraMotionBlur } from "@remotion/motion-blur";
import { beatFrame } from "../timing";
import { brand, C, display, easeIn, easeOut, mono, ramp, ring } from "../theme";

const PHRASES = [
  { text: "SPEAK FREELY", sub: "F9 · START / STOP", start: beatFrame(1) },
  { text: "SHAPE IT", sub: "POLISH · STRUCTURE · LIVE REWRITE", start: beatFrame(2) },
  { text: "KEEP EVERY WORD", sub: "ORIGINALS · REWRITES · HISTORY", start: beatFrame(3) },
];
const END = beatFrame(4);
const COLLAPSE = END - 7;
const CY = 500;
const SIZE = 142;

const phraseIndexAt = (frame: number) => {
  let index = -1;
  PHRASES.forEach((p, i) => {
    if (frame >= p.start) index = i;
  });
  return index;
};

// The recording light that opens the reel, echoing the widget's header dot.
const Light: React.FC<{ frame: number }> = ({ frame }) => {
  const grow = ramp(frame, 0, 10) + 0.28 * ring((frame - 1) / 60, 2.2, 6);
  const stretch = ramp(frame, 18, PHRASES[0].start, easeIn);
  const out = ramp(frame, PHRASES[0].start, PHRASES[0].start + 6, easeOut, 1, 0);
  const s = 0.35 + 0.65 * grow;
  return (
    <AbsoluteFill style={{ opacity: out }}>
      <div
        style={{
          position: "absolute",
          left: 960 - 260,
          top: CY - 260,
          width: 520,
          height: 520,
          borderRadius: "50%",
          background: "radial-gradient(circle, rgba(233,27,39,0.30) 0%, rgba(233,27,39,0.08) 40%, transparent 70%)",
          transform: `scale(${s})`,
        }}
      />
      <div
        style={{
          position: "absolute",
          left: 960 - 700,
          top: CY - 3,
          width: 1400,
          height: 6,
          background:
            "linear-gradient(90deg, transparent, rgba(233,27,39,0.6) 30%, #fff 50%, rgba(233,27,39,0.6) 70%, transparent)",
          transform: `scaleX(${0.08 + 0.92 * stretch}) scaleY(${1 + 1.5 * stretch})`,
          opacity: 0.25 + 0.75 * stretch,
          filter: "blur(1px)",
        }}
      />
      <div
        style={{
          position: "absolute",
          left: 960 - 22,
          top: CY - 22,
          width: 44,
          height: 44,
          borderRadius: "50%",
          background: "radial-gradient(circle, #fff 0%, #ff8a90 30%, #E91B27 58%, rgba(233,27,39,0) 72%)",
          transform: `scale(${s * (1 - 0.5 * stretch)}) scaleX(${1 + 3 * stretch})`,
          boxShadow: `0 0 ${30 + 30 * grow}px rgba(233,27,39,0.8)`,
        }}
      />
      <div
        style={{
          ...mono(14, C.ink3),
          position: "absolute",
          left: 960 + 40,
          top: CY - 9,
          opacity: ramp(frame, 4, 12) * (1 - stretch),
        }}
      >
        00:00
      </div>
    </AbsoluteFill>
  );
};

// A burst of voice bars at the baseline while one phrase becomes the next.
const Waveform: React.FC<{ frame: number; at: number; width: number }> = ({ frame, at, width }) => {
  const t = frame - at;
  const env = t < -5 || t > 9 ? 0 : Math.sin(((t + 5) / 14) * Math.PI);
  if (env <= 0) return null;
  const bars = 84;
  return (
    <div style={{ position: "absolute", left: 960 - width / 2, top: CY - 80, width, height: 160 }}>
      {Array.from({ length: bars }, (_, i) => {
        const x = i / (bars - 1);
        const shape = Math.sin(x * Math.PI) ** 0.6;
        const n = 0.5 + 0.5 * noise2D("wave", i * 0.35, frame * 0.35);
        const h = 6 + 150 * env * shape * n;
        const hot = Math.abs(x - 0.5) < 0.18;
        return (
          <div
            key={i}
            style={{
              position: "absolute",
              left: x * width - 3,
              top: 80 - h / 2,
              width: 5,
              height: h,
              borderRadius: 3,
              background: hot ? C.red : C.ink,
              opacity: 0.35 + 0.65 * env,
              boxShadow: hot ? `0 0 12px ${C.redGlow}` : "none",
            }}
          />
        );
      })}
    </div>
  );
};

const Phrase: React.FC<{ index: number; frame: number }> = ({ index, frame }) => {
  const phrase = PHRASES[index];
  const next = PHRASES[index + 1];
  const local = frame - phrase.start;
  const outAt = next ? next.start : COLLAPSE;
  const squashIn = ramp(local, 0, 7, easeOut);
  const squashOut = ramp(frame, outAt - 5, outAt, easeIn);
  const scaleY = index === 0 ? 1 : 0.04 + 0.96 * squashIn;
  const y = scaleY * (1 - squashOut * 0.97);
  const split = index === 0 ? 0 : 10 * (1 - ramp(local, 0, 9));
  const letters = phrase.text.split("");
  const popAt = index === 0 ? 13 : 5;
  const pop = ramp(local, popAt, popAt + 9, easeOut) + 0.45 * ring((local - popAt - 2) / 60, 2.4, 5);
  const layer = (color: string, dx: number, blend?: React.CSSProperties["mixBlendMode"]) => (
    <div
      style={{
        position: "absolute",
        left: 0,
        right: 0,
        top: CY - SIZE * 0.72,
        display: "flex",
        justifyContent: "center",
        alignItems: "baseline",
        transform: `translateX(${dx}px) scaleY(${y})`,
        filter: `brightness(${1 + squashOut * 1.5})`,
        mixBlendMode: blend,
      }}
    >
      {letters.map((ch, i) => {
        const appear = index === 0 ? ramp(local, i * 0.9, i * 0.9 + 9, easeOut) : 1;
        return (
          <span
            key={i}
            style={{
              ...display(SIZE),
              color,
              display: "inline-block",
              whiteSpace: "pre",
              transform: `translateY(${(1 - appear) * 60}px)`,
              opacity: appear,
              textShadow: blend ? "none" : "0 0 26px rgba(255,255,255,0.22), 0 0 70px rgba(255,255,255,0.08)",
            }}
          >
            {ch}
          </span>
        );
      })}
      <Img
        src={brand("mluva-mark.svg")}
        style={{
          width: SIZE * 0.36,
          height: SIZE * 0.36,
          marginLeft: SIZE * 0.05,
          transform: `translateY(${SIZE * 0.1}px) scale(${Math.max(0, pop)})`,
          opacity: blend ? 0 : 1,
          filter: "drop-shadow(0 0 14px rgba(233,27,39,0.6))",
        }}
      />
    </div>
  );
  return (
    <AbsoluteFill>
      {split > 0.2 ? layer("rgba(255,40,60,0.8)", -split, "screen") : null}
      {split > 0.2 ? layer("rgba(40,220,255,0.6)", split, "screen") : null}
      {layer(C.ink, 0)}
    </AbsoluteFill>
  );
};

const Ruler: React.FC<{ frame: number }> = ({ frame }) => {
  const width = 1400;
  const draw = ramp(frame, PHRASES[0].start, PHRASES[0].start + 18);
  const marker = ramp(frame, PHRASES[0].start, COLLAPSE, (t) => t);
  const index = Math.max(0, phraseIndexAt(frame));
  const fade = ramp(frame, COLLAPSE - 4, COLLAPSE + 3, easeOut, 1, 0);
  const subIn = ramp(frame - PHRASES[index].start, 2, 12);
  return (
    <div style={{ position: "absolute", left: 960 - width / 2, top: CY + 58, width, opacity: fade }}>
      <div
        style={{
          height: 10,
          width: width * draw,
          backgroundImage:
            "repeating-linear-gradient(90deg, rgba(245,245,245,0.28) 0 1px, transparent 1px 14px), linear-gradient(rgba(245,245,245,0.3), rgba(245,245,245,0.3))",
          backgroundSize: "100% 5px, 100% 1px",
          backgroundPosition: "0 0, 0 0",
          backgroundRepeat: "repeat-x, no-repeat",
        }}
      />
      <div
        style={{
          position: "absolute",
          left: width * (0.08 + 0.84 * marker),
          top: -8,
          width: 2,
          height: 20,
          background: C.red,
          boxShadow: `0 0 14px 2px ${C.redGlow}`,
          opacity: draw,
        }}
      />
      <div style={{ display: "flex", justifyContent: "space-between", marginTop: 26 }}>
        <div style={{ ...mono(15, C.ink2), opacity: subIn, transform: `translateY(${(1 - subIn) * 8}px)` }}>
          {PHRASES[index].sub}
        </div>
        <div style={{ ...mono(15, C.ink3), opacity: draw }}>
          <span style={{ color: C.red }}>0{index + 1}</span> / 03
        </div>
      </div>
    </div>
  );
};

const Numeral: React.FC<{ frame: number }> = ({ frame }) => {
  const index = phraseIndexAt(frame);
  if (index < 0) return null;
  const local = frame - PHRASES[index].start;
  const inT = ramp(local, 0, 12);
  const fade = ramp(frame, COLLAPSE - 4, COLLAPSE + 3, easeOut, 1, 0);
  return (
    <div
      style={{
        ...display(640, 900),
        position: "absolute",
        left: 1030 - frame * 0.6,
        top: 140,
        color: "transparent",
        WebkitTextStroke: "1.5px rgba(245,245,245,0.055)",
        transform: `translateY(${(1 - inT) * 60}px)`,
        opacity: inT * fade,
      }}
    >
      0{index + 1}
    </div>
  );
};

const Frame: React.FC<{ frame: number }> = ({ frame }) => {
  const open = ramp(frame, PHRASES[0].start - 4, PHRASES[0].start + 10);
  const fade = ramp(frame, COLLAPSE - 4, COLLAPSE + 3, easeOut, 1, 0);
  const w = 1120 + 380 * open;
  const h = 330;
  const arm = 18;
  const corner = (x: number, y: number, sx: number, sy: number) => (
    <div
      style={{
        position: "absolute",
        left: x - (sx < 0 ? arm : 0),
        top: y - (sy < 0 ? arm : 0),
        width: arm,
        height: arm,
        borderColor: C.ink3,
        borderStyle: "solid",
        borderWidth: 0,
        [sy > 0 ? "borderTopWidth" : "borderBottomWidth"]: 1.5,
        [sx > 0 ? "borderLeftWidth" : "borderRightWidth"]: 1.5,
      }}
    />
  );
  const l = 960 - w / 2;
  const t = CY - 190;
  return (
    <div style={{ position: "absolute", inset: 0, opacity: open * fade }}>
      {corner(l, t, 1, 1)}
      {corner(l + w, t, -1, 1)}
      {corner(l, t + h, 1, -1)}
      {corner(l + w, t + h, -1, -1)}
    </div>
  );
};

// The last phrase folds into a bright line: the seed for the widget reveal.
const SeedLine: React.FC<{ frame: number }> = ({ frame }) => {
  const on = ramp(frame, COLLAPSE - 1, COLLAPSE + 3);
  const width = 420 + 760 * ramp(frame, COLLAPSE, END, easeOut);
  if (on <= 0) return null;
  return (
    <div
      style={{
        position: "absolute",
        left: 960 - width / 2,
        top: CY - 2,
        width,
        height: 4,
        background: "linear-gradient(90deg, transparent, #fff 12%, #fff 88%, transparent)",
        boxShadow: "0 0 24px 4px rgba(255,255,255,0.45), 0 0 60px 10px rgba(233,27,39,0.35)",
        opacity: on,
      }}
    />
  );
};

export const C01Kinetic: React.FC = () => {
  const frame = useCurrentFrame();
  const index = phraseIndexAt(frame);
  return (
    <AbsoluteFill>
      <Numeral frame={frame} />
      <Frame frame={frame} />
      <Light frame={frame} />
      <CameraMotionBlur shutterAngle={200} samples={6}>
        <AbsoluteFill>{index >= 0 && frame < COLLAPSE + 1 ? <Phrase index={index} frame={frame} /> : null}</AbsoluteFill>
      </CameraMotionBlur>
      <Waveform frame={frame} at={PHRASES[1].start} width={1080} />
      <Waveform frame={frame} at={PHRASES[2].start} width={1240} />
      <Ruler frame={frame} />
      <SeedLine frame={frame} />
    </AbsoluteFill>
  );
};
