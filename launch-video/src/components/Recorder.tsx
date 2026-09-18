import { FONT, hexToRgba, type Palette } from "../theme";

const pad = (n: number) => String(Math.max(0, Math.floor(n))).padStart(2, "0");

/**
 * The Mluva recorder. Speech appears immediately at full opacity, word by word, exactly like the
 * app: no fade, no per-word animation. The breathing light follows the 3.4 s cycle from the design kit.
 */
export const Recorder: React.FC<{
  width: number;
  palette: Palette;
  text: string;
  visibleWords: number;
  state: "idle" | "recording" | "ready";
  seconds: number;
  t: number;
  lines?: number;
  fontSize?: number;
}> = ({ width, palette, text, visibleWords, state, seconds, t, lines = 5, fontSize = 24 }) => {
  const words = text.split(" ");
  const shown = words.slice(0, Math.max(0, Math.min(words.length, visibleWords))).join(" ");
  const breathe = state === "recording" ? 0.5 + 0.5 * Math.sin((t / 3.4) * Math.PI * 2) : 0;
  const dot = 12 + breathe * 6;
  const lineHeight = fontSize * 1.4;
  return (
    <div
      style={{
        width,
        fontFamily: FONT,
        background: palette.bg,
        border: `2px solid ${state === "recording" ? palette.border : palette.muted}`,
        borderRadius: 14,
        overflow: "hidden",
        boxSizing: "border-box",
        color: palette.fg,
        boxShadow: `0 30px 70px ${hexToRgba("#000000", 0.4)}`,
      }}
    >
      <div style={{ height: 44, display: "flex", alignItems: "center", padding: "0 16px", gap: 12, fontSize: 16, fontWeight: 500 }}>
        <div style={{ width: 22, display: "flex", justifyContent: "center", alignItems: "center" }}>
          <div
            style={{
              width: dot,
              height: dot,
              borderRadius: dot,
              background: state === "recording" ? palette.frost : state === "ready" ? palette.green : palette.muted,
              boxShadow: state === "recording" ? `0 0 ${14 + breathe * 16}px ${hexToRgba(palette.frost, 0.7)}` : "none",
            }}
          />
        </div>
        <div style={{ color: palette.fgStrong }}>{state === "recording" ? "Recording" : state === "ready" ? "Ready" : "Mluva"}</div>
        <div style={{ flex: 1 }} />
        <div style={{ color: palette.fg, opacity: 0.8, fontVariantNumeric: "tabular-nums" }}>
          {pad(seconds / 60)}:{pad(seconds % 60)}
        </div>
        <div style={{ color: palette.muted }}>{state === "recording" ? "F9 to stop" : "F9 to start"}</div>
      </div>
      <div
        style={{
          margin: "0 10px 10px",
          padding: 14,
          height: lineHeight * lines + 28,
          boxSizing: "border-box",
          background: hexToRgba(palette.deep, 0.82),
          fontSize,
          lineHeight: `${lineHeight}px`,
          color: palette.fgStrong,
          overflow: "hidden",
          wordBreak: "break-word",
        }}
      >
        {shown}
        {state === "recording" ? <span style={{ display: "inline-block", width: 3, height: fontSize, background: palette.frost, marginLeft: 4, verticalAlign: "middle" }} /> : null}
      </div>
      {state === "ready" ? (
        <div style={{ display: "flex", gap: 10, padding: "0 10px 12px", fontSize: 15 }}>
          {["Polish", "Structure", "More", "Open"].map((b, i) => (
            <div
              key={b}
              style={{
                padding: "7px 14px",
                borderRadius: 8,
                background: i === 0 ? palette.accent : palette.surface,
                color: i === 0 ? palette.deep : palette.fg,
                fontWeight: 500,
              }}
            >
              {b}
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
};
