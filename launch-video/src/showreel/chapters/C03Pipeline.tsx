import { AbsoluteFill, Img, useCurrentFrame } from "remotion";
import { noise2D } from "@remotion/noise";
import { getLength, getPointAtLength } from "@remotion/paths";
import { beatFrame } from "../timing";
import { brand, C, display, easeIn, easeInOut, easeOut, MONO, mono, ramp, ring } from "../theme";
import { hBlur, vBlur } from "../parts/Blur";
import { beatPulse } from "../parts/Background";

const B0 = beatFrame(8);
const BEATS = [0, 1, 2, 3].map((i) => beatFrame(8 + i) - B0);
const LENGTH = beatFrame(12) - B0;

const NODES = [
  { x: 420, y: 606, name: "Speak", sub: "01 · F9 · MICROPHONE", word: "Spoken" },
  { x: 800, y: 504, name: "Transcribe", sub: "02 · SCRIBE · LOCAL · API", word: "Heard" },
  { x: 1180, y: 606, name: "Copy", sub: "03 · CLIPBOARD", word: "Copied" },
  { x: 1560, y: 504, name: "Rewrite", sub: "04 · POLISH · STRUCTURE", word: "Shaped" },
];

const segment = (a: (typeof NODES)[number], b: (typeof NODES)[number]) => {
  const mx = (a.x + b.x) / 2;
  return `C ${mx} ${a.y} ${mx} ${b.y} ${b.x} ${b.y}`;
};
const PATH = `M ${NODES[0].x} ${NODES[0].y} ${segment(NODES[0], NODES[1])} ${segment(NODES[1], NODES[2])} ${segment(
  NODES[2],
  NODES[3],
)}`;
const TOTAL = getLength(PATH);
const STOPS = NODES.map((_, i) => (TOTAL * i) / 3);

const Icon: React.FC<{ index: number; color: string }> = ({ index, color }) => {
  const common = { fill: "none", stroke: color, strokeWidth: 2.4, strokeLinecap: "round" as const };
  if (index === 0)
    return (
      <div style={{ fontFamily: MONO, fontSize: 26, color, letterSpacing: "0.02em", fontWeight: 400 }}>F9</div>
    );
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
  const active = ramp(frame, on, on + 6);
  const appear = ramp(frame, 3 + index * 3, 13 + index * 3);
  const ripple = ramp(frame, on, on + 26, easeOut);
  const bump = 1 + 0.12 * ring((frame - on) / 60, 3, 7);
  const color = active > 0.5 ? C.ink : C.ink3;
  return (
    <div style={{ position: "absolute", left: node.x - 48, top: node.y - 48, opacity: appear }}>
      {frame >= on ? (
        <div
          style={{
            position: "absolute",
            left: 48 - 48 - 60 * ripple,
            top: 48 - 48 - 60 * ripple,
            width: 96 + 120 * ripple,
            height: 96 + 120 * ripple,
            borderRadius: 24 + 40 * ripple,
            border: `1.5px solid rgba(233,27,39,${0.8 * (1 - ripple)})`,
          }}
        />
      ) : null}
      <div
        style={{
          width: 96,
          height: 96,
          borderRadius: 24,
          background: "linear-gradient(180deg, #171717, #0c0c0c)",
          border: `1.5px solid ${active > 0.5 ? C.red : "rgba(245,245,245,0.14)"}`,
          boxShadow: active > 0.5 ? `0 0 34px rgba(233,27,39,0.45), inset 0 0 18px rgba(233,27,39,0.18)` : "none",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          transform: `scale(${bump})`,
        }}
      >
        <Icon index={index} color={color} />
      </div>
      {active > 0 ? (
        <div
          style={{
            position: "absolute",
            left: 78,
            top: -10,
            width: 26,
            height: 26,
            borderRadius: 13,
            background: C.red,
            transform: `scale(${active + 0.3 * ring((frame - on - 3) / 60, 3, 6)})`,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            boxShadow: `0 0 12px ${C.redGlow}`,
          }}
        >
          <svg width={14} height={14} viewBox="0 0 14 14">
            <path d="M3 7.5 L6 10 L11 4" fill="none" stroke="#fff" strokeWidth={2} strokeLinecap="round" />
          </svg>
        </div>
      ) : null}
      <div style={{ position: "absolute", left: 48, top: 118, transform: "translateX(-50%)", textAlign: "center" }}>
        <div style={{ ...display(34, 700), letterSpacing: "-0.02em", color: active > 0.5 ? C.ink : C.ink2 }}>{node.name}</div>
        <div style={{ ...mono(12, C.ink3), marginTop: 12 }}>{node.sub}</div>
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
  return (
    <svg width={1920} height={1080} style={{ position: "absolute", inset: 0 }}>
      <defs>
        <filter id="pipe-glow" x="-50%" y="-50%" width="200%" height="200%">
          <feGaussianBlur stdDeviation="6" />
        </filter>
      </defs>
      <path d={PATH} fill="none" stroke="rgba(245,245,245,0.22)" strokeWidth={2} strokeDasharray="2 9" strokeLinecap="round" opacity={draw} />
      <path d={PATH} fill="none" stroke={C.red} strokeWidth={2.5} strokeDasharray={`${s} ${TOTAL}`} opacity={0.9} />
      <path d={PATH} fill="none" stroke={C.red} strokeWidth={8} strokeDasharray={`${s} ${TOTAL}`} opacity={0.35} filter="url(#pipe-glow)" />
      {moving ? (
        <>
          <path
            d={PATH}
            fill="none"
            stroke="#fff"
            strokeWidth={3}
            strokeDasharray={`0 ${Math.max(0, s - 110)} 110 ${TOTAL}`}
            opacity={0.9}
            filter="url(#pipe-glow)"
          />
          <circle cx={head.x} cy={head.y} r={16} fill="rgba(233,27,39,0.5)" filter="url(#pipe-glow)" />
          <circle cx={head.x} cy={head.y} r={6} fill="#fff" />
        </>
      ) : null}
    </svg>
  );
};

const Card: React.FC<{ x: number; w: number; title: string; right: React.ReactNode; frame: number; delay: number; children: React.ReactNode }> = ({
  x,
  w,
  title,
  right,
  frame,
  delay,
  children,
}) => {
  const appear = ramp(frame, delay, delay + 12);
  return (
    <div
      style={{
        position: "absolute",
        left: x,
        top: 170,
        width: w,
        height: 200,
        borderRadius: 18,
        background: "linear-gradient(180deg, rgba(24,24,24,0.9), rgba(12,12,12,0.9))",
        border: "1px solid rgba(245,245,245,0.1)",
        boxShadow: "0 30px 60px rgba(0,0,0,0.5)",
        opacity: appear,
        transform: `translateY(${(1 - appear) * 24}px)`,
        overflow: "hidden",
      }}
    >
      <div style={{ display: "flex", justifyContent: "space-between", padding: "20px 24px 0" }}>
        <div style={mono(13, C.ink3)}>{title}</div>
        <div>{right}</div>
      </div>
      <div style={{ padding: "0 24px" }}>{children}</div>
    </div>
  );
};

const LINES = [
  { t: "00:00.4", text: "Quick note for the team:", at: 6 },
  { t: "00:00.9", text: "the release is ready", at: 18 },
  { t: "00:01.3", text: "and we ship on Friday.", at: 30 },
  { t: "00:01.7", text: "Coffee is on me.", at: 42 },
];

const Cards: React.FC<{ frame: number }> = ({ frame }) => {
  const copied = frame >= BEATS[2];
  const rec = Math.floor(frame / 12) % 2 === 0;
  return (
    <>
      <Card
        x={128}
        w={420}
        title="DICTATION"
        frame={frame}
        delay={2}
        right={<span style={mono(13, C.red)}>{rec ? "● " : "  "}REC</span>}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 5, height: 92, marginTop: 8 }}>
          {Array.from({ length: 34 }, (_, i) => {
            const n = 0.5 + 0.5 * noise2D("lvl", i * 0.4, frame * 0.18);
            const h = 6 + 70 * n * (0.45 + 0.55 * beatPulse(frame + 226, 5)) * (copied ? 0.25 : 1);
            return <div key={i} style={{ width: 5, height: h, borderRadius: 3, background: i % 7 === 3 ? C.red : "rgba(245,245,245,0.8)" }} />;
          })}
        </div>
        <div style={{ ...mono(12, C.ink3), marginTop: 18 }}>16 KHZ · 16-BIT · MONO PCM</div>
      </Card>
      <Card x={600} w={640} title="TRANSCRIPT" frame={frame} delay={5} right={<span style={mono(13, C.ink3)}>STREAMING</span>}>
        <div style={{ marginTop: 16 }}>
          {LINES.map((line) => {
            const typed = ramp(frame, line.at, line.at + 10, (t) => t);
            if (typed <= 0) return <div key={line.t} style={{ height: 34 }} />;
            const chars = Math.round(line.text.length * typed);
            return (
              <div key={line.t} style={{ display: "flex", gap: 22, height: 34, alignItems: "center", fontFamily: MONO, fontSize: 18 }}>
                <span style={{ color: C.ink3 }}>{line.t}</span>
                <span style={{ color: C.ink }}>
                  {line.text.slice(0, chars)}
                  {typed < 1 ? <span style={{ color: C.red }}>▌</span> : null}
                </span>
              </div>
            );
          })}
        </div>
      </Card>
      <Card
        x={1292}
        w={500}
        title="CLIPBOARD"
        frame={frame}
        delay={8}
        right={
          <span style={{ ...mono(13, copied ? C.ink : C.ink3), display: "inline-flex", alignItems: "center", gap: 8 }}>
            <span style={{ width: 8, height: 8, borderRadius: 4, background: copied ? C.red : C.ink3, boxShadow: copied ? `0 0 10px ${C.redGlow}` : "none" }} />
            {copied ? "COPIED" : "WAITING"}
          </span>
        }
      >
        <div style={{ ...display(46, 800), marginTop: 30, color: copied ? C.ink : C.ink3, letterSpacing: "-0.02em" }}>
          {copied ? "Ready to paste" : "···"}
        </div>
        <div style={{ ...mono(12, C.ink3), marginTop: 22 }}>CTRL+V · ORIGINAL ALWAYS KEPT</div>
      </Card>
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
  const pop = ramp(local, 3, 11, easeOut) + 0.4 * ring((local - 5) / 60, 2.6, 5);
  return (
    <div style={{ position: "absolute", left: 128, top: 770 }}>
      <div style={{ ...mono(14, C.ink2), display: "flex", gap: 36 }}>
        <span>
          <span style={{ display: "inline-block", width: 9, height: 9, background: C.red, marginRight: 14 }} />
          VOICE TO TEXT
        </span>
        <span style={{ color: C.ink3 }}>
          <span style={{ color: C.red }}>0{index + 1}</span> / 04
        </span>
      </div>
      <div style={{ height: 150, overflow: "hidden", marginTop: 14 }}>
        <div
          style={{
            display: "flex",
            alignItems: "baseline",
            transform: `translateY(${y}px)`,
            filter: vBlur(blur),
            opacity: 1 - outT,
          }}
        >
          <span style={{ ...display(128), textShadow: "0 0 30px rgba(255,255,255,0.18)" }}>{NODES[index].word}</span>
          <Img
            src={brand("mluva-mark.svg")}
            style={{ width: 44, height: 44, marginLeft: 6, transform: `translateY(12px) scale(${Math.max(0, pop)})` }}
          />
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
        transformOrigin: "1260px 540px",
        filter: enter < 1 ? hBlur(60 * (1 - enter)) : push > 0 ? `blur(${push * 10}px)` : "none",
        opacity: 1 - push,
      }}
    >
      <Cards frame={frame} />
      <Track frame={frame} />
      {NODES.map((_, i) => (
        <Node key={i} index={i} frame={frame} />
      ))}
      <BigWord frame={frame} />
    </AbsoluteFill>
  );
};
