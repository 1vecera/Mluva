import { AbsoluteFill, Easing, Img, interpolate, random, staticFile, useCurrentFrame } from "remotion";
import { beatFrame } from "../timing";
import { C, display, easeIn, easeInOut, easeOut, LABEL, mono, ramp, T } from "../theme";
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
const COLLAPSE = beatFrame(15) - B0 + 6;
const LENGTH = beatFrame(16) - B0;
const RADIUS = 360;
const TILE_W = 190;
const GOLDEN = Math.PI * (3 - Math.sqrt(5));
const COUNT_FROM = 0;
const COUNT_TO = 72;

const POINTS = TILES.map((_, i) => {
  const y = -1 + (2 * (i + 0.5)) / TILES.length;
  const r = Math.sqrt(1 - y * y);
  return { x: Math.cos(i * GOLDEN) * r, y, z: Math.sin(i * GOLDEN) * r };
});
const expoOut = Easing.out(Easing.exp);
// The count eases from 1 to 22: fast at first, then each theme holds long enough to read.
const eased = (frame: number) =>
  interpolate(frame, [COUNT_FROM, COUNT_TO], [1, 22], {
    easing: Easing.out(Easing.cubic),
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });
const countAt = (frame: number) => (frame < COUNT_FROM ? 0 : Math.round(eased(frame)));
// First frame at which the count reaches k.
const STEP_FRAMES = Array.from({ length: 23 }, (_, k) => {
  for (let f = 0; f <= COUNT_TO; f++) if (countAt(f) >= k) return f;
  return COUNT_TO;
});
const namedAt = (theme: string) => STEP_FRAMES[COUNT_ORDER.indexOf(theme) + 1];
// One highlight sweeps across the sphere on each beat of the hold.
const SWEEPS = [beatFrame(13) - B0, beatFrame(14) - B0];

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
          const delay = Math.floor(random(`tile-${i}`) * 10);
          const a = expoOut(Math.min(1, Math.max(0, (frame + 2 - delay) / 16)));
          if (a <= 0) return null;
          const depthZ = z * radius * a - 1800 * (1 - a);
          const perspective = 1500 / (1500 - depthZ);
          const facing = (z + 1) / 2;
          const screenX = cx + x * radius * a * perspective;
          const sweep = SWEEPS.reduce((sum, at) => {
            const t = (frame - at) / 16;
            if (t < 0 || t > 1) return sum;
            const bandX = cx - radius * 1.3 + t * radius * 2.6;
            return sum + Math.exp(-(((screenX - bandX) / 70) ** 2));
          }, 0);
          const shade = 0.4 + 0.7 * Math.max(0, z) + 0.35 * sweep;
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

// Integer digits: each change slides the new number up 18 % of its size over three frames.
const Count: React.FC<{ frame: number; size: number }> = ({ frame, size }) => {
  const value = countAt(frame);
  const since = value > 0 ? frame - STEP_FRAMES[value] : 99;
  const slide = 1 - easeOut(Math.min(1, since / 3));
  const pop = value === 22 ? 1 + 0.06 * (1 - expoOut(Math.min(1, (frame - STEP_FRAMES[22]) / 10))) : 1;
  return (
    <div
      style={{
        ...display(size),
        height: size,
        lineHeight: `${size}px`,
        transform: `translateY(${slide * size * 0.18}px) scale(${pop})`,
        transformOrigin: "0% 60%",
        opacity: 0.35 + 0.65 * (1 - slide),
        filter: "drop-shadow(0 0 22px rgba(255,255,255,0.1))",
        fontVariantNumeric: "tabular-nums",
      }}
    >
      {String(value).padStart(2, "0")}
    </div>
  );
};

// A short departures-style list: each theme name scrolls through three rows as it is counted.
// `value` is the eased, continuous count, so the list glides between names.
const ROW = 26;
const ThemeList: React.FC<{ value: number }> = ({ value }) => (
  <div
    style={{
      position: "relative",
      height: ROW * 3,
      marginTop: 18,
      overflow: "hidden",
      WebkitMaskImage: "linear-gradient(transparent 0%, black 30%, black 70%, transparent 100%)",
    }}
  >
    <div style={{ transform: `translateY(${ROW - (value - 1) * ROW}px)` }}>
      {COUNT_ORDER.map((theme, i) => {
        const near = 1 - Math.min(1, Math.abs(value - 1 - i));
        return (
          <div key={theme} style={{ ...mono(15, near > 0.5 ? C.ink : LABEL), height: ROW, lineHeight: `${ROW}px`, opacity: 0.45 + 0.55 * near }}>
            <span style={{ color: near > 0.5 ? C.red : "transparent" }}>● </span>
            {String(i + 1).padStart(2, "0")} {theme.replace("-", " ").toUpperCase()}
          </div>
        );
      })}
    </div>
  </div>
);

const Counter: React.FC<{ frame: number }> = ({ frame }) => {
  const enter = ramp(frame, 0, 10);
  const exit = ramp(frame, COLLAPSE, COLLAPSE + 10, easeIn);
  const glide = frame < COUNT_FROM ? 1 : eased(frame);
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
      <div style={{ marginTop: 20 }}>
        <Count frame={frame} size={T.xl} />
      </div>
      <div style={{ ...display(T.s, 800), letterSpacing: "-0.03em", marginTop: 6 }}>Omarchy themes</div>
      <ThemeList value={glide} />
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
