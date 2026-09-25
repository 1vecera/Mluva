import { AbsoluteFill, Easing, random, useCurrentFrame } from "remotion";
import { interpolatePath } from "@remotion/paths";
import { noise2D } from "@remotion/noise";
import { beatFrame } from "../timing";
import { brand, C, easeIn, easeInOut, easeOut, ramp, ring } from "../theme";
import { Box, lockupBoxes, LOCKUP, LockupFull, LockupHalf } from "../parts/Logo";
import { MARK_PATH, MARK_VIEWBOX } from "../parts/markPath";

const B0 = beatFrame(16);
const LENGTH = beatFrame(20) - B0;
const CX = 960;
const CY = 500;

// Final lockup: 900 px wide, centred on (960, 500).
export const LOCKUP_SCALE = 900 / LOCKUP.w;
export const LOCKUP_LEFT = CX - 450;
export const LOCKUP_TOP = CY - (LOCKUP.h * LOCKUP_SCALE) / 2;
const FINAL = lockupBoxes(LOCKUP_LEFT, LOCKUP_TOP, LOCKUP_SCALE);
// Between these frames mark and wordmark are both at rest: draw the brand file itself.
const REST_FROM = 75;
const REST_TO = LENGTH - 18;
const atRest = (frame: number) => frame >= REST_FROM && frame < REST_TO;
const FINAL_WORD_LEFT = (FINAL.word.x - LOCKUP_LEFT) / (LOCKUP.w * LOCKUP_SCALE);
// Chapter 06 picks the mark up as the orbit core.
export const CORE_BOX: Box = { x: 1330 - 62, y: 560 - 62, w: 124, h: 124 };

const [vx, vy, vw] = MARK_VIEWBOX.split(" ").map(Number);
const MID = { x: vx + vw / 2, y: vy + vw / 2 };
const circle = (r: number) => {
  const k = 0.5523 * r;
  const { x, y } = MID;
  return `M ${x} ${y - r} C ${x + k} ${y - r} ${x + r} ${y - k} ${x + r} ${y} C ${x + r} ${y + k} ${x + k} ${y + r} ${x} ${
    y + r
  } C ${x - k} ${y + r} ${x - r} ${y + k} ${x - r} ${y} C ${x - r} ${y - k} ${x - k} ${y - r} ${x} ${y - r} Z`;
};
const CIRCLE = circle(300);

const lerpBox = (a: Box, b: Box, t: number): Box => ({
  x: a.x + (b.x - a.x) * t,
  y: a.y + (b.y - a.y) * t,
  w: a.w + (b.w - a.w) * t,
  h: a.h + (b.h - a.h) * t,
});

const Explosion: React.FC<{ frame: number }> = ({ frame }) => {
  if (frame > 36) return null;
  const core = ramp(frame, 0, 12, easeOut, 1, 0);
  const wave = ramp(frame, 0, 28, easeOut);
  const t = frame / 60;
  return (
    <AbsoluteFill style={{ pointerEvents: "none" }}>
      <AbsoluteFill
        style={{
          background: `radial-gradient(circle at ${CX}px ${CY}px, rgba(255,255,255,${0.9 * core}) 0%, rgba(255,120,128,${
            0.55 * core
          }) 12%, rgba(233,27,39,${0.3 * core}) 30%, rgba(0,0,0,0) 60%)`,
        }}
      />
      <svg width={1920} height={1080} style={{ position: "absolute", inset: 0 }}>
        <defs>
          <filter id="spark-glow" x="-50%" y="-50%" width="200%" height="200%">
            <feGaussianBlur stdDeviation="2.5" />
          </filter>
        </defs>
        <circle cx={CX} cy={CY} r={30 + 900 * wave} fill="none" stroke="#fff" strokeWidth={2.5 - 1.8 * wave} opacity={0.8 * (1 - wave)} />
        {Array.from({ length: 48 }, (_, i) => {
          const angle = (i / 48) * Math.PI * 2 + 0.6 * (random(`ray-a-${i}`) - 0.5) * (Math.PI / 24);
          const speed = 700 + 1100 * random(`ray-s-${i}`);
          const len = 60 + 160 * random(`ray-l-${i}`);
          const travel = (speed * (1 - Math.exp(-3.4 * t))) / 3.4;
          const head = 50 + travel;
          const tail = Math.max(30, head - len * (1 - ramp(frame, 10, 24)));
          const op = ramp(frame, 0, 2) * ramp(frame, 8 + 10 * random(`ray-o-${i}`), 24, easeOut, 1, 0);
          return (
            <line
              key={i}
              x1={CX + Math.cos(angle) * tail}
              y1={CY + Math.sin(angle) * tail}
              x2={CX + Math.cos(angle) * head}
              y2={CY + Math.sin(angle) * head}
              stroke={i % 6 === 0 ? "#ff4a55" : "#fff"}
              strokeWidth={1.2 + 1.6 * random(`ray-w-${i}`)}
              strokeLinecap="round"
              opacity={op}
            />
          );
        })}
      </svg>
    </AbsoluteFill>
  );
};

// The drop's full-frame hit: white, then a red afterglow, gone in six frames.
const Hit: React.FC<{ frame: number }> = ({ frame }) => {
  if (frame > 6) return null;
  const white = frame <= 1 ? 0.7 : ramp(frame, 1, 3, easeOut, 0.7, 0);
  const red = frame < 3 ? 0.25 * ramp(frame, 0, 3) : ramp(frame, 3, 6, easeOut, 0.25, 0);
  return (
    <>
      <AbsoluteFill style={{ background: "#fff", opacity: white, mixBlendMode: "screen" }} />
      <AbsoluteFill style={{ background: C.red, opacity: red, mixBlendMode: "screen" }} />
    </>
  );
};

const FormingMark: React.FC<{ frame: number; ambient: boolean }> = ({ frame, ambient }) => {
  const t = (frame - 2) / 60;
  const settled = frame > 74;
  const morph = ramp(frame, 2, 24, easeOut);
  const grow = ramp(frame, 1, 20, easeOut);
  const jelly = settled ? 0 : 0.17 * ring(t - 0.05, 2.6, 4.2);
  const wobble = settled ? 0 : 7 * ring(t, 1.7, 3.8);
  const glossy = ramp(frame, 12, 30, easeInOut);
  const move = ramp(frame, 38, 60, easeInOut);
  const fly = ramp(frame, LENGTH - 16, LENGTH, easeInOut);
  const start: Box = { x: CX - 210, y: CY - 210, w: 420, h: 420 };
  let box = lerpBox(start, FINAL.mark, move);
  box = lerpBox(box, CORE_BOX, fly);
  const settle = 0.18 + 0.82 * grow;
  const sx = settle * (1 + jelly);
  const sy = settle * (1 - jelly);
  const identity = Math.abs(sx - 1) < 1e-4 && Math.abs(sy - 1) < 1e-4 && Math.abs(wobble) < 1e-3;
  const path = interpolatePath(morph, CIRCLE, MARK_PATH);
  const sweep = ramp(frame, 30, 52, easeInOut);
  return (
    <AbsoluteFill
      style={{
        transformOrigin: `${box.x + box.w / 2}px ${box.y + box.h * 0.6}px`,
        transform: identity ? undefined : `rotate(${wobble}deg) scale(${sx}, ${sy})`,
      }}
    >
      <div
        style={{
          position: "absolute",
          left: box.x - box.w * 0.4,
          top: box.y - box.h * 0.4,
          width: box.w * 1.8,
          height: box.h * 1.8,
          borderRadius: "50%",
          background: "radial-gradient(circle, rgba(233,27,39,0.34), rgba(233,27,39,0.08) 45%, transparent 68%)",
          opacity: ambient ? grow * (1 - 0.5 * fly) : 0,
        }}
      />
      {glossy < 1 ? (
        <svg viewBox={MARK_VIEWBOX} width={box.w} height={box.h} style={{ position: "absolute", left: box.x, top: box.y, opacity: ramp(glossy, 0.8, 1, (t) => t, 1, 0) }}>
          <path d={path} fill={C.red} />
        </svg>
      ) : null}
      {atRest(frame) ? (
        <LockupFull left={LOCKUP_LEFT} top={LOCKUP_TOP} scale={LOCKUP_SCALE} />
      ) : (
        <LockupHalf part="mark" left={LOCKUP_LEFT} top={LOCKUP_TOP} scale={LOCKUP_SCALE} markTo={box} style={{ opacity: glossy }} />
      )}
      {sweep > 0 && sweep < 1 ? (
        <div
          style={{
            position: "absolute",
            left: box.x,
            top: box.y,
            width: box.w,
            height: box.h,
            WebkitMaskImage: `url(${brand("mluva-mark-flat.svg")})`,
            WebkitMaskSize: "100% 100%",
            background: `linear-gradient(115deg, transparent ${sweep * 140 - 30}%, rgba(255,255,255,0.55) ${sweep * 140 - 15}%, transparent ${
              sweep * 140
            }%)`,
            mixBlendMode: "screen",
          }}
        />
      ) : null}
    </AbsoluteFill>
  );
};

// The wordmark wipes in once the sliding mark has cleared its space (its right edge passes x = 900
// at about frame 54), so nothing is occluded or cut.
const Word: React.FC<{ frame: number }> = ({ frame }) => {
  const reveal = ramp(frame, 54, 70, easeOut);
  const out = ramp(frame, LENGTH - 18, LENGTH - 6, easeIn);
  if (reveal <= 0 || out >= 1 || atRest(frame)) return null;
  const rest = reveal >= 1 && out <= 0;
  return (
    <LockupHalf
      part="word"
      left={LOCKUP_LEFT}
      top={LOCKUP_TOP}
      scale={LOCKUP_SCALE}
      transform={rest ? "" : `translateX(${(1 - reveal) * -16 + out * 60}px)`}
      style={
        rest
          ? undefined
          : {
              clipPath: `inset(-20% ${(1 - reveal) * (1 - FINAL_WORD_LEFT) * 100}% -20% 0)`,
              filter: `blur(${(1 - reveal) * 3 + out * 10}px)`,
              opacity: 1 - out,
            }
      }
    />
  );
};

// `ambient` false hides the glow for the pixel check in LockupCheck.tsx.
export const C05Logo: React.FC<{ ambient?: boolean }> = ({ ambient = true }) => {
  const frame = useCurrentFrame();
  const shake = frame < 12 ? (1 - frame / 12) * 8 : 0;
  const sx = shake * noise2D("shake-x", frame * 0.9, 0);
  const sy = shake * noise2D("shake-y", 0, frame * 0.9);
  const punch = frame < 14 ? 1 + 0.06 * (1 - Easing.out(Easing.exp)(frame / 14)) : 1;
  return (
    <AbsoluteFill style={{ transform: frame < 14 ? `translate(${sx}px, ${sy}px) scale(${punch})` : undefined }}>
      <Hit frame={frame} />
      <Explosion frame={frame} />
      <FormingMark frame={frame} ambient={ambient} />
      <Word frame={frame} />
    </AbsoluteFill>
  );
};
