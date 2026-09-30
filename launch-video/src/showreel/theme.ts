import { Easing, interpolate, staticFile } from "remotion";
import { loadFont } from "@remotion/fonts";

loadFont({ family: "Adwaita Sans", url: staticFile("fonts/AdwaitaSans-Regular.ttf"), weight: "100 900" });
loadFont({ family: "JetBrains Mono", url: staticFile("fonts/JetBrainsMono-Regular.ttf"), weight: "400" });
// Static Black instance with overlaps removed (showreel/prepare.py), for outlined type only.
loadFont({ family: "Adwaita Sans Outline", url: staticFile("showreel/fonts/AdwaitaSans-Black-NoOverlap.ttf"), weight: "900" });

// Mluva identity: black surface, #F5F5F5 ink, one red accent (docs/brand).
export const C = {
  bg: "#050505",
  ink: "#F5F5F5",
  ink2: "#A3A3A3",
  ink3: "#6A6A6A",
  ink4: "#262626",
  line: "rgba(245,245,245,0.14)",
  red: "#E91B27",
  redGlow: "rgba(233,27,39,0.55)",
};

export const DISPLAY = "'Adwaita Sans', sans-serif";
export const OUTLINE = "'Adwaita Sans Outline', 'Adwaita Sans', sans-serif";

// One type scale for display text (px at 1080p).
export const T = { xl: 200, l: 150, m: 104, s: 64, xs: 38 };
// Labels stay readable: at least 15 px mono in a mid grey.
export const LABEL = "#8C8C8C";
// A restrained bloom for white display type.
export const BLOOM = "0 0 28px rgba(255,255,255,0.1)";
export const MONO = "'JetBrains Mono', monospace";

export const mono = (size = 15, color = C.ink2): React.CSSProperties => ({
  fontFamily: MONO,
  fontSize: size,
  letterSpacing: "0.2em",
  textTransform: "uppercase",
  color,
  whiteSpace: "nowrap",
});

export const outline = (size: number, color = "rgba(245,245,245,0.9)", width = 2): React.CSSProperties => ({
  fontFamily: OUTLINE,
  fontSize: size,
  fontWeight: 900,
  letterSpacing: "-0.035em",
  lineHeight: 1,
  color: "transparent",
  WebkitTextStroke: `${width}px ${color}`,
  whiteSpace: "nowrap",
});

export const display = (size: number, weight = 900): React.CSSProperties => ({
  fontFamily: DISPLAY,
  fontSize: size,
  fontWeight: weight,
  letterSpacing: "-0.035em",
  lineHeight: 1,
  color: C.ink,
  whiteSpace: "nowrap",
});

export const brand = (name: string) => staticFile(`showreel/brand/${name}`);

export const easeOut = Easing.bezier(0.16, 1, 0.3, 1);
export const easeIn = Easing.bezier(0.7, 0, 0.84, 0);
export const easeInOut = Easing.bezier(0.65, 0, 0.35, 1);

export const ramp = (
  frame: number,
  from: number,
  to: number,
  easing: (t: number) => number = easeOut,
  outFrom = 0,
  outTo = 1,
) =>
  interpolate(frame, [from, to], [outFrom, outTo], {
    easing,
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });

// Damped oscillation for jelly and overshoot: 1 -> 0 while ringing.
export const ring = (t: number, frequency = 3.2, damping = 5.5) =>
  t < 0 ? 0 : Math.exp(-damping * t) * Math.sin(2 * Math.PI * frequency * t);

export const clamp01 = (v: number) => Math.min(1, Math.max(0, v));
