import { C, mono, ramp } from "../theme";
import { scramble } from "./Scramble";

// A thin leader from an anchor point to a mono label, drawn on in `progress`.
export const Callout: React.FC<{
  x: number;
  y: number;
  dx: number;
  dy: number;
  label: string;
  frame: number;
  at: number;
  until?: number;
  align?: "left" | "right";
}> = ({ x, y, dx, dy, label, frame, at, until = 1e9, align = "left" }) => {
  const draw = ramp(frame, at, at + 10);
  const out = ramp(frame, until, until + 6, (t) => t, 1, 0);
  const opacity = draw * out;
  if (opacity <= 0) return null;
  const elbowY = y + dy * Math.min(1, draw * 2);
  const run = Math.max(0, draw * 2 - 1);
  const endX = x + dx * run;
  const text = scramble(label, ramp(frame, at + 5, at + 9), label, frame);
  return (
    <div style={{ position: "absolute", inset: 0, opacity, pointerEvents: "none" }}>
      <svg width={1920} height={1080} style={{ position: "absolute", inset: 0 }}>
        <circle cx={x} cy={y} r={4} fill="none" stroke={C.ink} strokeWidth={1.2} />
        <circle cx={x} cy={y} r={1.6} fill={C.ink} />
        <polyline
          points={`${x},${y} ${x},${elbowY} ${endX},${elbowY}`}
          fill="none"
          stroke="rgba(245,245,245,0.55)"
          strokeWidth={1}
        />
      </svg>
      <div
        style={{
          ...mono(15, C.ink),
          position: "absolute",
          top: y + dy - 8,
          ...(align === "left" ? { left: x + dx + 14 } : { right: 1920 - (x + dx) + 14 }),
        }}
      >
        {text}
      </div>
    </div>
  );
};
