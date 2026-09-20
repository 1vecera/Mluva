import { loadFont } from "@remotion/fonts";
import { Easing, interpolate, staticFile } from "remotion";

export const FONT = "JetBrains Mono";

// JetBrains Mono is bundled with the app; the film uses the same four files.
for (const [file, weight] of [
  ["Regular", "400"],
  ["Medium", "500"],
  ["Bold", "700"],
] as const) {
  loadFont({
    family: FONT,
    url: staticFile(`fonts/JetBrainsMono-${file}.ttf`),
    weight,
  });
}
loadFont({
  family: FONT,
  url: staticFile("fonts/JetBrainsMono-Italic.ttf"),
  weight: "400",
  style: "italic",
});

export type Palette = {
  name: string;
  bg: string;
  deep: string;
  surface: string;
  selection: string;
  muted: string;
  fg: string;
  fgStrong: string;
  accent: string;
  border: string;
  frost: string;
  green: string;
  yellow: string;
  red: string;
  orange: string;
  purple: string;
};

// Omarchy theme palettes (from the video-kit tokens, sourced from Omarchy colors.toml files).
export const NORD: Palette = {
  name: "Nord",
  bg: "#2e3440",
  deep: "#191c23",
  surface: "#3b4252",
  selection: "#434c5e",
  muted: "#4c566a",
  fg: "#d8dee9",
  fgStrong: "#eceff4",
  accent: "#81a1c1",
  border: "#88c0d0",
  frost: "#88c0d0",
  green: "#a3be8c",
  yellow: "#ebcb8b",
  red: "#bf616a",
  orange: "#d08770",
  purple: "#b48ead",
};

export const TOKYO: Palette = {
  name: "Tokyo Night",
  bg: "#1a1b26",
  deep: "#13141c",
  surface: "#24283b",
  selection: "#33467c",
  muted: "#565f89",
  fg: "#c0caf5",
  fgStrong: "#e6eaff",
  accent: "#7aa2f7",
  border: "#7aa2f7",
  frost: "#7dcfff",
  green: "#9ece6a",
  yellow: "#e0af68",
  red: "#f7768e",
  orange: "#ff9e64",
  purple: "#bb9af7",
};

export const ROSE: Palette = {
  name: "Rosé Pine",
  bg: "#191724",
  deep: "#12101b",
  surface: "#1f1d2e",
  selection: "#403d52",
  muted: "#6e6a86",
  fg: "#e0def4",
  fgStrong: "#f6f4ff",
  accent: "#c4a7e7",
  border: "#ebbcba",
  frost: "#9ccfd8",
  green: "#3e8fb0",
  yellow: "#f6c177",
  red: "#eb6f92",
  orange: "#ea9a97",
  purple: "#c4a7e7",
};

export const EASE_OUT = Easing.bezier(0.16, 1, 0.3, 1);
export const EASE_IN_OUT = Easing.bezier(0.65, 0, 0.35, 1);
export const EASE_IN = Easing.bezier(0.7, 0, 0.84, 0);

/** Clamped tween in seconds: value goes from `out[0]` to `out[1]` between `range[0]` and `range[1]`. */
export const tween = (
  t: number,
  range: readonly [number, number],
  out: readonly [number, number],
  easing: (input: number) => number = EASE_OUT,
): number =>
  interpolate(t, range, out, {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing,
  });

/** Number of characters/words visible when revealing one item every `cadence` seconds from `start`. */
export const revealed = (t: number, start: number, cadence: number, total: number): number =>
  Math.max(0, Math.min(total, Math.floor((t - start) / cadence) + 1)) * (t >= start ? 1 : 0);

export const hexToRgba = (hex: string, alpha: number): string => {
  const h = hex.replace("#", "");
  const n = parseInt(h.length === 3 ? h.split("").map((c) => c + c).join("") : h, 16);
  return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${alpha})`;
};
