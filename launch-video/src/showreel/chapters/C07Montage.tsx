import { AbsoluteFill, random, useCurrentFrame } from "remotion";
import { beatFrame } from "../timing";
import { C, display, DISPLAY, easeOut, MONO, mono, ramp, ring } from "../theme";
import { Keycap } from "../parts/Keycap";
import { scramble } from "../parts/Scramble";

const B0 = beatFrame(24);
export const CUTS = Array.from({ length: 9 }, (_, k) => beatFrame(24 + k / 2) - B0);

const Punch: React.FC<{ f: number; children: React.ReactNode; bg?: string }> = ({ f, children, bg }) => {
  const inT = ramp(f, 0, 4, easeOut);
  return (
    <AbsoluteFill style={{ background: bg }}>
      <AbsoluteFill style={{ transform: `scale(${1.1 - 0.1 * inT + 0.012 * f})`, filter: `blur(${(1 - inT) * 8}px)` }}>
        {children}
      </AbsoluteFill>
    </AbsoluteFill>
  );
};

const PressF9: React.FC<{ f: number }> = ({ f }) => {
  const press = f >= 2 && f < 8 ? Math.sin(((f - 2) / 6) * Math.PI) : 0;
  const wave = ramp(f, 3, 13, easeOut);
  return (
    <Punch f={f}>
      <div style={{ position: "absolute", left: 960 - 170 - 220 * wave, top: 540 - 170 - 220 * wave, width: 340 + 440 * wave, height: 340 + 440 * wave, borderRadius: "50%", border: `2px solid rgba(233,27,39,${0.8 * (1 - wave)})` }} />
      <AbsoluteFill style={{ display: "flex", flexDirection: "row", alignItems: "center", justifyContent: "center", gap: 64 }}>
        <div style={{ ...display(210), textShadow: "0 0 40px rgba(255,255,255,0.2)" }}>PRESS</div>
        <div style={{ transform: `rotate(-4deg) translateY(${-18 + 30 * press}px)` }}>
          <Keycap label="F9" size={250} press={press} lit={Math.max(press, f > 2 ? 0.6 : 0)} />
        </div>
      </AbsoluteFill>
    </Punch>
  );
};

const LiveRewrite: React.FC<{ f: number }> = ({ f }) => {
  const text = scramble("LIVE REWRITE", ramp(f, 0, 9), "live", f * 3, 0.5);
  const line = ramp(f, 3, 11, easeOut);
  return (
    <Punch f={f}>
      <AbsoluteFill style={{ backgroundImage: "repeating-linear-gradient(0deg, rgba(245,245,245,0.03) 0 1px, transparent 1px 6px)" }} />
      <AbsoluteFill style={{ display: "flex", alignItems: "center", justifyContent: "center" }}>
        <div style={{ position: "relative" }}>
          <div style={{ ...display(196), textShadow: "0 0 40px rgba(255,255,255,0.2)", minWidth: 1320, textAlign: "center" }}>{text}</div>
          <div style={{ position: "absolute", left: 0, bottom: -26, height: 8, width: `${line * 100}%`, background: C.red, boxShadow: `0 0 24px ${C.redGlow}` }} />
          <div style={{ ...mono(18, C.red), position: "absolute", right: 0, top: -54, border: `1.5px solid ${C.red}`, padding: "8px 14px", opacity: ramp(f, 4, 7) }}>
            EXPERIMENTAL
          </div>
        </div>
      </AbsoluteFill>
    </Punch>
  );
};

const PolishStructure: React.FC<{ f: number }> = ({ f }) => {
  const rows = 9;
  return (
    <Punch f={f}>
      <AbsoluteFill style={{ display: "flex", flexDirection: "column", justifyContent: "center", gap: 0 }}>
        {Array.from({ length: rows }, (_, r) => {
          const center = r === Math.floor(rows / 2);
          const dir = r % 2 === 0 ? 1 : -1;
          const x = -400 + dir * f * 9 - (r * 173) % 400;
          return (
            <div key={r} style={{ ...display(118), whiteSpace: "nowrap", transform: `translateX(${x}px)`, lineHeight: "116px", height: 116 }}>
              {Array.from({ length: 5 }, (_, i) => (
                <span key={i} style={{ marginRight: 40 }}>
                  <span style={center ? { color: C.ink } : { color: "transparent", WebkitTextStroke: "1.5px rgba(245,245,245,0.16)" }}>POLISH</span>
                  <span style={{ marginRight: 40 }} />
                  <span style={center ? { color: C.red, textShadow: `0 0 30px ${C.redGlow}` } : { color: "transparent", WebkitTextStroke: "1.5px rgba(245,245,245,0.16)" }}>
                    STRUCTURE
                  </span>
                </span>
              ))}
            </div>
          );
        })}
      </AbsoluteFill>
    </Punch>
  );
};

const History: React.FC<{ f: number }> = ({ f }) => {
  const words = ["SEARCH", "RENAME", "MERGE", "EXPORT"];
  return (
    <Punch f={f}>
      <AbsoluteFill style={{ transform: "rotate(-11deg) scale(1.25)", display: "flex", flexDirection: "column", justifyContent: "center" }}>
        {Array.from({ length: 8 }, (_, r) => {
          const dir = r % 2 === 0 ? -1 : 1;
          const tone = r === 3 ? C.ink : r === 5 ? C.red : r % 2 === 0 ? "rgba(245,245,245,0.35)" : "rgba(245,245,245,0.14)";
          return (
            <div key={r} style={{ ...display(96, 900), color: tone, whiteSpace: "nowrap", transform: `translateX(${-600 + dir * f * 14 - r * 90}px)`, height: 104, lineHeight: "104px" }}>
              {Array.from({ length: 6 }, (_, i) => words.map((w) => `${w} · `).join("") + (r === 3 && i === 1 ? "YOUR HISTORY · " : "")).join("")}
            </div>
          );
        })}
      </AbsoluteFill>
    </Punch>
  );
};

const Incognito: React.FC<{ f: number }> = ({ f }) => {
  const bar = ramp(f, 3, 10, easeOut);
  return (
    <Punch f={f} bg={C.red}>
      <AbsoluteFill style={{ display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center" }}>
        <svg width={96} height={96} viewBox="0 0 48 48" style={{ marginBottom: 20 }}>
          <path d="M4 24 C10 13 17 9 24 9 C31 9 38 13 44 24 C38 35 31 39 24 39 C17 39 10 35 4 24 Z" fill="none" stroke="#000" strokeWidth={3.4} />
          <circle cx={24} cy={24} r={6.5} fill="#000" />
          <line x1={8} y1={40} x2={40} y2={8} stroke="#000" strokeWidth={3.8} strokeLinecap="round" />
        </svg>
        <div style={{ position: "relative" }}>
          <div style={{ ...display(236), color: "#000" }}>INCOGNITO</div>
          <div style={{ position: "absolute", left: 0, bottom: -34, height: 16, width: `${bar * 100}%`, background: "#000" }} />
        </div>
        <div style={{ ...mono(20, "#000"), marginTop: 64, letterSpacing: "0.24em" }}>NO HISTORY · NO RECOVERY AUDIO</div>
      </AbsoluteFill>
    </Punch>
  );
};

const FLAP_ROWS = ["CTRL + P", "ACTIONS "];
const Flaps: React.FC<{ f: number }> = ({ f }) => (
  <Punch f={f}>
    <AbsoluteFill style={{ display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", gap: 18 }}>
      {FLAP_ROWS.map((row, r) => (
        <div key={r} style={{ display: "flex", gap: 12 }}>
          {row.split("").map((ch, i) => {
            const settle = 1 + i * 0.6 + r * 1.5;
            const done = f >= settle;
            const glyph = done ? ch : "ABCDEFGHJKLMNPRSTUVWXYZ0123456789+#"[Math.floor(random(`flap-${r}-${i}-${f}`) * 35)];
            const flip = done ? ramp(f, settle, settle + 2) : (f % 2) / 2;
            return (
              <div
                key={i}
                style={{
                  width: 132,
                  height: 172,
                  borderRadius: 12,
                  background: "linear-gradient(180deg, #1c1c1c 0%, #141414 49.5%, #050505 50%, #111 50.5%, #0c0c0c 100%)",
                  border: "1px solid rgba(245,245,245,0.08)",
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  fontFamily: DISPLAY,
                  fontWeight: 800,
                  fontSize: 124,
                  color: r === 0 && ch === "P" && done ? C.red : C.ink,
                  transform: `perspective(600px) rotateX(${(1 - flip) * 30}deg)`,
                  opacity: ch === " " && done ? 0.25 : 1,
                }}
              >
                {glyph === " " ? "" : glyph}
              </div>
            );
          })}
        </div>
      ))}
    </AbsoluteFill>
  </Punch>
);

const Export: React.FC<{ f: number }> = ({ f }) => (
  <Punch f={f}>
    <AbsoluteFill style={{ transform: `translateX(${-f * 6}px)`, display: "flex", flexDirection: "column", justifyContent: "center", gap: 14, paddingLeft: 40 }}>
      {Array.from({ length: 7 }, (_, r) => (
        <div key={r} style={{ display: "flex", gap: 14, whiteSpace: "nowrap" }}>
          {Array.from({ length: 6 }, (_, c) => {
            const word = (r + c) % 2 === 0 ? "MARKDOWN" : "JSON";
            const hotMd = r === 3 && c === 1;
            const hotJson = r === 2 && c === 3;
            const lit = ramp(f, hotMd ? 2 : 5, hotMd ? 5 : 8);
            return (
              <div
                key={c}
                style={{
                  ...display(64, 900),
                  letterSpacing: "-0.02em",
                  padding: "10px 26px",
                  borderRadius: 10,
                  color: hotMd || hotJson ? (hotJson ? "#000" : C.ink) : "rgba(245,245,245,0.13)",
                  background: hotMd ? `rgba(233,27,39,${lit})` : hotJson ? `rgba(245,245,245,${lit})` : "transparent",
                  boxShadow: hotMd ? `0 0 40px rgba(233,27,39,${0.5 * lit})` : "none",
                }}
              >
                {word}
              </div>
            );
          })}
        </div>
      ))}
    </AbsoluteFill>
    <div style={{ ...mono(18, C.ink2), position: "absolute", left: 960, top: 190, transform: "translateX(-50%)" }}>EXPORT EVERY CONVERSATION</div>
  </Punch>
);

const OpenSource: React.FC<{ f: number }> = ({ f }) => {
  const second = ramp(f, 3, 8, easeOut);
  return (
    <Punch f={f}>
      <AbsoluteFill style={{ display: "flex", flexDirection: "column", justifyContent: "center", paddingLeft: 330 }}>
        <div style={{ display: "flex", alignItems: "baseline" }}>
          <span style={{ ...display(250), textShadow: "0 0 40px rgba(255,255,255,0.2)" }}>OPEN</span>
          <span style={{ width: 46, height: 46, background: C.red, marginLeft: 14, boxShadow: `0 0 30px ${C.redGlow}`, transform: `scale(${1 + 0.4 * ring((f - 2) / 60, 3, 6)})` }} />
        </div>
        <div style={{ ...display(250), color: "transparent", WebkitTextStroke: "2.5px rgba(245,245,245,0.9)", marginTop: -10, transform: `translateX(${(1 - second) * 120}px)`, opacity: second }}>
          SOURCE
        </div>
        <div style={{ ...mono(20, C.ink2), marginTop: 28, opacity: second }}>FREE · APACHE-2.0 LICENSE</div>
      </AbsoluteFill>
    </Punch>
  );
};

const CARDS = [PressF9, LiveRewrite, PolishStructure, History, Incognito, Flaps, Export, OpenSource];

export const C07Montage: React.FC = () => {
  const frame = useCurrentFrame();
  let index = 0;
  CUTS.forEach((cut, i) => {
    if (i < CARDS.length && frame >= cut) index = i;
  });
  const Card = CARDS[index];
  return (
    <AbsoluteFill style={{ fontFamily: MONO }}>
      <Card f={frame - CUTS[index]} />
    </AbsoluteFill>
  );
};
