import { AbsoluteFill, useCurrentFrame } from "remotion";
import { beatFrame } from "../timing";
import { BLOOM, C, display, easeIn, easeOut, LABEL, mono, outline, ramp, ring, T } from "../../showreel/theme";
import { Keycap } from "../../showreel/parts/Keycap";
import { MarkPeriod } from "../../showreel/parts/MarkPeriod";
import { scramble } from "../../showreel/parts/Scramble";
import { Delivery } from "../parts/Delivery";

const B0 = beatFrame(44);
const k = (beat: number) => beatFrame(44 + beat) - B0;
const LENGTH = k(12);
// Bar 13: eight cuts on eighth notes. Bar 14 is the score's break before the final impact.
const CUT0 = k(4);
export const CUTS = Array.from({ length: 9 }, (_, i) => beatFrame(48 + i / 2) - B0);
const BREAK = k(8);

const Punch: React.FC<{ f: number; children: React.ReactNode; bg?: string }> = ({ f, children, bg }) => {
  const inT = ramp(f, 0, 4, easeOut);
  return (
    <AbsoluteFill style={{ background: bg }}>
      <AbsoluteFill style={{ transform: `scale(${1.08 - 0.08 * inT + 0.008 * f})`, filter: inT < 1 ? `blur(${(1 - inT) * 8}px)` : undefined }}>
        {children}
      </AbsoluteFill>
    </AbsoluteFill>
  );
};

// Where dictation goes now, one word per eighth note. Codex Desktop is listed on the wheel but
// its delivery is not yet verified live, so it is not named here.
const WORDS: { word: string; sub: string; style: "solid" | "outline" | "red" | "card" }[] = [
  { word: "CLAUDE CODE", sub: "SEND · QUEUE", style: "solid" },
  { word: "TERMINAL", sub: "ANY TERMINAL", style: "outline" },
  { word: "TMUX", sub: "THROUGH THE AGENT'S OWN QUEUE", style: "red" },
  { word: "HERDR", sub: "GUARDED PROMPT", style: "outline" },
  { word: "BACKGROUND", sub: "CLAUDE --BG", style: "solid" },
  { word: "CLAUDE DESKTOP", sub: "CODE SESSIONS · NEW CHATS", style: "outline" },
  { word: "CODEX", sub: "LIVE THREADS · SEND IF IDLE", style: "red" },
  { word: "ANY AGENT", sub: "ONE TARGET MODEL", style: "card" },
];

const Cut: React.FC<{ i: number; f: number }> = ({ i, f }) => {
  const w = WORDS[i];
  const size = w.word.length > 11 ? T.l : T.xl;
  const text =
    w.style === "outline" ? (
      <div style={outline(size, "rgba(245,245,245,0.92)", 2.5)}>{w.word}</div>
    ) : w.style === "red" ? (
      <div style={{ ...display(size), color: C.red, textShadow: `0 0 40px ${C.redGlow}` }}>{w.word}</div>
    ) : w.style === "card" ? (
      <div style={{ display: "flex", alignItems: "baseline" }}>
        <span style={{ ...display(size), color: "#050505" }}>{w.word}</span>
        <MarkPeriod fontSize={size} local={f} at={2} after={w.word} />
      </div>
    ) : (
      <div style={{ ...display(size), textShadow: BLOOM }}>{w.word}</div>
    );
  return (
    <Punch f={f} bg={w.style === "card" ? C.ink : undefined}>
      <AbsoluteFill style={{ justifyContent: "center", alignItems: "center", flexDirection: "column" }}>
        {text}
        <div style={{ ...mono(18, w.style === "card" ? "#262626" : LABEL), marginTop: 34, letterSpacing: "0.24em" }}>
          {scramble(w.sub, ramp(f, 1, 5), `cut-${i}`, f)}
        </div>
      </AbsoluteFill>
    </Punch>
  );
};

// The break: one key, pressed on the last offbeat, into the final impact.
const OneKey: React.FC<{ f: number }> = ({ f }) => {
  const len = LENGTH - BREAK;
  const inT = ramp(f, 0, 8, easeOut);
  const pressAt = len - beatFrame(0.5) + beatFrame(0);
  const press = f >= pressAt ? Math.sin(Math.min(1, (f - pressAt) / 10) * Math.PI) : 0;
  const glow = ramp(f, pressAt, len, easeIn);
  return (
    <AbsoluteFill style={{ justifyContent: "center", alignItems: "center" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 64, opacity: inT, transform: `scale(${0.96 + 0.04 * inT + 0.03 * ramp(f, 0, len, (t) => t)})` }}>
        <div style={{ display: "flex", alignItems: "baseline" }}>
          <span style={{ ...display(T.xl), textShadow: BLOOM }}>ONE KEY</span>
          <MarkPeriod fontSize={T.xl} local={f} at={6} after="ONE KEY" />
        </div>
        <div style={{ transform: `translateY(${8 * press}px)` }}>
          <Keycap label="F12" size={200} press={press} lit={Math.max(press, glow)} />
        </div>
      </div>
      <div style={{ ...mono(18, LABEL), position: "absolute", top: 720, letterSpacing: "0.24em", opacity: ramp(f, 8, 16) }}>
        OPEN · CHOOSE · SPEAK · SENT
      </div>
      <AbsoluteFill style={{ background: `radial-gradient(circle at 50% 50%, rgba(255,255,255,${0.5 * glow}), rgba(233,27,39,${0.3 * glow}) 30%, transparent 60%)` }} />
    </AbsoluteFill>
  );
};

export const U06Montage: React.FC = () => {
  const frame = useCurrentFrame();
  if (frame < CUT0) return <Delivery which="second" frame={frame} length={CUT0} />;
  if (frame < BREAK) {
    let i = 0;
    CUTS.forEach((c, j) => {
      if (frame >= c && j < WORDS.length) i = j;
    });
    return <Cut i={i} f={frame - CUTS[i]} />;
  }
  return <OneKey f={frame - BREAK} />;
};
