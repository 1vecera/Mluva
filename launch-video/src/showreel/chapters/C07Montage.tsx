import { AbsoluteFill, random, useCurrentFrame } from "remotion";
import { beatFrame } from "../timing";
import { BLOOM, C, display, DISPLAY, easeOut, LABEL, MONO, mono, outline, ramp, T } from "../theme";
import { Keycap } from "../parts/Keycap";
import { MarkPeriod } from "../parts/MarkPeriod";
import { scramble } from "../parts/Scramble";

const B0 = beatFrame(24);
export const CUTS = Array.from({ length: 9 }, (_, k) => beatFrame(24 + k / 2) - B0);

// Ink bounds of Adwaita Sans Black at -0.035em tracking (fontTools, AdwaitaSans-Black-NoOverlap.ttf):
// the ink starts 0.047em inside the text box and ends 0.0065em before its right edge.
const INK_LEFT = "0.047em";
const INK_RIGHT = "0.0065em";
// Keep type walls clear of the HUD bands at the top and bottom of the frame.
const HUD_SAFE = "linear-gradient(transparent 0px, transparent 140px, black 220px, black 820px, transparent 900px)";

const Punch: React.FC<{ f: number; children: React.ReactNode; bg?: string }> = ({ f, children, bg }) => {
  const inT = ramp(f, 0, 4, easeOut);
  return (
    <AbsoluteFill style={{ background: bg }}>
      <AbsoluteFill style={{ transform: `scale(${1.08 - 0.08 * inT + 0.01 * f})`, filter: inT < 1 ? `blur(${(1 - inT) * 8}px)` : undefined }}>
        {children}
      </AbsoluteFill>
    </AbsoluteFill>
  );
};

const Tag: React.FC<{ color: string; style?: React.CSSProperties }> = ({ color, style }) => (
  <div style={{ ...mono(16, color), letterSpacing: "0.2em", border: `1.5px solid ${color}`, padding: "7px 12px", position: "absolute", ...style }}>
    EXPERIMENTAL
  </div>
);

const PressF9: React.FC<{ f: number }> = ({ f }) => {
  const p = f >= 2 && f < 7 ? Math.sin(((f - 2) / 5) * Math.PI) : 0;
  return (
    <Punch f={f}>
      <AbsoluteFill style={{ display: "flex", flexDirection: "row", alignItems: "center", justifyContent: "center", gap: 70 }}>
        <div style={{ ...display(T.xl), textShadow: BLOOM }}>PRESS</div>
        <div style={{ transform: `translateY(${6 * p}px) scale(${1 - 0.03 * p})`, marginTop: 20 }}>
          <Keycap label="F9" size={230} />
        </div>
      </AbsoluteFill>
    </Punch>
  );
};

const LiveRewrite: React.FC<{ f: number }> = ({ f }) => {
  const text = scramble("LIVE REWRITE", ramp(f, 0, 4), "live", f * 3, 0.85);
  const line = ramp(f, 2, 9, easeOut);
  return (
    <Punch f={f}>
      <AbsoluteFill style={{ display: "flex", alignItems: "center", justifyContent: "center" }}>
        <div style={{ position: "relative", ...display(T.xl), textShadow: BLOOM }}>
          <span style={{ visibility: "hidden" }}>LIVE REWRITE</span>
          <span style={{ position: "absolute", left: 0, top: 0 }}>{text}</span>
          <div
            style={{
              position: "absolute",
              left: INK_LEFT,
              right: INK_RIGHT,
              bottom: -26,
              height: 8,
              background: C.red,
              transformOrigin: "0 50%",
              transform: `scaleX(${line})`,
            }}
          />
          <Tag color={C.red} style={{ right: INK_RIGHT, top: -58, opacity: ramp(f, 3, 5), fontFamily: MONO }} />
        </div>
      </AbsoluteFill>
    </Punch>
  );
};

const PolishStructure: React.FC<{ f: number }> = ({ f }) => {
  const rows = 9;
  return (
    <Punch f={f}>
      <AbsoluteFill style={{ display: "flex", flexDirection: "column", justifyContent: "center", WebkitMaskImage: HUD_SAFE }}>
        {Array.from({ length: rows }, (_, r) => {
          const center = r === Math.floor(rows / 2);
          const dir = r % 2 === 0 ? 1 : -1;
          const x = -400 + dir * f * 9 - ((r * 173) % 400);
          const quiet = outline(T.m, "rgba(245,245,245,0.16)", 1.5);
          return (
            <div key={r} style={{ whiteSpace: "nowrap", transform: `translateX(${x}px)`, height: 112, lineHeight: "112px" }}>
              {Array.from({ length: 5 }, (_, i) => (
                <span key={i} style={{ marginRight: 48 }}>
                  <span style={center ? display(T.m) : quiet}>POLISH</span>
                  <span style={{ marginRight: 48 }} />
                  <span style={center ? { ...display(T.m), color: C.red } : quiet}>STRUCTURE</span>
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
      <AbsoluteFill style={{ WebkitMaskImage: HUD_SAFE }}>
        <AbsoluteFill style={{ transform: "rotate(-11deg) scale(1.25)", display: "flex", flexDirection: "column", justifyContent: "center" }}>
          {Array.from({ length: 8 }, (_, r) => {
            const dir = r % 2 === 0 ? -1 : 1;
            const tone = r === 3 ? C.ink : r === 5 ? C.red : "#2a2a2a";
            return (
              <div key={r} style={{ ...display(T.m), color: tone, transform: `translateX(${-600 + dir * f * 14 - r * 90}px)`, height: 110, lineHeight: "110px" }}>
                {Array.from({ length: 6 }, (_, i) => words.map((w) => `${w} · `).join("") + (r === 3 && i === 1 ? "YOUR HISTORY · " : "")).join("")}
              </div>
            );
          })}
        </AbsoluteFill>
      </AbsoluteFill>
    </Punch>
  );
};

const Incognito: React.FC<{ f: number }> = ({ f }) => {
  const bar = ramp(f, 2, 8, easeOut);
  return (
    <Punch f={f} bg={C.red}>
      <AbsoluteFill style={{ display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center" }}>
        <svg width={96} height={96} viewBox="0 0 48 48" style={{ marginBottom: 26 }}>
          <path d="M4 24 C10 13 17 9 24 9 C31 9 38 13 44 24 C38 35 31 39 24 39 C17 39 10 35 4 24 Z" fill="none" stroke="#000" strokeWidth={3.4} />
          <circle cx={24} cy={24} r={6.5} fill="#000" />
          <line x1={8} y1={40} x2={40} y2={8} stroke="#000" strokeWidth={3.8} strokeLinecap="round" />
        </svg>
        <div style={{ position: "relative", ...display(T.xl), color: "#000" }}>
          INCOGNITO
          <div style={{ position: "absolute", left: INK_LEFT, right: INK_RIGHT, bottom: -30, height: 14, background: "#000", transformOrigin: "0 50%", transform: `scaleX(${bar})` }} />
          <Tag color="#000" style={{ right: INK_RIGHT, top: -60, opacity: ramp(f, 3, 5) }} />
        </div>
        <div style={{ ...mono(20, "#000"), marginTop: 66, letterSpacing: "0.24em" }}>NO HISTORY · NO RECOVERY AUDIO</div>
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
            const settle = 0.5 + i * 0.3 + r * 0.8;
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
                  fontSize: T.m,
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
    <AbsoluteFill style={{ WebkitMaskImage: HUD_SAFE }}>
      <AbsoluteFill style={{ transform: `translateX(${-f * 6}px)`, display: "flex", flexDirection: "column", justifyContent: "center", gap: 16, paddingLeft: 40 }}>
        {Array.from({ length: 7 }, (_, r) => (
          <div key={r} style={{ display: "flex", gap: 16, whiteSpace: "nowrap" }}>
            {Array.from({ length: 6 }, (_, c) => {
              const word = (r + c) % 2 === 0 ? "MARKDOWN" : "JSON";
              const hotMd = r === 3 && c === 1;
              const hotJson = r === 2 && c === 3;
              const lit = ramp(f, hotMd ? 1 : 2, hotMd ? 3 : 4);
              return (
                <div
                  key={c}
                  style={{
                    ...display(T.s),
                    letterSpacing: "-0.02em",
                    padding: "8px 24px",
                    color: hotMd || hotJson ? (hotJson ? "#000" : C.ink) : "rgba(245,245,245,0.13)",
                    background: hotMd ? `rgba(233,27,39,${lit})` : hotJson ? `rgba(245,245,245,${lit})` : "transparent",
                  }}
                >
                  {word}
                </div>
              );
            })}
          </div>
        ))}
      </AbsoluteFill>
    </AbsoluteFill>
    <div style={{ ...mono(15, LABEL), position: "absolute", left: 960, top: 196, transform: "translateX(-50%)", background: C.bg, padding: "10px 22px" }}>
      EXPORT EVERY CONVERSATION
    </div>
  </Punch>
);

const OpenSource: React.FC<{ f: number }> = ({ f }) => {
  const second = ramp(f, 2, 6, easeOut);
  return (
    <Punch f={f}>
      <AbsoluteFill style={{ display: "flex", flexDirection: "column", justifyContent: "center", paddingLeft: 330 }}>
        <div style={{ display: "flex", alignItems: "baseline" }}>
          <span style={{ ...display(T.xl), textShadow: BLOOM }}>OPEN</span>
          <MarkPeriod fontSize={T.xl} local={f} at={1} />
        </div>
        <div style={{ ...outline(T.xl, "rgba(245,245,245,0.9)", 2.5), marginTop: -6, transform: `translateX(${(1 - second) * 120}px)`, opacity: second }}>
          SOURCE
        </div>
        <div style={{ ...mono(18, LABEL), marginTop: 30, opacity: second }}>FREE · APACHE-2.0 LICENSE</div>
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
    <AbsoluteFill>
      <Card f={frame - CUTS[index]} />
    </AbsoluteFill>
  );
};
