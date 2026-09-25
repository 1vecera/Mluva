import { AbsoluteFill, Img, staticFile, useCurrentFrame } from "remotion";
import { beatFrame } from "../timing";
import { C, display, easeIn, easeInOut, easeOut, mono, ramp } from "../theme";
import { scramble } from "../parts/Scramble";
import { vBlur } from "../parts/Blur";

// Every theme installed with Omarchy, photographed from the production widget (showreel/capture_widget.py).
const THEMES = [
  "catppuccin", "catppuccin-latte", "ethereal", "everforest", "flexoki-light", "gruvbox", "hackerman", "kanagawa",
  "last-horizon", "lumon", "lupine", "matte-black", "miasma", "nord", "osaka-jade", "retro-82", "ristretto",
  "rose-pine", "solitude", "tokyo-night", "vantablack", "white",
];
const STILLS = THEMES.flatMap((theme) => [
  { src: `showreel/themes/${theme}-recording.png`, aspect: 225 / 750 },
  { src: `showreel/themes/${theme}-review.png`, aspect: 281 / 750 },
]);
// Each still appears twice, spread apart on the sphere so neighbours differ.
const CARDS = [...STILLS, ...STILLS.slice(22), ...STILLS.slice(0, 22)];
const B0 = beatFrame(12);
const COLLAPSE = beatFrame(15) - B0;
const LENGTH = beatFrame(16) - B0;
const RADIUS = 360;
const CARD_W = 172;
const GOLDEN = Math.PI * (3 - Math.sqrt(5));

const POINTS = CARDS.map((_, i) => {
  const y = 1 - (2 * (i + 0.5)) / CARDS.length;
  const r = Math.sqrt(1 - y * y);
  const phi = i * GOLDEN;
  return { x: Math.cos(phi) * r, y, z: Math.sin(phi) * r };
});

const Sphere: React.FC<{ frame: number }> = ({ frame }) => {
  const enter = ramp(frame, 0, 18, easeOut);
  const zoom = 1 + 0.35 * (1 - ramp(frame, 0, 14, easeOut));
  const collapse = ramp(frame, COLLAPSE, LENGTH - 6, easeIn);
  const spin = 2.6 * ramp(frame, 0, 70, easeOut) + 0.012 * frame + 5 * collapse * collapse;
  const tilt = 0.32;
  const radius = RADIUS * zoom * (0.8 + 0.2 * enter) * (1 - collapse);
  const cx = 1250 - 290 * ramp(frame, COLLAPSE - 6, LENGTH - 8, easeInOut);
  const cy = 540;
  const cosY = Math.cos(spin);
  const sinY = Math.sin(spin);
  const cosX = Math.cos(tilt);
  const sinX = Math.sin(tilt);
  const placed = POINTS.map((p, i) => {
    const x1 = p.x * cosY + p.z * sinY;
    const z1 = -p.x * sinY + p.z * cosY;
    const y2 = p.y * cosX - z1 * sinX;
    const z2 = p.y * sinX + z1 * cosX;
    return { i, x: x1, y: y2, z: z2 };
  }).sort((a, b) => a.z - b.z);
  return (
    <AbsoluteFill>
      <div
        style={{
          position: "absolute",
          left: cx - radius * 1.25,
          top: cy - radius * 1.25,
          width: radius * 2.5,
          height: radius * 2.5,
          borderRadius: "50%",
          background: "radial-gradient(circle, rgba(233,27,39,0.10) 0%, rgba(233,27,39,0.04) 45%, transparent 70%)",
          opacity: enter,
        }}
      />
      {placed.filter(({ z }) => z > -0.08).map(({ i, x, y, z }) => {
        const card = CARDS[i];
        const depth = (z + 1) / 2;
        const perspective = 1500 / (1500 - z * radius);
        const yaw = Math.atan2(x, z);
        const pitch = -Math.asin(Math.max(-1, Math.min(1, y)));
        const appear = ramp(frame, (i % 11) * 0.5, 6 + (i % 11) * 0.5);
        const w = CARD_W * perspective * (1 - 0.6 * collapse);
        const edge = Math.min(1, (z + 0.08) / 0.25);
        return (
          <div
            key={i}
            style={{
              position: "absolute",
              left: cx + x * radius * perspective - w / 2,
              top: cy + y * radius * perspective - (w * card.aspect) / 2,
              width: w,
              height: w * card.aspect,
              transform: `perspective(900px) rotateY(${yaw}rad) rotateX(${pitch}rad)`,
              opacity: appear * edge * (0.45 + 0.55 * depth) * (1 - collapse * 0.6),
              filter: `brightness(${0.5 + 0.7 * depth})`,
              boxShadow: `0 ${12 * depth}px ${26 * depth}px rgba(0,0,0,0.6)`,
            }}
          >
            <Img src={staticFile(card.src)} style={{ width: "100%", height: "100%", display: "block" }} />
          </div>
        );
      })}
    </AbsoluteFill>
  );
};

const Odometer: React.FC<{ value: number; size: number }> = ({ value, size }) => {
  const digitH = size * 1.0;
  const ones = value % 10;
  const tensBase = Math.floor(value / 10);
  const carry = Math.max(0, (ones - 9) / 1);
  const tens = tensBase + carry;
  const strip = (pos: number, key: string) => (
    <div key={key} style={{ height: digitH, overflow: "hidden", width: size * 0.62 }}>
      <div style={{ transform: `translateY(${-pos * digitH}px)`, filter: vBlur(0) }}>
        {Array.from({ length: 11 }, (_, d) => (
          <div key={d} style={{ ...display(size), height: digitH, lineHeight: `${digitH}px`, textAlign: "center" }}>
            {d % 10}
          </div>
        ))}
      </div>
    </div>
  );
  return (
    <div style={{ display: "flex", textShadow: "0 0 30px rgba(255,255,255,0.2)" }}>
      {strip(tens, "t")}
      {strip(ones, "o")}
    </div>
  );
};

const Counter: React.FC<{ frame: number }> = ({ frame }) => {
  const enter = ramp(frame, 2, 14);
  const exit = ramp(frame, COLLAPSE, COLLAPSE + 10, easeIn);
  const value = 22 * ramp(frame, 6, 64, easeOut);
  const speed = 22 * (ramp(frame + 1, 6, 64, easeOut) - ramp(frame, 6, 64, easeOut));
  const name = THEMES[Math.floor(frame / 5) % THEMES.length].replace("-", " ").toUpperCase();
  return (
    <div
      style={{
        position: "absolute",
        left: 128,
        top: 318,
        opacity: enter * (1 - exit),
        transform: `translateY(${(1 - enter) * 30 - exit * 40}px)`,
        filter: vBlur(exit * 30),
      }}
    >
      <div style={{ ...mono(14, C.ink2) }}>
        <span style={{ display: "inline-block", width: 9, height: 9, background: C.red, marginRight: 14 }} />
        ONE WIDGET · EVERY THEME
      </div>
      <div style={{ marginTop: 20, filter: vBlur(Math.min(30, speed * 60)) }}>
        <Odometer value={value} size={220} />
      </div>
      <div style={{ ...display(66, 800), letterSpacing: "-0.03em", marginTop: 8 }}>Omarchy themes</div>
      <div style={{ ...mono(15, C.ink3), marginTop: 26 }}>
        <span style={{ color: C.red }}>● </span>+ {scramble(name, ramp(frame % 5, 0, 3), `t${Math.floor(frame / 5)}`, frame, 0.6)}
      </div>
      <div style={{ ...mono(12, C.ink3), marginTop: 14 }}>CAPTURED FROM THE REAL WIDGET</div>
    </div>
  );
};

const Point: React.FC<{ frame: number }> = ({ frame }) => {
  const on = ramp(frame, LENGTH - 14, LENGTH - 4, easeOut);
  if (on <= 0) return null;
  const s = on * (1 + 0.5 * ramp(frame, LENGTH - 5, LENGTH, easeIn));
  return (
    <div
      style={{
        position: "absolute",
        left: 960 - 30,
        top: 540 - 30,
        width: 60,
        height: 60,
        borderRadius: "50%",
        background: "radial-gradient(circle, #fff 0%, #ff8a90 25%, #E91B27 50%, rgba(233,27,39,0) 72%)",
        transform: `scale(${s})`,
        boxShadow: `0 0 ${60 * on}px rgba(233,27,39,0.9)`,
      }}
    />
  );
};

export const C04Themes: React.FC = () => {
  const frame = useCurrentFrame();
  const enter = ramp(frame, 0, 5, easeOut);
  return (
    <AbsoluteFill style={{ opacity: enter }}>
      <Counter frame={frame} />
      <Sphere frame={frame} />
      <Point frame={frame} />
    </AbsoluteFill>
  );
};
