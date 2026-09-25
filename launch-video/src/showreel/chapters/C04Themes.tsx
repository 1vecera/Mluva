import { AbsoluteFill, Easing, Img, random, staticFile, useCurrentFrame } from "remotion";
import { beatFrame } from "../timing";
import { C, display, easeIn, easeInOut, easeOut, LABEL, mono, ramp, T } from "../theme";
import { scramble } from "../parts/Scramble";
import { vBlur } from "../parts/Blur";

// Every theme installed with Omarchy, photographed from the production widget (showreel/capture_widget.py),
// sorted by the widget's background luminance from themes.json: light at the top of the sphere.
const LIGHT_TO_DARK = [
  "white", "flexoki-light", "lupine", "rose-pine", "catppuccin-latte", "everforest", "nord", "gruvbox", "ristretto",
  "lumon", "miasma", "kanagawa", "catppuccin", "tokyo-night", "osaka-jade", "retro-82", "solitude", "matte-black",
  "hackerman", "ethereal", "last-horizon", "vantablack",
];
// The counter names them from the hero's Vantablack outward.
const COUNT_ORDER = [...LIGHT_TO_DARK].reverse();
const TILES = LIGHT_TO_DARK.flatMap((theme) => [
  { theme, src: `showreel/themes/${theme}-recording.png` },
  { theme, src: `showreel/themes/${theme}-review.png` },
]);
const B0 = beatFrame(12);
const COLLAPSE = beatFrame(15) - B0;
const LENGTH = beatFrame(16) - B0;
const RADIUS = 360;
const TILE_W = 190;
const GOLDEN = Math.PI * (3 - Math.sqrt(5));
const COUNT_FROM = 2;
const COUNT_TO = 82;

const POINTS = TILES.map((_, i) => {
  const y = -1 + (2 * (i + 0.5)) / TILES.length;
  const r = Math.sqrt(1 - y * y);
  return { x: Math.cos(i * GOLDEN) * r, y, z: Math.sin(i * GOLDEN) * r };
});
const expoOut = Easing.out(Easing.exp);
const countAt = (frame: number) => Math.max(0, Math.min(22, ((frame - COUNT_FROM) / (COUNT_TO - COUNT_FROM)) * 22));
const namedAt = (theme: string) => COUNT_FROM + ((COUNT_ORDER.indexOf(theme) + 1) / 22) * (COUNT_TO - COUNT_FROM);

const Sphere: React.FC<{ frame: number }> = ({ frame }) => {
  const collapse = ramp(frame, COLLAPSE, LENGTH - 6, easeIn);
  const spin = 1.4 * (1 - expoOut(Math.min(1, frame / 26))) + 0.0045 * frame + 5 * collapse * collapse;
  const tilt = 0.3;
  const radius = RADIUS * (1 - collapse);
  const cx = 1250 - 290 * ramp(frame, COLLAPSE - 6, LENGTH - 8, easeInOut);
  const cy = 540;
  const placed = POINTS.map((p, i) => {
    const x1 = p.x * Math.cos(spin) + p.z * Math.sin(spin);
    const z1 = -p.x * Math.sin(spin) + p.z * Math.cos(spin);
    const y2 = p.y * Math.cos(tilt) - z1 * Math.sin(tilt);
    const z2 = p.y * Math.sin(tilt) + z1 * Math.cos(tilt);
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
          background: "radial-gradient(circle, rgba(233,27,39,0.09) 0%, rgba(233,27,39,0.03) 45%, transparent 70%)",
        }}
      />
      {placed
        .filter(({ z }) => z > -0.08)
        .map(({ i, x, y, z }) => {
          const tile = TILES[i];
          // Assemble from deep behind the sphere, staggered, with an exponential ease.
          const delay = Math.floor(random(`tile-${i}`) * 18);
          const a = expoOut(Math.min(1, Math.max(0, (frame - delay) / 22)));
          if (a <= 0) return null;
          const depthZ = z * radius * a - 1800 * (1 - a);
          const perspective = 1500 / (1500 - depthZ);
          const facing = (z + 1) / 2;
          const shade = 0.35 + 0.65 * Math.max(0, z);
          const edge = Math.min(1, (z + 0.08) / 0.25);
          const yaw = Math.atan2(x, z);
          const pitch = -Math.asin(Math.max(-1, Math.min(1, y)));
          const at = namedAt(tile.theme);
          const pop = ramp(frame, at, at + 3) * ramp(frame, at + 3, at + 11, (t) => t, 1, 0);
          const w = TILE_W * perspective * (1 - 0.6 * collapse) * (1 + 0.12 * pop);
          const h = w / 2;
          return (
            <div
              key={i}
              style={{
                position: "absolute",
                left: cx + x * radius * a * perspective - w / 2,
                top: cy + y * radius * a * perspective - h / 2,
                width: w,
                height: h,
                overflow: "hidden",
                transform: `perspective(900px) rotateY(${yaw}rad) rotateX(${pitch}rad)`,
                opacity: Math.min(1, a * 1.4) * edge * (1 - collapse * 0.6),
                filter: `brightness(${shade + 0.35 * pop})`,
                boxShadow: `0 ${12 * facing}px ${26 * facing}px rgba(0,0,0,0.6)`,
              }}
            >
              <Img src={staticFile(tile.src)} style={{ height: "100%", width: "auto", maxWidth: "none", display: "block" }} />
            </div>
          );
        })}
    </AbsoluteFill>
  );
};

// Digit strips masked with a soft gradient so glow and motion blur are not boxed in.
const Odometer: React.FC<{ value: number; size: number }> = ({ value, size }) => {
  const ones = value % 10;
  const tens = Math.floor(value / 10) + Math.max(0, ones - 9);
  const strip = (pos: number, key: string) => (
    <div
      key={key}
      style={{
        height: size,
        width: size * 0.62,
        WebkitMaskImage: "linear-gradient(transparent 0%, black 16%, black 84%, transparent 100%)",
      }}
    >
      <div style={{ transform: `translateY(${-pos * size}px)` }}>
        {Array.from({ length: 11 }, (_, d) => (
          <div key={d} style={{ ...display(size), height: size, lineHeight: `${size}px`, textAlign: "center" }}>
            {d % 10}
          </div>
        ))}
      </div>
    </div>
  );
  return (
    <div style={{ display: "flex", filter: "drop-shadow(0 0 22px rgba(255,255,255,0.1))" }}>
      {strip(tens, "t")}
      {strip(ones, "o")}
    </div>
  );
};

const Counter: React.FC<{ frame: number }> = ({ frame }) => {
  const enter = ramp(frame, 0, 10);
  const exit = ramp(frame, COLLAPSE, COLLAPSE + 10, easeIn);
  const value = countAt(frame);
  const speed = countAt(frame + 1) - value;
  const named = Math.max(1, Math.ceil(value));
  const name = COUNT_ORDER[named - 1].replace("-", " ").toUpperCase();
  const since = frame - namedAt(COUNT_ORDER[named - 1]);
  return (
    <div
      style={{
        position: "absolute",
        left: 128,
        top: 318,
        opacity: enter * (1 - exit),
        transform: `translateY(${(1 - enter) * 30 - exit * 40}px)`,
        filter: exit > 0 ? vBlur(exit * 30) : undefined,
      }}
    >
      <div style={mono(15, LABEL)}>
        <span style={{ display: "inline-block", width: 9, height: 9, background: C.red, marginRight: 14 }} />
        ONE WIDGET · EVERY THEME
      </div>
      <div style={{ marginTop: 20, filter: vBlur(Math.min(20, speed * 40)) }}>
        <Odometer value={value} size={T.xl} />
      </div>
      <div style={{ ...display(T.s, 800), letterSpacing: "-0.03em", marginTop: 6 }}>Omarchy themes</div>
      <div style={{ ...mono(15, C.ink), marginTop: 26 }}>
        <span style={{ color: C.red }}>● </span>
        {scramble(name, ramp(since, -1, 2), `t${named}`, frame, 0.6)}
      </div>
      <div style={{ ...mono(15, LABEL), marginTop: 14 }}>CAPTURED FROM THE REAL WIDGET</div>
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
  return (
    <AbsoluteFill style={{ opacity: ramp(frame, 0, 3, easeOut) }}>
      <Counter frame={frame} />
      <Sphere frame={frame} />
      <Point frame={frame} />
    </AbsoluteFill>
  );
};
