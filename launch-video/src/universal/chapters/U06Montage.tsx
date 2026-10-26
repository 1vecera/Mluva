import { AbsoluteFill, useCurrentFrame } from "remotion";
import { beatFrame } from "../timing";
import { BLOOM, C, display, easeIn, easeOut, mono, outline, ramp, T } from "../../showreel/theme";
import { Keycap } from "../../showreel/parts/Keycap";
import { MarkPeriod } from "../../showreel/parts/MarkPeriod";
import { scramble } from "../../showreel/parts/Scramble";
import { Delivery, Shot } from "../parts/Delivery";

const B0 = beatFrame(44);
const k = (beat: number) => beatFrame(44 + beat) - B0;
const LENGTH = k(12);
// Bar 13: four cuts, one per beat. Bar 14 is the score's break before the final impact.
const CUT0 = k(4);
const CUTS = Array.from({ length: 5 }, (_, i) => beatFrame(48 + i) - B0);
const BREAK = k(8);

// The re-drop: the queued dictation lands in Claude Code ("Portal test") at take 22.10 s, on the
// beat, and its reply at 22.58 s; Mluva reports it delivered from 22.42 s. Real speed.
const CLAUDE: Shot[] = [
  { beat: 0, from: 22.1, rate: 1, framing: "claude", push: 0.09, label: "CLAUDE CODE · QUEUED IN ITS OWN INBOX" },
  { beat: 2, from: 23.033, rate: 1, framing: "status", push: 0.05, label: "DELIVERED · TAKEN INTO A TURN" },
];

// `iris` opens the cut from the centre instead of popping it, for the white card.
const Punch: React.FC<{ f: number; children: React.ReactNode; bg?: string; iris?: boolean }> = ({ f, children, bg, iris }) => {
  const inT = ramp(f, 0, 4, easeOut);
  const open = iris ? ramp(f, 0, 7, easeOut) : 1;
  return (
    <AbsoluteFill style={{ background: bg, clipPath: open < 1 ? `circle(${open * 120}% at 50% 50%)` : undefined }}>
      <AbsoluteFill style={{ transform: `scale(${1.08 - 0.08 * inT + 0.008 * f})`, filter: inT < 1 ? `blur(${(1 - inT) * 8}px)` : undefined }}>
        {children}
      </AbsoluteFill>
    </AbsoluteFill>
  );
};

// Where dictation goes now, one product per beat. Codex Desktop is listed on the wheel but its
// delivery is not yet verified live, so it is not named here.
const WORDS: { word: string; sub: string; style: "solid" | "outline" | "red" | "card" }[] = [
  { word: "CLAUDE CODE", sub: "TERMINAL · TMUX · HERDR · BACKGROUND", style: "solid" },
  { word: "CODEX", sub: "TERMINAL · TMUX · HERDR · SEND IF IDLE", style: "red" },
  { word: "CLAUDE DESKTOP", sub: "CODE SESSIONS · NEW CHATS", style: "outline" },
  { word: "ANY AGENT", sub: "WITH OR WITHOUT HERDR", style: "card" },
];

const Cut: React.FC<{ i: number; f: number }> = ({ i, f }) => {
  const w = WORDS[i];
  const size = w.word.length > 11 ? T.l : T.xl;
  const text =
    w.style === "outline" ? (
      <div style={outline(size, C.ink, 3)}>{w.word}</div>
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
    <Punch f={f} bg={w.style === "card" ? C.ink : undefined} iris={w.style === "card"}>
      <AbsoluteFill style={{ justifyContent: "center", alignItems: "center", flexDirection: "column" }}>
        {text}
        <div style={{ ...mono(22, w.style === "card" ? "#262626" : C.ink2), marginTop: 38, letterSpacing: "0.22em" }}>
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
      <div style={{ ...mono(22, C.ink2), position: "absolute", top: 720, letterSpacing: "0.22em", opacity: ramp(f, 8, 16) }}>
        OPEN · CHOOSE · SPEAK · DELIVERED
      </div>
      <AbsoluteFill style={{ background: `radial-gradient(circle at 50% 50%, rgba(255,255,255,${0.32 * glow}), rgba(233,27,39,${0.28 * glow}) 30%, transparent 60%)` }} />
    </AbsoluteFill>
  );
};

export const U06Montage: React.FC = () => {
  const frame = useCurrentFrame();
  if (frame < CUT0) return <Delivery frame={frame} b0={44} shots={CLAUDE} length={CUT0} note="REAL APP · SCRIPTED DEMO · LOOPBACK MODEL" />;
  if (frame < BREAK) {
    let i = 0;
    CUTS.forEach((c, j) => {
      if (frame >= c && j < WORDS.length) i = j;
    });
    return <Cut i={i} f={frame - CUTS[i]} />;
  }
  return <OneKey f={frame - BREAK} />;
};
