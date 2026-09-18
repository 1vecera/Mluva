import { FONT, hexToRgba, revealed, type Palette } from "../theme";

export const SCREEN_W = 1920;
export const SCREEN_H = 1080;
export const BAR_H = 40;
export const GAP = 12;

export type Rect = { x: number; y: number; w: number; h: number };

/** Tiled rectangle in the area below the bar. cols/rows split the area; col/row pick a cell; span widens. */
export const tile = (col: number, row: number, cols: number, rows: number, colSpan = 1, rowSpan = 1): Rect => {
  const areaX = GAP;
  const areaY = BAR_H + GAP;
  const areaW = SCREEN_W - GAP * 2;
  const areaH = SCREEN_H - BAR_H - GAP * 2;
  const cw = (areaW - GAP * (cols - 1)) / cols;
  const rh = (areaH - GAP * (rows - 1)) / rows;
  return {
    x: areaX + col * (cw + GAP),
    y: areaY + row * (rh + GAP),
    w: cw * colSpan + GAP * (colSpan - 1),
    h: rh * rowSpan + GAP * (rowSpan - 1),
  };
};

export const lerpRect = (a: Rect, b: Rect, p: number): Rect => ({
  x: a.x + (b.x - a.x) * p,
  y: a.y + (b.y - a.y) * p,
  w: a.w + (b.w - a.w) * p,
  h: a.h + (b.h - a.h) * p,
});

/** An Omarchy-style desktop: dark wallpaper, top bar with workspaces and a clock. */
export const DesktopChrome: React.FC<{
  palette: Palette;
  activeWorkspace?: number;
  title?: string;
  children: React.ReactNode;
}> = ({ palette, activeWorkspace = 2, title = "", children }) => (
  <div
    style={{
      position: "absolute",
      inset: 0,
      width: SCREEN_W,
      height: SCREEN_H,
      background: `radial-gradient(110% 80% at 50% 45%, ${palette.bg} 0%, ${palette.deep} 100%)`,
      fontFamily: FONT,
      color: palette.fg,
      overflow: "hidden",
    }}
  >
    <div
      style={{
        position: "absolute",
        left: 0,
        top: 0,
        width: SCREEN_W,
        height: BAR_H,
        display: "flex",
        alignItems: "center",
        padding: "0 18px",
        background: hexToRgba(palette.deep, 0.92),
        fontSize: 16,
        fontWeight: 500,
      }}
    >
      <div style={{ display: "flex", gap: 6 }}>
        {[1, 2, 3, 4, 5].map((n) => (
          <div
            key={n}
            style={{
              width: 26,
              height: 24,
              borderRadius: 6,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              background: n === activeWorkspace ? palette.accent : "transparent",
              color: n === activeWorkspace ? palette.deep : palette.muted,
            }}
          >
            {n}
          </div>
        ))}
      </div>
      <div style={{ flex: 1, textAlign: "center", color: palette.fg, opacity: 0.85 }}>{title}</div>
      <div style={{ display: "flex", gap: 18, color: palette.fg, opacity: 0.85 }}>
        <span>wifi</span>
        <span>92%</span>
        <span>14:32</span>
      </div>
    </div>
    {children}
  </div>
);

export const Window: React.FC<{
  rect: Rect;
  palette: Palette;
  title?: string;
  active?: boolean;
  opacity?: number;
  children?: React.ReactNode;
  contentStyle?: React.CSSProperties;
}> = ({ rect, palette, title, active = false, opacity = 1, children, contentStyle }) => (
  <div
    style={{
      position: "absolute",
      left: rect.x,
      top: rect.y,
      width: rect.w,
      height: rect.h,
      boxSizing: "border-box",
      border: `2px solid ${active ? palette.border : palette.muted}`,
      background: palette.bg,
      opacity,
      overflow: "hidden",
      display: "flex",
      flexDirection: "column",
      boxShadow: active ? `0 24px 60px ${hexToRgba("#000000", 0.35)}` : `0 12px 40px ${hexToRgba("#000000", 0.25)}`,
    }}
  >
    {title !== undefined ? (
      <div
        style={{
          height: 36,
          flex: "0 0 36px",
          display: "flex",
          alignItems: "center",
          padding: "0 14px",
          fontSize: 15,
          fontWeight: 500,
          color: palette.fg,
          background: palette.surface,
          borderBottom: `1px solid ${hexToRgba(palette.fg, 0.08)}`,
        }}
      >
        {title}
      </div>
    ) : null}
    <div style={{ flex: 1, position: "relative", overflow: "hidden", ...contentStyle }}>{children}</div>
  </div>
);

const Bar: React.FC<{ w: number | string; h?: number; color: string; style?: React.CSSProperties }> = ({ w, h = 12, color, style }) => (
  <div style={{ width: w, height: h, borderRadius: 6, background: color, ...style }} />
);

export const BrowserWindow: React.FC<{ rect: Rect; palette: Palette; active?: boolean; t: number }> = ({ rect, palette, active, t }) => (
  <Window rect={rect} palette={palette} active={active}>
    <div style={{ display: "flex", alignItems: "center", gap: 10, padding: "10px 14px", background: palette.surface }}>
      <div style={{ padding: "6px 14px", borderRadius: 8, background: palette.bg, fontSize: 15, color: palette.fg }}>Mluva · Docs</div>
      <div style={{ padding: "6px 14px", borderRadius: 8, fontSize: 15, color: palette.muted }}>Omarchy manual</div>
      <div style={{ flex: 1 }} />
    </div>
    <div style={{ margin: "10px 14px", padding: "9px 16px", borderRadius: 10, background: palette.deep, fontSize: 16, color: palette.fg, opacity: 0.9 }}>
      mluva.dev/docs/getting-started
    </div>
    <div style={{ padding: "26px 44px", display: "flex", flexDirection: "column", gap: 18, transform: `translateY(${-Math.min(40, t * 6)}px)` }}>
      <div style={{ fontSize: 34, fontWeight: 700, color: palette.fgStrong }}>Getting started</div>
      <div style={{ fontSize: 19, color: palette.fg, lineHeight: 1.5, maxWidth: 760 }}>
        Mluva turns speech into text that lands where you are typing. Press F9, talk, press F9 again.
      </div>
      <Bar w="88%" color={hexToRgba(palette.fg, 0.18)} />
      <Bar w="72%" color={hexToRgba(palette.fg, 0.18)} />
      <Bar w="80%" color={hexToRgba(palette.fg, 0.18)} />
      <div style={{ marginTop: 12, padding: "16px 20px", borderRadius: 10, background: palette.deep, color: palette.green, fontSize: 17 }}>
        $ omarchy-mluva install
      </div>
      <Bar w="64%" color={hexToRgba(palette.fg, 0.18)} />
      <Bar w="76%" color={hexToRgba(palette.fg, 0.18)} />
    </div>
  </Window>
);

export const TerminalWindow: React.FC<{ rect: Rect; palette: Palette; active?: boolean; t: number; startAt?: number }> = ({
  rect,
  palette,
  active,
  t,
  startAt = 0.2,
}) => {
  const lines = [
    { kind: "cmd", text: "make test" },
    { kind: "out", text: "ruff check ......................... ok" },
    { kind: "out", text: "pytest  440 passed in 6.2s" },
    { kind: "ok", text: "✓ workspace gate passed" },
    { kind: "cmd", text: "git switch -c feat/live-rewrite" },
    { kind: "out", text: "Switched to a new branch 'feat/live-rewrite'" },
  ];
  const shown = revealed(t, startAt, 0.55, lines.length);
  return (
    <Window rect={rect} palette={palette} active={active} title="ghostty — ~/code/mluva">
      <div style={{ padding: "16px 20px", fontSize: 18, lineHeight: 1.65, color: palette.fg }}>
        {lines.slice(0, shown).map((l, i) => (
          <div key={i} style={{ color: l.kind === "ok" ? palette.green : l.kind === "cmd" ? palette.fgStrong : palette.fg, opacity: l.kind === "out" ? 0.8 : 1 }}>
            {l.kind === "cmd" ? (
              <>
                <span style={{ color: palette.accent }}>~/code/mluva </span>
                <span style={{ color: palette.frost }}>❯ </span>
              </>
            ) : null}
            {l.text}
          </div>
        ))}
        <div style={{ display: "flex" }}>
          <span style={{ color: palette.accent }}>~/code/mluva </span>
          <span style={{ color: palette.frost }}>❯ </span>
          <span style={{ width: 11, height: 22, marginTop: 4, background: palette.fg, opacity: Math.floor(t * 2) % 2 === 0 ? 1 : 0 }} />
        </div>
      </div>
    </Window>
  );
};

export const HerdrWindow: React.FC<{ rect: Rect; palette: Palette; active?: boolean; t: number }> = ({ rect, palette, active, t }) => {
  const runs = [
    { agent: "Codex Lenovo", task: "S27-483 · prompt editing", done: 1 },
    { agent: "Claude Claw Mini", task: "Logo study · vector pass", done: 0.62 + 0.03 * Math.sin(t) },
    { agent: "Codex Claw Mini", task: "Omarchy VM captures", done: Math.min(1, 0.3 + t * 0.08) },
  ];
  return (
    <Window rect={rect} palette={palette} active={active} title="Herdr — runs">
      <div style={{ padding: "14px 18px", display: "flex", flexDirection: "column", gap: 14 }}>
        {runs.map((r, i) => (
          <div key={i} style={{ display: "flex", flexDirection: "column", gap: 8, padding: "12px 14px", borderRadius: 10, background: palette.surface }}>
            <div style={{ display: "flex", justifyContent: "space-between", fontSize: 16 }}>
              <span style={{ color: palette.fgStrong, fontWeight: 500 }}>{r.agent}</span>
              <span style={{ color: r.done >= 1 ? palette.green : palette.yellow }}>{r.done >= 1 ? "done" : "running"}</span>
            </div>
            <div style={{ fontSize: 15, color: palette.fg, opacity: 0.8 }}>{r.task}</div>
            <div style={{ height: 6, borderRadius: 3, background: palette.deep, overflow: "hidden" }}>
              <div style={{ width: `${Math.round(r.done * 100)}%`, height: "100%", background: r.done >= 1 ? palette.green : palette.accent }} />
            </div>
          </div>
        ))}
      </div>
    </Window>
  );
};
