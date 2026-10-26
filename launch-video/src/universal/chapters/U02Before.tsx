import { AbsoluteFill, Img, random, useCurrentFrame } from "remotion";
import { beatFrame } from "../timing";
import { BEFORE } from "../claims";
import { brand, C, display, easeIn, easeInOut, easeOut, LABEL, mono, outline, ramp, ring, T } from "../../showreel/theme";
import { MarkPeriod } from "../../showreel/parts/MarkPeriod";
import { hBlur } from "../../showreel/parts/Blur";
import { scramble } from "../../showreel/parts/Scramble";

const B0 = beatFrame(8);
const k = (beat: number) => beatFrame(8 + beat) - B0;
const LENGTH = k(8);
const GLITCH = k(4);

const SOURCE = { x: 470, y: 560 };
const ROW_X = 1060;
const ROW_W = 660;
const ROW_H = 76;
const rowY = (i: number) => 300 + i * 92;

// Before the target model, a wire per row: Herdr's guarded prompt sent, everything else stopped.
const Wire: React.FC<{ i: number; frame: number }> = ({ i, frame }) => {
  const row = BEFORE[i];
  const y = rowY(i) + ROW_H / 2;
  const x0 = SOURCE.x + 92;
  const x1 = ROW_X - 14;
  const mx = (x0 + x1) / 2;
  const d = `M ${x0} ${SOURCE.y} C ${mx} ${SOURCE.y} ${mx} ${y} ${x1} ${y}`;
  const live = row.outcome === "SEND";
  const draw = live ? ramp(frame, k(1) + i * 3, k(1) + i * 3 + 12, easeInOut) : ramp(frame, 4 + i * 2, 18 + i * 2);
  const stopAt = row.outcome === "NOT LISTED" ? 0.38 : 0.62;
  const shown = live ? draw : draw * stopAt;
  const decided = frame >= k(2) + (i - 2) * 7;
  return (
    <g>
      <path d={d} fill="none" stroke="rgba(245,245,245,0.1)" strokeWidth={1.5} strokeDasharray="2 8" strokeLinecap="round" />
      <path
        d={d}
        pathLength={1}
        fill="none"
        stroke={live ? C.red : "rgba(245,245,245,0.32)"}
        strokeWidth={live ? 2.2 : 1.5}
        strokeDasharray={`${shown} 1`}
        style={live ? { filter: `drop-shadow(0 0 6px ${C.redGlow})` } : undefined}
      />
      {!live && decided ? (
        <g opacity={ramp(frame, k(2) + (i - 2) * 7, k(2) + (i - 2) * 7 + 5)}>
          {(() => {
            // The stop mark sits where the wire ends.
            const t = stopAt;
            const bx = (1 - t) ** 3 * x0 + 3 * (1 - t) ** 2 * t * mx + 3 * (1 - t) * t * t * mx + t ** 3 * x1;
            const by = (1 - t) ** 3 * SOURCE.y + 3 * (1 - t) ** 2 * t * SOURCE.y + 3 * (1 - t) * t * t * y + t ** 3 * y;
            return (
              <g transform={`translate(${bx} ${by})`}>
                <line x1={-7} y1={-7} x2={7} y2={7} stroke={C.ink2} strokeWidth={1.6} />
                <line x1={-7} y1={7} x2={7} y2={-7} stroke={C.ink2} strokeWidth={1.6} />
              </g>
            );
          })()}
        </g>
      ) : null}
    </g>
  );
};

const Row: React.FC<{ i: number; frame: number }> = ({ i, frame }) => {
  const row = BEFORE[i];
  const enter = ramp(frame, 2 + i * 2, 16 + i * 2, easeOut);
  const live = row.outcome === "SEND";
  const at = live ? k(1) + i * 3 + 10 : k(2) + (i - 2) * 7;
  const tag = ramp(frame, at, at + 6);
  const pop = 1 + 0.08 * ring((frame - at) / 60, 3, 8);
  const dim = live ? 0 : ramp(frame, at, at + 8);
  return (
    <div
      style={{
        position: "absolute",
        left: ROW_X,
        top: rowY(i),
        width: ROW_W,
        height: ROW_H,
        border: `1px solid ${live && frame >= at ? "rgba(233,27,39,0.55)" : "rgba(245,245,245,0.12)"}`,
        background: live && frame >= at ? "rgba(233,27,39,0.05)" : "rgba(245,245,245,0.015)",
        opacity: enter * (1 - 0.45 * dim),
        transform: `translateX(${(1 - enter) * 60}px)`,
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        padding: "0 26px",
        boxSizing: "border-box",
      }}
    >
      <div style={{ display: "flex", alignItems: "baseline", gap: 18 }}>
        <span style={{ ...display(30, 700), letterSpacing: "-0.01em", color: live ? C.ink : C.ink2 }}>{row.kind}</span>
        <span style={{ ...mono(15, LABEL) }}>{row.where}</span>
      </div>
      <div
        style={{
          ...mono(15, live ? C.ink : row.outcome === "DRAFT ONLY" ? C.ink2 : C.ink3),
          opacity: tag,
          transform: `scale(${pop})`,
          display: "flex",
          alignItems: "center",
          gap: 10,
          textDecoration: row.outcome === "NOT LISTED" ? "line-through" : undefined,
        }}
      >
        {live ? <span style={{ width: 8, height: 8, borderRadius: 4, background: C.red, boxShadow: `0 0 10px ${C.redGlow}` }} /> : null}
        {scramble(row.outcome, ramp(frame, at, at + 5), `before-${i}`, frame)}
      </div>
    </div>
  );
};

const Diagram: React.FC<{ frame: number }> = ({ frame }) => {
  const enter = ramp(frame, 0, 12, easeOut);
  const head = ramp(frame, 4, 16, easeOut);
  const glitch = frame >= GLITCH ? ramp(frame, GLITCH, GLITCH + 14, easeIn) : 0;
  // Horizontal slices tear sideways as the old routing falls apart.
  const slices = 9;
  const torn = glitch > 0;
  const body = (
    <AbsoluteFill>
      <svg width={1920} height={1080} style={{ position: "absolute", inset: 0 }}>
        {BEFORE.map((_, i) => (
          <Wire key={i} i={i} frame={frame} />
        ))}
      </svg>
      <div style={{ position: "absolute", left: SOURCE.x - 80, top: SOURCE.y - 80, width: 160, height: 160, opacity: enter, transform: `scale(${0.6 + 0.4 * enter})` }}>
        <div style={{ position: "absolute", inset: -40, borderRadius: "50%", background: "radial-gradient(circle, rgba(233,27,39,0.25), transparent 65%)" }} />
        <Img src={brand("mluva-mark.svg")} style={{ position: "absolute", inset: 0, width: 160, height: 160 }} />
      </div>
      <div style={{ ...mono(15, LABEL), position: "absolute", left: SOURCE.x - 120, width: 240, textAlign: "center", top: SOURCE.y + 104, opacity: enter }}>
        MLUVA · F12
      </div>
      {BEFORE.map((_, i) => (
        <Row key={i} i={i} frame={frame} />
      ))}
      <div style={{ position: "absolute", left: 128, top: 178, opacity: head, transform: `translateY(${(1 - head) * 16}px)` }}>
        <div style={{ ...mono(15, LABEL), display: "flex", alignItems: "center", gap: 14 }}>
          <span style={{ display: "inline-block", width: 9, height: 9, background: C.red }} />
          BEFORE
        </div>
        <div style={{ display: "flex", alignItems: "baseline", marginTop: 14 }}>
          <span style={{ ...display(T.s) }}>One route</span>
          <MarkPeriod fontSize={T.s} local={frame} at={10} />
        </div>
        <div style={{ ...mono(15, LABEL), marginTop: 16 }}>ONLY HERDR'S GUARDED PROMPT COULD SEND</div>
      </div>
    </AbsoluteFill>
  );
  if (!torn) return <AbsoluteFill style={{ opacity: enter, filter: enter < 1 ? hBlur(50 * (1 - enter)) : undefined }}>{body}</AbsoluteFill>;
  return (
    <AbsoluteFill style={{ opacity: 1 - glitch }}>
      {Array.from({ length: slices }, (_, s) => {
        const top = (s / slices) * 100;
        const dx = (random(`tear-${s}`) - 0.5) * 520 * glitch + (s % 2 ? 1 : -1) * 40 * glitch;
        return (
          <AbsoluteFill key={s} style={{ clipPath: `inset(${top}% 0 ${100 - top - 100 / slices}% 0)`, transform: `translateX(${dx}px)` }}>
            {body}
          </AbsoluteFill>
        );
      })}
    </AbsoluteFill>
  );
};

// After the tear: the routing was rebuilt around one model.
const Rebuilt: React.FC<{ frame: number }> = ({ frame }) => {
  const a = k(4);
  const b = k(6);
  if (frame < a) return null;
  const first = ramp(frame, a, a + 8, easeOut);
  const second = ramp(frame, b, b + 7, easeOut);
  const collapse = ramp(frame, LENGTH - 7, LENGTH, easeIn);
  const split = 8 * (1 - ramp(frame, a, a + 10));
  const scaleY = 1 - collapse * 0.97;
  const line = (
    <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 6 }}>
      <div style={{ ...outline(T.l, "rgba(245,245,245,0.9)", 2), opacity: first, transform: `translateY(${(1 - first) * 40}px)` }}>REBUILT</div>
      <div style={{ display: "flex", alignItems: "baseline", opacity: second, transform: `translateY(${(1 - second) * 50}px)` }}>
        <span style={{ ...display(T.l) }}>FROM SCRATCH</span>
        <MarkPeriod fontSize={T.l} local={frame - b} at={5} after="FROM SCRATCH" />
      </div>
    </div>
  );
  return (
    <AbsoluteFill style={{ justifyContent: "center", alignItems: "center", transform: `scaleY(${scaleY})`, filter: `brightness(${1 + collapse * 1.5})` }}>
      {split > 0.3 ? (
        <>
          <AbsoluteFill style={{ justifyContent: "center", alignItems: "center", transform: `translateX(${-split}px)`, mixBlendMode: "screen", opacity: 0.7, filter: "sepia(1) saturate(8) hue-rotate(-40deg)" }}>
            {line}
          </AbsoluteFill>
          <AbsoluteFill style={{ justifyContent: "center", alignItems: "center", transform: `translateX(${split}px)`, mixBlendMode: "screen", opacity: 0.6, filter: "sepia(1) saturate(8) hue-rotate(150deg)" }}>
            {line}
          </AbsoluteFill>
        </>
      ) : null}
      {line}
      <div style={{ ...mono(15, LABEL), position: "absolute", top: 760, opacity: ramp(frame, b + 6, b + 14) * (1 - collapse) }}>
        ONE TARGET MODEL · EVERY ROUTE BEHIND IT
      </div>
    </AbsoluteFill>
  );
};

export const U02Before: React.FC = () => {
  const frame = useCurrentFrame();
  const seed = ramp(frame, LENGTH - 2, LENGTH, easeOut);
  return (
    <AbsoluteFill>
      {frame < GLITCH + 16 ? <Diagram frame={frame} /> : null}
      <Rebuilt frame={frame} />
      {seed > 0 ? (
        <div
          style={{
            position: "absolute",
            left: 960 - 3,
            top: 540 - 3,
            width: 6,
            height: 6,
            borderRadius: 3,
            background: "#fff",
            boxShadow: "0 0 24px 6px rgba(255,255,255,0.5), 0 0 60px 14px rgba(233,27,39,0.4)",
            opacity: seed,
          }}
        />
      ) : null}
    </AbsoluteFill>
  );
};
