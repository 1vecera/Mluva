import { AbsoluteFill, useCurrentFrame } from "remotion";
import { beatFrame } from "../timing";
import { C, display, easeIn, easeOut, mono, ramp, ring } from "../theme";
import { Mark } from "../parts/Logo";
import { CORE_BOX } from "./C05Logo";
import { beatPulse } from "../parts/Background";
import { scramble } from "../parts/Scramble";

const B0 = beatFrame(20);
const LENGTH = beatFrame(24) - B0;
const OX = CORE_BOX.x + CORE_BOX.w / 2;
const OY = CORE_BOX.y + CORE_BOX.h / 2;
const INNER = 160;
const OUTER = 320;
const GOLDEN = Math.PI * (3 - Math.sqrt(5));
const DOTS = Array.from({ length: 760 }, (_, i) => {
  const y = 1 - (2 * (i + 0.5)) / 760;
  const r = Math.sqrt(1 - y * y);
  return { x: Math.cos(i * GOLDEN) * r, y, z: Math.sin(i * GOLDEN) * r };
});

// Provider names from docs/provider-selection.md.
const SATELLITES = [
  { label: "LOCAL MODEL", tag: "ON DEVICE · SPEECH", radius: INNER, angle: 70, at: 12 },
  { label: "ELEVENLABS SCRIBE V2", tag: "CLOUD · SPEECH", radius: OUTER, angle: -62, at: 26 },
  { label: "COMPATIBLE API", tag: "YOUR SERVER · SPEECH + REWRITE", radius: OUTER, angle: 150, at: 40 },
  { label: "CODEX", tag: "REWRITE", radius: OUTER, angle: -5, at: 54 },
];

const Cloud: React.FC<{ frame: number }> = ({ frame }) => {
  const appear = ramp(frame, 0, 20, easeOut);
  const spin = frame * 0.012 + 0.6;
  const tilt = 0.36;
  return (
    <svg width={1920} height={1080} style={{ position: "absolute", inset: 0 }}>
      {DOTS.map((p, i) => {
        const x1 = p.x * Math.cos(spin) + p.z * Math.sin(spin);
        const z1 = -p.x * Math.sin(spin) + p.z * Math.cos(spin);
        const y2 = p.y * Math.cos(tilt) - z1 * Math.sin(tilt);
        const z2 = p.y * Math.sin(tilt) + z1 * Math.cos(tilt);
        const depth = (z2 + 1) / 2;
        const r = OUTER * (0.85 + 0.15 * appear);
        return (
          <circle
            key={i}
            cx={OX + x1 * r}
            cy={OY + y2 * r}
            r={0.7 + 1.6 * depth}
            fill={depth > 0.93 && i % 9 === 0 ? C.red : C.ink}
            opacity={appear * (0.08 + 0.55 * depth * depth)}
          />
        );
      })}
      <circle
        cx={OX}
        cy={OY}
        r={INNER}
        fill="none"
        stroke="rgba(245,245,245,0.3)"
        strokeWidth={1.2}
        strokeDasharray="3 9"
        transform={`rotate(${frame * 0.4} ${OX} ${OY})`}
        opacity={ramp(frame, 4, 16)}
      />
      <circle cx={OX} cy={OY} r={OUTER} fill="none" stroke="rgba(245,245,245,0.1)" strokeWidth={1} opacity={appear} />
    </svg>
  );
};

const Satellite: React.FC<{ index: number; frame: number }> = ({ index, frame }) => {
  const s = SATELLITES[index];
  const a = (s.angle * Math.PI) / 180;
  const x = OX + Math.cos(a) * s.radius;
  const y = OY + Math.sin(a) * s.radius;
  const draw = ramp(frame, s.at, s.at + 12, easeOut);
  const label = ramp(frame, s.at + 6, s.at + 16);
  if (draw <= 0) return null;
  const bend = 0.22 * (index % 2 === 0 ? 1 : -1);
  const mx = (OX + x) / 2 - (y - OY) * bend;
  const my = (OY + y) / 2 + (x - OX) * bend;
  const path = `M ${OX} ${OY} Q ${mx} ${my} ${x} ${y}`;
  const pulseT = ((frame - s.at) % 40) / 40;
  const px = (1 - pulseT) ** 2 * OX + 2 * (1 - pulseT) * pulseT * mx + pulseT ** 2 * x;
  const py = (1 - pulseT) ** 2 * OY + 2 * (1 - pulseT) * pulseT * my + pulseT ** 2 * y;
  const right = Math.cos(a) >= 0;
  const pop = 1 + 0.5 * ring((frame - s.at - 10) / 60, 3, 6);
  return (
    <>
      <svg width={1920} height={1080} style={{ position: "absolute", inset: 0 }}>
        <path d={path} fill="none" stroke={C.red} strokeWidth={1.6} pathLength={1} strokeDasharray={`${draw} 1`} opacity={0.85} />
        <circle cx={x} cy={y} r={5 * pop * draw} fill={C.red} />
        <circle cx={x} cy={y} r={14 * draw} fill="none" stroke="rgba(233,27,39,0.5)" strokeWidth={1} />
        {draw >= 1 ? <circle cx={px} cy={py} r={3} fill="#fff" opacity={0.9} /> : null}
      </svg>
      <div
        style={{
          position: "absolute",
          top: y - 26,
          ...(right ? { left: x + 28 } : { right: 1920 - x + 28 }),
          textAlign: right ? "left" : "right",
          opacity: label,
          padding: "10px 14px",
          background: "rgba(10,10,10,0.78)",
          border: "1px solid rgba(245,245,245,0.12)",
          borderRadius: 8,
        }}
      >
        <div style={{ ...mono(14, C.ink), letterSpacing: "0.16em" }}>{scramble(s.label, label, s.label, frame)}</div>
        <div style={{ ...mono(11, C.ink3), marginTop: 6 }}>{s.tag}</div>
      </div>
    </>
  );
};

const Headline: React.FC<{ frame: number }> = ({ frame }) => {
  const l1 = ramp(frame, 2, 16, easeOut);
  const l2 = ramp(frame, 8, 22, easeOut);
  const sub = ramp(frame, 16, 28);
  const stats = ramp(frame, 22, 34);
  const out = ramp(frame, LENGTH - 8, LENGTH, easeIn);
  const bars = 26;
  return (
    <div style={{ position: "absolute", left: 128, top: 300, opacity: 1 - out, transform: `translateX(${-out * 80}px)` }}>
      <div style={{ overflow: "hidden", height: 124 }}>
        <div style={{ ...display(108), transform: `translateY(${(1 - l1) * 124}px)`, textShadow: "0 0 30px rgba(255,255,255,0.16)" }}>
          Local or cloud.
        </div>
      </div>
      <div style={{ overflow: "hidden", height: 136 }}>
        <div style={{ ...display(108), transform: `translateY(${(1 - l2) * 136}px)`, color: C.ink2 }}>
          Your <span style={{ color: C.red, textShadow: `0 0 30px ${C.redGlow}` }}>engines.</span>
        </div>
      </div>
      <div style={{ ...mono(15, C.ink2), marginTop: 26, opacity: sub, display: "flex", alignItems: "center", gap: 16 }}>
        <span style={{ width: 44, height: 1.5, background: C.red, display: "inline-block" }} />
        SPEECH AND REWRITING, CHOSEN SEPARATELY
      </div>
      <div style={{ display: "flex", gap: 64, marginTop: 54, opacity: stats }}>
        {[
          { label: "SPEECH", value: 3 },
          { label: "REWRITE", value: 2 },
        ].map((s) => (
          <div key={s.label}>
            <div style={mono(12, C.ink3)}>
              <span style={{ color: C.red }}>● </span>
              {s.label}
            </div>
            <div style={{ ...display(64, 400), fontFamily: "'JetBrains Mono', monospace", letterSpacing: 0, marginTop: 10 }}>
              0{Math.round(s.value * ramp(frame, 24, 44, easeOut))}
            </div>
          </div>
        ))}
        <div>
          <div style={mono(12, C.ink3)}>SIGNAL</div>
          <div style={{ display: "flex", alignItems: "flex-end", gap: 4, height: 64, marginTop: 10 }}>
            {Array.from({ length: bars }, (_, i) => {
              const p = beatPulse(frame + B0 - i * 1.5, 6);
              const h = 4 + 56 * p * (0.35 + 0.65 * Math.abs(Math.sin(i * 1.7 + frame * 0.05)));
              return <div key={i} style={{ width: 5, height: h, background: i > bars - 6 ? C.red : "rgba(245,245,245,0.75)" }} />;
            })}
          </div>
        </div>
      </div>
    </div>
  );
};

export const C06Engines: React.FC = () => {
  const frame = useCurrentFrame();
  const zoom = ramp(frame, LENGTH - 8, LENGTH, easeIn);
  const breathe = 1 + 0.04 * beatPulse(frame + B0, 5) * ramp(frame, 0, 14);
  return (
    <AbsoluteFill style={{ transform: `scale(${1 + zoom * 0.25})`, transformOrigin: `${OX}px ${OY}px`, opacity: 1 - zoom * 0.7 }}>
      <Cloud frame={frame} />
      {SATELLITES.map((_, i) => (
        <Satellite key={i} index={i} frame={frame} />
      ))}
      <div style={{ position: "absolute", left: OX - 150, top: OY - 150, width: 300, height: 300, borderRadius: "50%", background: "radial-gradient(circle, rgba(233,27,39,0.3), transparent 65%)" }} />
      <Mark box={CORE_BOX} style={{ transform: `scale(${breathe})` }} />
      <div style={{ ...mono(12, C.ink2), position: "absolute", right: 1920 - OX + 84, top: OY - 8, opacity: ramp(frame, 6, 16) }}>
        YOUR DESKTOP
      </div>
      <Headline frame={frame} />
    </AbsoluteFill>
  );
};
