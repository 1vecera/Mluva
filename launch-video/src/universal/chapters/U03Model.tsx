import { AbsoluteFill, useCurrentFrame } from "remotion";
import { noise2D } from "@remotion/noise";
import { beatFrame } from "../timing";
import { FIELDS, PAIRS } from "../claims";
import { BLOOM, C, display, easeIn, easeInOut, easeOut, LABEL, mono, ramp, ring, T } from "../../showreel/theme";
import { MarkPeriod } from "../../showreel/parts/MarkPeriod";
import { scramble } from "../../showreel/parts/Scramble";

const B0 = beatFrame(16);
const k = (beat: number) => beatFrame(16 + beat) - B0;
const LENGTH = k(8);

const CHIP = { x: 128, w: 380, h: 66 };
const CARD = { x: 650, y: 300, w: 600, h: 430 };
const OUT = { x: 1400, w: 400 };
const rowY = (i: number) => 330 + i * 98;

const tick = (l: boolean, t: boolean): React.CSSProperties => ({
  position: "absolute",
  [l ? "left" : "right"]: -1,
  [t ? "top" : "bottom"]: -1,
  width: 12,
  height: 12,
  borderColor: "rgba(245,245,245,0.6)",
  borderStyle: "solid",
  borderWidth: 0,
  [t ? "borderTopWidth" : "borderBottomWidth"]: 1.5,
  [l ? "borderLeftWidth" : "borderRightWidth"]: 1.5,
});

// Which route is lit: one per beat from beat 4.
const activeAt = (frame: number) => {
  let a = -1;
  PAIRS.forEach((_, i) => {
    if (frame >= k(4 + i)) a = i;
  });
  return a;
};

const Card: React.FC<{ frame: number }> = ({ frame }) => {
  // The seed point from chapter 02 opens into the card.
  const open = ramp(frame, 0, 12, easeOut);
  const active = activeAt(frame);
  const w = CARD.w * (0.02 + 0.98 * open);
  const h = CARD.h * ramp(frame, 4, 16, easeOut, 0.01, 1);
  return (
    <div
      style={{
        position: "absolute",
        left: CARD.x + (CARD.w - w) / 2,
        top: CARD.y + (CARD.h - h) / 2,
        width: w,
        height: h,
        border: "1px solid rgba(245,245,245,0.16)",
        background: "rgba(10,10,10,0.82)",
        boxShadow: active >= 0 ? `0 0 60px rgba(233,27,39,${0.12 + 0.1 * ring((frame - k(4 + active)) / 60, 2, 6)})` : "none",
        overflow: "hidden",
      }}
    >
      <div style={tick(true, true)} />
      <div style={tick(false, true)} />
      <div style={tick(true, false)} />
      <div style={tick(false, false)} />
      <div style={{ display: "flex", justifyContent: "space-between", padding: "22px 30px 0", opacity: ramp(frame, 10, 18) }}>
        <div style={{ ...mono(15, C.ink) }}>TARGET</div>
        <div style={{ ...mono(15, LABEL) }}>ONE MODEL · EVERY AGENT</div>
      </div>
      <div style={{ padding: "26px 30px 0" }}>
        {FIELDS.map((field, i) => {
          const on = k(i);
          const shown = frame >= on;
          const settle = ramp(frame, on, on + 6);
          // Before the routes, each field cycles through its values; afterwards it follows the lit route.
          let value = field.values[Math.floor(Math.max(0, frame - on) / 7) % field.values.length];
          if (active >= 0) {
            const pair = PAIRS[active];
            value = field.key === "KIND" ? pair.kind : field.key === "DELIVERY" ? pair.level : field.key === "STATE" ? pair.state : value;
          }
          const hot = active >= 0 && field.key === "DELIVERY";
          return (
            <div
              key={field.key}
              style={{
                display: "flex",
                alignItems: "baseline",
                height: 78,
                borderTop: "1px solid rgba(245,245,245,0.08)",
                opacity: shown ? 1 : 0.25,
              }}
            >
              <div style={{ ...mono(15, LABEL), width: 150, paddingTop: 28 }}>{field.key}</div>
              <div
                style={{
                  ...display(field.key === "NAME" ? 24 : 34, field.key === "NAME" ? 500 : 800),
                  letterSpacing: field.key === "NAME" ? "0.02em" : "-0.01em",
                  color: hot ? C.ink : shown ? C.ink : C.ink3,
                  paddingTop: 22,
                  textShadow: hot ? BLOOM : undefined,
                }}
              >
                {shown ? scramble(value, settle, `${field.key}-${value}`, frame) : "—"}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
};

const Chip: React.FC<{ i: number; frame: number }> = ({ i, frame }) => {
  const pair = PAIRS[i];
  const appear = ramp(frame, 6 + i * 3, 18 + i * 3, easeOut);
  const on = k(4 + i);
  const lit = frame >= on;
  const current = activeAt(frame) === i;
  const bump = 1 + 0.06 * ring((frame - on) / 60, 3, 8);
  return (
    <div
      style={{
        position: "absolute",
        left: CHIP.x,
        top: rowY(i),
        width: CHIP.w,
        height: CHIP.h,
        border: `1px solid ${current ? C.red : lit ? "rgba(245,245,245,0.32)" : "rgba(245,245,245,0.12)"}`,
        background: current ? "rgba(233,27,39,0.07)" : "rgba(245,245,245,0.02)",
        boxShadow: current ? `0 0 26px rgba(233,27,39,0.3)` : "none",
        opacity: appear,
        transform: `translateX(${(1 - appear) * -50}px) scale(${lit ? bump : 1})`,
        display: "flex",
        flexDirection: "column",
        justifyContent: "center",
        padding: "0 22px",
        boxSizing: "border-box",
      }}
    >
      <div style={{ ...mono(15, lit ? C.ink : C.ink2) }}>{pair.route}</div>
      <div style={{ ...mono(13, LABEL), marginTop: 7, letterSpacing: "0.16em" }}>{pair.where}</div>
    </div>
  );
};

const Outcome: React.FC<{ i: number; frame: number }> = ({ i, frame }) => {
  const pair = PAIRS[i];
  const on = k(4 + i) + 4;
  if (frame < on) return null;
  const inT = ramp(frame, on, on + 8, easeOut);
  const current = activeAt(frame) === i;
  return (
    <div style={{ position: "absolute", left: OUT.x, top: rowY(i) + 2, width: OUT.w, opacity: inT, transform: `translateX(${(1 - inT) * 40}px)` }}>
      <div style={{ ...display(34, 800), letterSpacing: "-0.01em", color: current ? C.ink : C.ink2, textShadow: current ? BLOOM : undefined }}>
        {pair.level}
      </div>
      <div style={{ ...mono(13, LABEL), marginTop: 9, letterSpacing: "0.14em" }}>{pair.line}</div>
    </div>
  );
};

const Wires: React.FC<{ frame: number }> = ({ frame }) => {
  const active = activeAt(frame);
  const draw = ramp(frame, 8, 26);
  return (
    <svg width={1920} height={1080} style={{ position: "absolute", inset: 0 }}>
      <defs>
        <filter id="model-glow" x="-50%" y="-50%" width="200%" height="200%">
          <feGaussianBlur stdDeviation="4" />
        </filter>
      </defs>
      {PAIRS.map((_, i) => {
        const y = rowY(i) + CHIP.h / 2;
        const cy = CARD.y + 120 + i * 70;
        const inPath = `M ${CHIP.x + CHIP.w} ${y} C ${CHIP.x + CHIP.w + 80} ${y} ${CARD.x - 80} ${cy} ${CARD.x} ${cy}`;
        const outPath = `M ${CARD.x + CARD.w} ${cy} C ${CARD.x + CARD.w + 80} ${cy} ${OUT.x - 90} ${y} ${OUT.x - 22} ${y}`;
        const on = k(4 + i);
        const flow = ramp(frame, on, on + 10, easeInOut);
        const current = active === i;
        const lit = frame >= on;
        return (
          <g key={i}>
            <path d={inPath} fill="none" stroke="rgba(245,245,245,0.16)" strokeWidth={1.5} strokeDasharray="2 8" strokeLinecap="round" opacity={draw} />
            <path d={outPath} fill="none" stroke="rgba(245,245,245,0.16)" strokeWidth={1.5} strokeDasharray="2 8" strokeLinecap="round" opacity={draw} />
            {lit ? (
              <>
                <path d={inPath} pathLength={1} fill="none" stroke={current ? C.red : "rgba(245,245,245,0.4)"} strokeWidth={2} strokeDasharray={`${flow} 1`} />
                <path
                  d={outPath}
                  pathLength={1}
                  fill="none"
                  stroke={current ? C.red : "rgba(245,245,245,0.4)"}
                  strokeWidth={2}
                  strokeDasharray={`${ramp(frame, on + 2, on + 12, easeInOut)} 1`}
                />
              </>
            ) : null}
          </g>
        );
      })}
    </svg>
  );
};

const WORDS = ["Discover", "Name", "Track", "Deliver"];
const BigWord: React.FC<{ frame: number }> = ({ frame }) => {
  let index = 0;
  WORDS.forEach((_, i) => {
    if (frame >= k(i)) index = i;
  });
  const local = frame - k(index);
  const next = index < 3 ? k(index + 1) : LENGTH + 99;
  const inT = ramp(local, 0, 7, easeOut);
  const outT = index < 3 ? ramp(frame, next - 5, next, easeIn) : ramp(frame, LENGTH - 8, LENGTH, easeIn);
  const y = (1 - inT) * 110 - outT * 110;
  return (
    <div style={{ position: "absolute", left: 128, top: 800 }}>
      <div style={{ ...mono(15, LABEL), display: "flex", gap: 36 }}>
        <span>
          <span style={{ display: "inline-block", width: 9, height: 9, background: C.red, marginRight: 14 }} />
          AFTER · ONE TARGET MODEL
        </span>
        <span>
          <span style={{ color: C.ink }}>0{index + 1}</span> / 04
        </span>
      </div>
      <div style={{ height: 110, overflow: "hidden", marginTop: 10 }}>
        <div style={{ display: "flex", alignItems: "baseline", transform: `translateY(${y}px)` }}>
          <span style={{ ...display(T.s + 26), textShadow: BLOOM }}>{WORDS[index]}</span>
          <MarkPeriod fontSize={T.s + 26} local={local} at={3} />
        </div>
      </div>
    </div>
  );
};

// A faint drift of grid dots behind the model, the "scratch" it was built on.
const Field: React.FC<{ frame: number }> = ({ frame }) => (
  <svg width={1920} height={1080} style={{ position: "absolute", inset: 0, opacity: ramp(frame, 0, 20) * 0.5 }}>
    {Array.from({ length: 12 }, (_, r) =>
      Array.from({ length: 22 }, (_, c) => {
        const n = noise2D("field", c * 0.3, r * 0.3 + frame * 0.01);
        return <circle key={`${r}-${c}`} cx={60 + c * 86} cy={80 + r * 86} r={1 + 0.6 * n} fill="rgba(245,245,245,0.14)" />;
      }),
    )}
  </svg>
);

export const U03Model: React.FC = () => {
  const frame = useCurrentFrame();
  const out = ramp(frame, LENGTH - 6, LENGTH, easeIn);
  return (
    <AbsoluteFill style={{ opacity: 1 - out, transform: `scale(${1 + 0.04 * out})` }}>
      <Field frame={frame} />
      <Wires frame={frame} />
      {PAIRS.map((_, i) => (
        <Chip key={i} i={i} frame={frame} />
      ))}
      <Card frame={frame} />
      {PAIRS.map((_, i) => (
        <Outcome key={i} i={i} frame={frame} />
      ))}
      <div style={{ ...mono(13, LABEL), position: "absolute", left: OUT.x, top: rowY(4) + 6, letterSpacing: "0.14em", opacity: ramp(frame, k(7) + 10, k(7) + 18) }}>
        NO ROUTE → <span style={{ color: C.ink2 }}>DRAFT ONLY</span> · KEPT IN MLUVA
      </div>
      <BigWord frame={frame} />
    </AbsoluteFill>
  );
};
