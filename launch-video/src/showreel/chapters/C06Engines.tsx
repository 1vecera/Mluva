import { AbsoluteFill, useCurrentFrame } from "remotion";
import { beatFrame } from "../timing";
import { BLOOM, C, display, easeIn, easeOut, LABEL, MONO, mono, ramp, ring, T } from "../theme";
import { Mark } from "../parts/Logo";
import { CORE_BOX } from "./C05Logo";
import { scramble } from "../parts/Scramble";

const B0 = beatFrame(20);
const LENGTH = beatFrame(24) - B0;
const BEATS = [1, 2, 3].map((i) => beatFrame(20 + i) - B0);
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

// Provider names from docs/provider-selection.md; maturity from docs/feature-maturity.md.
// Each link lands on a beat: frames 14, 28, 56 and 84 of the chapter.
const SATELLITES = [
  { label: "ELEVENLABS SCRIBE V2", tag: "CLOUD · SPEECH", radius: OUTER, angle: -62, at: 6, place: "right" },
  { label: "LOCAL MODEL", tag: "ON DEVICE · SPEECH", radius: INNER, angle: 70, at: 20, place: "right", experimental: true },
  { label: "COMPATIBLE API", tag: "YOUR SERVER · SPEECH + REWRITE", radius: OUTER, angle: 150, at: 48, place: "below", experimental: true },
  { label: "CODEX", tag: "REWRITE", radius: OUTER, angle: -5, at: 76, place: "right" },
] as const;

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
        return <circle key={i} cx={OX + x1 * r} cy={OY + y2 * r} r={0.7 + 1.5 * depth} fill={C.ink} opacity={appear * (0.07 + 0.5 * depth * depth)} />;
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
  const draw = ramp(frame, s.at, s.at + 8, easeOut);
  const label = ramp(frame, s.at + 6, s.at + 12);
  if (draw <= 0) return null;
  const bend = 0.22 * (index % 2 === 0 ? 1 : -1);
  const mx = (OX + x) / 2 - (y - OY) * bend;
  const my = (OY + y) / 2 + (x - OX) * bend;
  const path = `M ${OX} ${OY} Q ${mx} ${my} ${x} ${y}`;
  const flowing = frame >= s.at + 10;
  const phase = (((frame - s.at - 10) % 14) + 14) % 14 / 14;
  const px = (1 - phase) ** 2 * OX + 2 * (1 - phase) * phase * mx + phase ** 2 * x;
  const py = (1 - phase) ** 2 * OY + 2 * (1 - phase) * phase * my + phase ** 2 * y;
  const pop = 1 + 0.5 * ring((frame - s.at - 8) / 60, 3, 7);
  const box: React.CSSProperties =
    s.place === "below" ? { left: x - 24, top: y + 26 } : { left: x + 28, top: y - 30 };
  return (
    <>
      <svg width={1920} height={1080} style={{ position: "absolute", inset: 0 }}>
        <path d={path} fill="none" stroke={C.red} strokeWidth={1.6} pathLength={1} strokeDasharray={`${draw} 1`} opacity={0.85} />
        <circle cx={x} cy={y} r={5 * pop * draw} fill={C.red} />
        <circle cx={x} cy={y} r={14 * draw} fill="none" stroke="rgba(233,27,39,0.45)" strokeWidth={1} />
        {flowing ? <circle cx={px} cy={py} r={3} fill="#fff" opacity={0.9} /> : null}
      </svg>
      <div
        style={{
          position: "absolute",
          ...box,
          opacity: label,
          padding: "12px 16px",
          background: "rgba(8,8,8,0.82)",
          border: "1px solid rgba(245,245,245,0.12)",
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 14 }}>
          <div style={{ ...mono(15, C.ink), letterSpacing: "0.16em" }}>{scramble(s.label, ramp(frame, s.at + 6, s.at + 10), s.label, frame)}</div>
          {"experimental" in s && s.experimental ? (
            <div style={{ ...mono(13, C.ink2), letterSpacing: "0.14em", border: "1px solid rgba(245,245,245,0.3)", padding: "3px 7px" }}>
              EXPERIMENTAL
            </div>
          ) : null}
        </div>
        <div style={{ ...mono(15, LABEL), marginTop: 8, letterSpacing: "0.14em" }}>{s.tag}</div>
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
  return (
    <div style={{ position: "absolute", left: 128, top: 318, opacity: 1 - out, transform: `translateX(${-out * 80}px)` }}>
      <div style={{ overflow: "hidden", height: 118 }}>
        <div style={{ ...display(T.m), transform: `translateY(${(1 - l1) * 118}px)`, textShadow: BLOOM }}>Local or cloud.</div>
      </div>
      <div style={{ overflow: "hidden", height: 130 }}>
        <div style={{ ...display(T.m), transform: `translateY(${(1 - l2) * 130}px)`, color: C.ink2 }}>
          Your <span style={{ color: C.red }}>engines.</span>
        </div>
      </div>
      <div style={{ ...mono(15, LABEL), marginTop: 26, opacity: sub, display: "flex", alignItems: "center", gap: 16 }}>
        <span style={{ width: 44, height: 1.5, background: C.red, display: "inline-block" }} />
        SPEECH AND REWRITING, CHOSEN SEPARATELY
      </div>
      <div style={{ display: "flex", gap: 72, marginTop: 50, opacity: stats }}>
        {[
          { label: "SPEECH", value: 3 },
          { label: "REWRITE", value: 2 },
        ].map((s) => (
          <div key={s.label}>
            <div style={mono(15, LABEL)}>{s.label}</div>
            <div style={{ fontFamily: MONO, fontSize: T.s, color: C.ink, marginTop: 10, lineHeight: 1 }}>
              0{Math.round(s.value * ramp(frame, 24, 44, easeOut))}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
};

// A kick on every beat after the mark has landed: 6 frames up, 18 frames down.
const corePulse = (frame: number) => {
  let v = 0;
  for (const b of BEATS) {
    const dt = frame - b;
    if (dt >= 0) v = dt < 6 ? dt / 6 : Math.max(0, 1 - (dt - 6) / 18);
  }
  return v;
};

export const C06Engines: React.FC = () => {
  const frame = useCurrentFrame();
  const zoom = ramp(frame, LENGTH - 8, LENGTH, easeIn);
  const pulse = 1 + 0.04 * corePulse(frame);
  return (
    <AbsoluteFill style={{ transform: `scale(${1 + zoom * 0.25})`, transformOrigin: `${OX}px ${OY}px`, opacity: 1 - zoom * 0.7 }}>
      <Cloud frame={frame} />
      {SATELLITES.map((_, i) => (
        <Satellite key={i} index={i} frame={frame} />
      ))}
      <div style={{ position: "absolute", left: OX - 150, top: OY - 150, width: 300, height: 300, borderRadius: "50%", background: "radial-gradient(circle, rgba(233,27,39,0.24), transparent 65%)" }} />
      <Mark box={CORE_BOX} style={{ transform: `scale(${pulse})` }} />
      <div style={{ ...mono(15, LABEL), position: "absolute", right: 1920 - OX + 84, top: OY - 9, opacity: ramp(frame, 6, 16) }}>YOUR DESKTOP</div>
      <Headline frame={frame} />
    </AbsoluteFill>
  );
};
