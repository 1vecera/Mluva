import { AbsoluteFill, Img, staticFile } from "remotion";
import take from "../delivery-take.json";
import { beatFrame } from "../timing";
import { C, easeInOut, easeOut, mono, ramp } from "../../showreel/theme";
import { scramble } from "../../showreel/parts/Scramble";

// The delivery take: real Mluva (left half) and a disposable Codex TUI over a disposable Claude Code
// session in tmux in Ghostty (right half), recorded together from one private 3840×2160 screen.
// The reel shows it through one viewport, 1680 px wide and centred on y = 525; a framing is a
// rectangle of that screen (x, y, scale s) and the viewport's height h, which eases between shots.
const VIEW = { x: 120, w: 1680, cy: 525 };
type Framing = { x: number; y: number; s: number; h: number };
const S = VIEW.w / 1920;
export const FRAMINGS = {
  // The whole screen: both applications.
  wide: { x: 0, y: 200, s: VIEW.w / 3840, h: 770 },
  // Mluva's conversation: the recording, then the transcript.
  mluva: { x: 0, y: 40, s: S, h: 400 },
  // Mluva's status lines: the recipient, the countdown and the outcome.
  status: { x: 0, y: 1670, s: S, h: 420 },
  // The Codex pane: the dictation arriving as the thread's next turn.
  codex: { x: 1920, y: 330, s: S, h: 700 },
  // Closer on the dictated turn and the reply, above the status line.
  codexClose: { x: 1940, y: 670, s: 1, h: 430 },
  // The Claude Code pane: the dictation arriving from Claude Code's inbox, from the message to the prompt.
  claude: { x: 1920, y: 1262, s: S, h: 690 },
} satisfies Record<string, Framing>;

// One shot: from `beat` (relative to the chapter), show take time `from` onward at `rate`, through
// `framing`; `move` frames ease from the previous framing instead of cutting.
// `push` scales the framing up by that fraction across the shot, around its centre, so holds stay alive.
export type Shot = { beat: number; from: number; rate: number; framing: keyof typeof FRAMINGS; move?: number; push?: number; label: string };

const frameAt = (t: number) => {
  // The last staged frame captured at or before take time t.
  let lo = 0;
  let hi = take.times.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (take.times[mid] <= t) lo = mid;
    else hi = mid - 1;
  }
  return lo;
};

const lerp = (a: Framing, b: Framing, k: number): Framing => ({
  x: a.x + (b.x - a.x) * k,
  y: a.y + (b.y - a.y) * k,
  s: a.s + (b.s - a.s) * k,
  h: a.h + (b.h - a.h) * k,
});

// `punch` opens the chapter on a hit: the panel lands from 4 % larger and the room behind it flashes,
// never anything over the captured pixels.
export const Delivery: React.FC<{ frame: number; b0: number; shots: Shot[]; length: number; note: string; punch?: boolean }> = ({
  frame,
  b0,
  shots,
  length,
  note,
  punch = true,
}) => {
  const at = (beat: number) => beatFrame(b0 + beat) - beatFrame(b0);
  let i = 0;
  shots.forEach((shot, j) => {
    if (frame >= at(shot.beat)) i = j;
  });
  const shot = shots[i];
  const local = frame - at(shot.beat);
  const t = shot.from + (local / 60) * shot.rate;
  const n = frameAt(t);
  const target = FRAMINGS[shot.framing];
  const prev = i > 0 ? FRAMINGS[shots[i - 1].framing] : target;
  const eased = shot.move ? lerp(prev, target, ramp(local, 0, shot.move, easeInOut)) : target;
  const next = shots[i + 1] ? at(shots[i + 1].beat) : length;
  // A push zooms about the framing's centre: the visible rectangle shrinks, the viewport stays.
  const grow = 1 + (shot.push ?? 0) * ramp(local, 0, next - at(shot.beat), (v) => v);
  const cw = VIEW.w / eased.s;
  const ch = eased.h / eased.s;
  const view = { ...eased, s: eased.s * grow, x: eased.x + (cw - cw / grow) / 2, y: eased.y + (ch - ch / grow) / 2 };
  const hit = punch ? 1 + 0.04 * (1 - ramp(frame, 0, 12, easeOut)) : 1;
  const flash = punch ? ramp(frame, 0, 10, easeOut, 1, 0) : 0;
  const labelIn = ramp(local, 0, 5);
  const top = VIEW.cy - view.h / 2;
  return (
    <AbsoluteFill>
      {flash > 0 ? (
        <AbsoluteFill style={{ background: `radial-gradient(ellipse 70% 60% at 50% 48%, rgba(255,255,255,${0.32 * flash}), rgba(233,27,39,${0.22 * flash}) 45%, transparent 75%)` }} />
      ) : null}
      <div
        style={{
          position: "absolute",
          left: VIEW.x,
          top,
          width: VIEW.w,
          height: view.h,
          overflow: "hidden",
          transform: hit !== 1 ? `scale(${hit})` : undefined,
          boxShadow: "0 0 0 1px rgba(255,255,255,0.12), 0 50px 140px rgba(0,0,0,0.8)",
          background: "#000",
        }}
      >
        <Img
          src={staticFile(`universal/delivery/${String(n).padStart(5, "0")}.png`)}
          style={{
            position: "absolute",
            left: -view.x * view.s,
            top: -view.y * view.s,
            width: take.screen[0] * view.s,
            height: take.screen[1] * view.s,
            maxWidth: "none",
          }}
        />
      </div>
      {/* Captions sit below the viewport, never over the captured pixels. */}
      <div style={{ position: "absolute", left: VIEW.x, top: top + view.h + 22, display: "flex", alignItems: "center", gap: 14 }}>
        <span style={{ width: 8, height: 8, background: C.red, boxShadow: `0 0 10px ${C.redGlow}` }} />
        <span style={{ ...mono(15, C.ink), opacity: labelIn }}>{scramble(shot.label, labelIn, `dl-${i}`, frame)}</span>
      </div>
      <div style={{ ...mono(16, C.ink2), letterSpacing: "0.16em", position: "absolute", right: 1920 - VIEW.x - VIEW.w, top: top + view.h + 22 }}>{note}</div>
    </AbsoluteFill>
  );
};
