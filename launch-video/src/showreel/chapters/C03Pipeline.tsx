import { AbsoluteFill, Img, staticFile, useCurrentFrame } from "remotion";
import { noise2D } from "@remotion/noise";
import { getLength, getPointAtLength } from "@remotion/paths";
import { beatFrame } from "../timing";
import { BLOOM, C, display, easeIn, easeInOut, easeOut, LABEL, MONO, mono, ramp, ring, T } from "../theme";
import { hBlur, vBlur } from "../parts/Blur";
import { beatPulse } from "../parts/Background";
import { MarkPeriod } from "../parts/MarkPeriod";

const B0 = beatFrame(8);
const BEATS = [0, 1, 2, 3].map((i) => beatFrame(8 + i) - B0);
const LENGTH = beatFrame(12) - B0;

const NODES = [
  { x: 420, y: 606, name: "Speak", sub: "01 · F9 · MICROPHONE", word: "Spoken" },
  { x: 800, y: 504, name: "Transcribe", sub: "02 · SCRIBE · LOCAL · API", word: "Heard" },
  { x: 1180, y: 606, name: "Copy", sub: "03 · CLIPBOARD", word: "Copied" },
  { x: 1560, y: 504, name: "Rewrite", sub: "04 · ON DEMAND", word: "Shaped" },
];

const segment = (a: (typeof NODES)[number], b: (typeof NODES)[number]) => {
  const mx = (a.x + b.x) / 2;
  return `C ${mx} ${a.y} ${mx} ${b.y} ${b.x} ${b.y}`;
};
const FIXED = `M ${NODES[0].x} ${NODES[0].y} ${segment(NODES[0], NODES[1])} ${segment(NODES[1], NODES[2])}`;
const ON_DEMAND = `M ${NODES[2].x} ${NODES[2].y} ${segment(NODES[2], NODES[3])}`;
const PATH = `${FIXED} ${segment(NODES[2], NODES[3])}`;
const TOTAL = getLength(PATH);
const FIXED_LENGTH = getLength(FIXED);
const STOPS = NODES.map((_, i) => (TOTAL * i) / 3);

const Icon: React.FC<{ index: number; color: string }> = ({ index, color }) => {
  const common = { fill: "none", stroke: color, strokeWidth: 2.4, strokeLinecap: "round" as const };
  if (index === 0) return <div style={{ fontFamily: MONO, fontSize: 26, color, letterSpacing: "0.02em" }}>F9</div>;
  if (index === 1)
    return (
      <svg width={44} height={44} viewBox="0 0 44 44">
        {[6, 12, 18, 24, 30, 36].map((x, i) => (
          <line key={x} x1={x + 1} x2={x + 1} y1={22 - [5, 11, 16, 9, 13, 4][i]} y2={22 + [5, 11, 16, 9, 13, 4][i]} {...common} />
        ))}
      </svg>
    );
  if (index === 2)
    return (
      <svg width={44} height={44} viewBox="0 0 44 44">
        <rect x={10} y={9} width={24} height={29} rx={4} {...common} />
        <rect x={16} y={5} width={12} height={8} rx={2} {...common} />
        <line x1={16} y1={21} x2={28} y2={21} {...common} />
        <line x1={16} y1={28} x2={25} y2={28} {...common} />
      </svg>
    );
  return (
    <svg width={44} height={44} viewBox="0 0 44 44">
      <path d="M22 6 L25 18 L37 22 L25 26 L22 38 L19 26 L7 22 L19 18 Z" {...common} strokeLinejoin="round" />
      <path d="M35 6 L36 10 L40 11 L36 12 L35 16 L34 12 L30 11 L34 10 Z" fill={color} />
    </svg>
  );
};

const Node: React.FC<{ index: number; frame: number }> = ({ index, frame }) => {
  const node = NODES[index];
  const on = BEATS[index];
  const next = BEATS[index + 1] ?? LENGTH;
  const reached = frame >= on;
  const current = reached && frame < next;
  const appear = ramp(frame, 3 + index * 3, 13 + index * 3);
  const ripple = ramp(frame, on, on + 24, easeOut);
  const bump = 1 + 0.1 * ring((frame - on) / 60, 3, 8);
  const color = reached ? C.ink : C.ink3;
  return (
    <div style={{ position: "absolute", left: node.x - 48, top: node.y - 48, opacity: appear }}>
      {reached && ripple < 1 ? (
        <div
          style={{
            position: "absolute",
            left: -56 * ripple,
            top: -56 * ripple,
            width: 96 + 112 * ripple,
            height: 96 + 112 * ripple,
            borderRadius: 24 + 36 * ripple,
            border: `1.5px solid rgba(233,27,39,${0.7 * (1 - ripple)})`,
          }}
        />
      ) : null}
      <div
        style={{
          width: 96,
          height: 96,
          borderRadius: 24,
          background: "#0c0c0c",
          border: `1.5px solid ${current ? C.red : reached ? "rgba(245,245,245,0.3)" : "rgba(245,245,245,0.12)"}`,
          boxShadow: current ? "0 0 30px rgba(233,27,39,0.35)" : "none",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          transform: `scale(${bump})`,
        }}
      >
        <Icon index={index} color={color} />
      </div>
      {reached && !current ? (
        <svg width={22} height={22} viewBox="0 0 22 22" style={{ position: "absolute", left: 84, top: -8 }}>
          <circle cx={11} cy={11} r={10} fill="#0c0c0c" stroke="rgba(245,245,245,0.35)" strokeWidth={1.2} />
          <path d="M6.5 11.5 L9.5 14.5 L15.5 8" fill="none" stroke={C.ink} strokeWidth={1.8} strokeLinecap="round" />
        </svg>
      ) : null}
      <div style={{ position: "absolute", left: 48, top: 116, transform: "translateX(-50%)", textAlign: "center" }}>
        <div style={{ ...display(T.xs, 700), letterSpacing: "-0.02em", color: reached ? C.ink : C.ink2 }}>{node.name}</div>
        <div style={{ ...mono(15, LABEL), marginTop: 12 }}>{node.sub}</div>
      </div>
    </div>
  );
};

const Track: React.FC<{ frame: number }> = ({ frame }) => {
  let s = 0;
  for (let i = 0; i < 3; i++) {
    const t = ramp(frame, BEATS[i] + 3, BEATS[i + 1], easeInOut);
    if (frame >= BEATS[i] + 3) s = STOPS[i] + (STOPS[i + 1] - STOPS[i]) * t;
  }
  const draw = ramp(frame, 0, 16);
  const head = getPointAtLength(PATH, Math.max(0.01, Math.min(TOTAL - 0.01, s))) ?? { x: NODES[0].x, y: NODES[0].y };
  const moving = BEATS.some((b, i) => i < 3 && frame > b + 3 && frame < BEATS[i + 1]);
  const fixed = Math.min(s, FIXED_LENGTH);
  const demand = Math.max(0, s - FIXED_LENGTH);
  return (
    <svg width={1920} height={1080} style={{ position: "absolute", inset: 0 }}>
      <defs>
        <filter id="pipe-glow" x="-50%" y="-50%" width="200%" height="200%">
          <feGaussianBlur stdDeviation="5" />
        </filter>
      </defs>
      <path d={PATH} fill="none" stroke="rgba(245,245,245,0.2)" strokeWidth={2} strokeDasharray="2 9" strokeLinecap="round" opacity={draw} />
      <path d={FIXED} fill="none" stroke={C.red} strokeWidth={2} strokeDasharray={`${fixed} ${TOTAL}`} />
      {demand > 0 ? (
        <path
          d={ON_DEMAND}
          fill="none"
          stroke={C.red}
          strokeWidth={2}
          strokeDasharray="7 9"
          style={{ clipPath: `inset(0 ${Math.max(0, 1920 - head.x - 2)}px 0 0)` }}
        />
      ) : null}
      {moving ? (
        <>
          <circle cx={head.x} cy={head.y} r={13} fill="rgba(233,27,39,0.45)" filter="url(#pipe-glow)" />
          <circle cx={head.x} cy={head.y} r={5} fill="#fff" />
        </>
      ) : null}
    </svg>
  );
};

// Diagram frames, not app windows: a hairline box with corner ticks and plain-text state.
const Frame: React.FC<{ x: number; w: number; title: string; state: string; live?: boolean; frame: number; delay: number; children: React.ReactNode }> = ({
  x,
  w,
  title,
  state,
  live,
  frame,
  delay,
  children,
}) => {
  const appear = ramp(frame, delay, delay + 12);
  const tick = (l: boolean, t: boolean): React.CSSProperties => ({
    position: "absolute",
    [l ? "left" : "right"]: -1,
    [t ? "top" : "bottom"]: -1,
    width: 12,
    height: 12,
    borderColor: "rgba(245,245,245,0.55)",
    borderStyle: "solid",
    borderWidth: 0,
    [t ? "borderTopWidth" : "borderBottomWidth"]: 1.5,
    [l ? "borderLeftWidth" : "borderRightWidth"]: 1.5,
  });
  return (
    <div
      style={{
        position: "absolute",
        left: x,
        top: 176,
        width: w,
        height: 188,
        border: "1px solid rgba(245,245,245,0.1)",
        opacity: appear,
        transform: `translateY(${(1 - appear) * 20}px)`,
      }}
    >
      <div style={tick(true, true)} />
      <div style={tick(false, true)} />
      <div style={tick(true, false)} />
      <div style={tick(false, false)} />
      <div style={{ display: "flex", justifyContent: "space-between", padding: "18px 22px 0" }}>
        <div style={mono(15, LABEL)}>{title}</div>
        <div style={{ ...mono(15, live ? C.ink : LABEL), display: "flex", alignItems: "center", gap: 10 }}>
          {live ? <span style={{ width: 7, height: 7, borderRadius: 4, background: C.red, display: "inline-block" }} /> : null}
          {state}
        </div>
      </div>
      <div style={{ padding: "0 22px" }}>{children}</div>
    </div>
  );
};

// The transcript is a crop of the real widget frame, revealed line by line.
const TEXT_FRAME = "showreel/widget/0150.png";
const CROP = { x: 94, y: 280, w: 1450, h: 125, scale: 0.4 };
const LINES = [
  { y0: 0, y1: 58, at: 8 },
  { y0: 62, y1: 125, at: 24 },
];

const Transcript: React.FC<{ frame: number }> = ({ frame }) => (
  <div style={{ position: "relative", width: CROP.w * CROP.scale, height: CROP.h * CROP.scale, marginTop: 34 }}>
    {LINES.map((line, i) => {
      const reveal = ramp(frame, line.at, line.at + 12, (t) => t);
      return (
        <div
          key={i}
          style={{
            position: "absolute",
            left: 0,
            top: line.y0 * CROP.scale,
            width: CROP.w * CROP.scale,
            height: (line.y1 - line.y0) * CROP.scale,
            overflow: "hidden",
            clipPath: `inset(0 ${(1 - reveal) * 100}% 0 0)`,
          }}
        >
          <Img
            src={staticFile(TEXT_FRAME)}
            style={{
              position: "absolute",
              left: -CROP.x * CROP.scale,
              top: -(CROP.y + line.y0) * CROP.scale,
              width: 1644 * CROP.scale,
              height: 704 * CROP.scale,
              maxWidth: "none",
            }}
          />
        </div>
      );
    })}
  </div>
);

const Frames: React.FC<{ frame: number }> = ({ frame }) => {
  const copied = frame >= BEATS[2];
  const polished = frame >= BEATS[3];
  return (
    <>
      <Frame x={128} w={420} title="DICTATION" state={copied ? "DONE" : "REC"} live={!copied} frame={frame} delay={2}>
        <div style={{ display: "flex", alignItems: "center", gap: 5, height: 86, marginTop: 10 }}>
          {Array.from({ length: 34 }, (_, i) => {
            const n = 0.5 + 0.5 * noise2D("lvl", i * 0.4, frame * 0.18);
            const h = copied ? 4 : 6 + 64 * n * (0.45 + 0.55 * beatPulse(frame + B0, 5));
            return <div key={i} style={{ width: 5, height: h, borderRadius: 3, background: "rgba(245,245,245,0.7)" }} />;
          })}
        </div>
        <div style={{ ...mono(15, LABEL), marginTop: 12 }}>16 KHZ · 16-BIT · MONO</div>
      </Frame>
      <Frame x={600} w={640} title="TRANSCRIPT" state={copied ? "FINAL" : "LIVE"} live={!copied} frame={frame} delay={5}>
        <Transcript frame={frame} />
      </Frame>
      <Frame x={1292} w={500} title="CLIPBOARD" state={copied ? "COPIED" : "EMPTY"} frame={frame} delay={8}>
        <div style={{ ...mono(24, copied ? C.ink : C.ink3), letterSpacing: "0.12em", marginTop: 40 }}>
          {polished ? "POLISHED DRAFT" : copied ? "ORIGINAL TEXT" : "—"}
        </div>
        <div style={{ ...mono(15, LABEL), marginTop: 28 }}>PASTE WITH CTRL+V</div>
      </Frame>
    </>
  );
};

const BigWord: React.FC<{ frame: number }> = ({ frame }) => {
  let index = 0;
  BEATS.forEach((b, i) => {
    if (frame >= b) index = i;
  });
  const local = frame - BEATS[index];
  const next = BEATS[index + 1] ?? LENGTH;
  const inT = ramp(local, 0, 7, easeOut);
  const outT = index < 3 ? ramp(frame, next - 5, next, easeIn) : 0;
  const y = (1 - inT) * 70 - outT * 70;
  const blur = 30 * (1 - inT) + 30 * outT;
  return (
    <div style={{ position: "absolute", left: 128, top: 790 }}>
      <div style={{ ...mono(15, LABEL), display: "flex", gap: 36 }}>
        <span>
          <span style={{ display: "inline-block", width: 9, height: 9, background: C.red, marginRight: 14 }} />
          VOICE TO TEXT
        </span>
        <span>
          <span style={{ color: C.ink }}>0{index + 1}</span> / 04
        </span>
      </div>
      <div style={{ height: 124, overflow: "hidden", marginTop: 12 }}>
        <div style={{ display: "flex", alignItems: "baseline", transform: `translateY(${y}px)`, filter: vBlur(blur), opacity: 1 - outT }}>
          <span style={{ ...display(T.m), textShadow: BLOOM }}>{NODES[index].word}</span>
          <MarkPeriod fontSize={T.m} local={local} at={3} />
        </div>
      </div>
    </div>
  );
};

export const C03Pipeline: React.FC = () => {
  const frame = useCurrentFrame();
  const enter = ramp(frame, 0, 9, easeOut);
  const push = ramp(frame, LENGTH - 6, LENGTH, easeIn);
  return (
    <AbsoluteFill
      style={{
        transform: `translateX(${900 * (1 - enter)}px) scale(${1 + 0.35 * push})`,
        transformOrigin: "1250px 540px",
        filter: enter < 1 ? hBlur(60 * (1 - enter)) : push > 0 ? `blur(${push * 10}px)` : undefined,
        opacity: 1 - push,
      }}
    >
      <Frames frame={frame} />
      <Track frame={frame} />
      {NODES.map((_, i) => (
        <Node key={i} index={i} frame={frame} />
      ))}
      <BigWord frame={frame} />
    </AbsoluteFill>
  );
};
