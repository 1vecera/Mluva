import { FONT, hexToRgba, tween, type Palette } from "../theme";

export type HistoryItem = { title: string; date: string };

export const HISTORY: HistoryItem[] = [
  { title: "Could we move the review to Tuesday?", date: "Today · 14:31" },
  { title: "Notes export requirements", date: "Today · 11:08" },
  { title: "Standup: recommender rollout", date: "Yesterday" },
  { title: "Grocery list for the weekend", date: "Mon" },
  { title: "Ideas for the launch film", date: "Sun" },
];

const Caret: React.FC<{ t: number; color: string; height?: number }> = ({ t, color, height = 22 }) => (
  <span style={{ display: "inline-block", width: 2, height, background: color, marginLeft: 3, verticalAlign: "text-bottom", opacity: Math.floor(t * 2.2) % 2 === 0 ? 1 : 0 }} />
);

/**
 * The full Mluva window: History sidebar, note header, one or two reading panes and a status bar.
 * Sized by the caller; everything inside uses the given palette so a theme switch recolors it whole.
 */
export const Workspace: React.FC<{
  width: number;
  height: number;
  palette: Palette;
  t: number;
  query?: string;
  items?: HistoryItem[];
  selected?: number;
  itemOpacities?: number[];
  noteTitle?: string;
  noteMeta?: string;
  original: React.ReactNode;
  draft?: React.ReactNode;
  draftTitle?: string;
  badge?: string;
  statusLeft?: string;
  statusRight?: string;
  chips?: string[];
  chipsAt?: number;
  composer?: boolean;
  fontSize?: number;
}> = ({
  width,
  height,
  palette,
  t,
  query = "",
  items = HISTORY,
  selected = 0,
  itemOpacities,
  noteTitle = "Could we move the review to Tuesday?",
  noteMeta = "Today · 14:31 · Whisper (local)",
  original,
  draft,
  draftTitle = "Live draft",
  badge,
  statusLeft = "Dictation ready. Automatic copying is off.",
  statusRight = "Ctrl+P Commands",
  chips,
  chipsAt = 0,
  composer = false,
  fontSize = 22,
}) => {
  const sidebarW = Math.round(width * 0.19);
  const badgePulse = 0.5 + 0.5 * Math.sin((t / 1.6) * Math.PI * 2);
  return (
    <div
      style={{
        width,
        height,
        display: "flex",
        fontFamily: FONT,
        color: palette.fg,
        background: palette.bg,
        boxSizing: "border-box",
        overflow: "hidden",
      }}
    >
      <div style={{ width: sidebarW, flex: `0 0 ${sidebarW}px`, background: palette.deep, display: "flex", flexDirection: "column", borderRight: `1px solid ${hexToRgba(palette.fg, 0.08)}` }}>
        <div style={{ padding: "18px 18px 10px", fontSize: 17, fontWeight: 600, color: palette.fgStrong }}>Mluva</div>
        <div style={{ margin: "0 14px 12px", padding: "10px 14px", borderRadius: 8, background: palette.surface, fontSize: 16, color: palette.fgStrong, fontWeight: 500 }}>New conversation</div>
        <div style={{ margin: "0 14px 12px", padding: "10px 14px", borderRadius: 8, background: palette.bg, border: `1px solid ${query ? palette.accent : hexToRgba(palette.fg, 0.1)}`, fontSize: 17, color: query ? palette.fgStrong : palette.muted, display: "flex", alignItems: "center" }}>
          {query || "Search history"}
          {query ? <Caret t={t} color={palette.accent} height={18} /> : null}
        </div>
        <div style={{ display: "flex", flexDirection: "column", gap: 4, padding: "0 10px" }}>
          {items.map((item, i) => (
            <div
              key={item.title}
              style={{
                padding: "10px 12px",
                paddingLeft: 9,
                borderLeft: `3px solid ${i === selected ? palette.accent : "transparent"}`,
                borderRadius: 8,
                background: i === selected ? palette.selection : "transparent",
                opacity: itemOpacities?.[i] ?? 1,
                display: "flex",
                flexDirection: "column",
                gap: 4,
              }}
            >
              <div style={{ fontSize: 15, color: i === selected ? palette.fgStrong : palette.fg, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{item.title}</div>
              <div style={{ fontSize: 13, color: palette.muted }}>{item.date}</div>
            </div>
          ))}
        </div>
        <div style={{ flex: 1 }} />
        <div style={{ padding: "10px 24px", fontSize: 15, color: palette.fg }}>Show more</div>
        <div style={{ padding: "0 24px 18px", fontSize: 15, color: palette.muted }}>Manage history</div>
      </div>
      <div style={{ flex: 1, display: "flex", flexDirection: "column", minWidth: 0 }}>
        <div style={{ height: 64, flex: "0 0 64px", display: "flex", alignItems: "center", padding: "0 28px", gap: 16, borderBottom: `1px solid ${hexToRgba(palette.fg, 0.08)}` }}>
          <div style={{ fontSize: 20, fontWeight: 500, color: palette.fgStrong, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{noteTitle}</div>
          <div style={{ fontSize: 15, color: palette.muted, whiteSpace: "nowrap" }}>{noteMeta}</div>
          <div style={{ flex: 1 }} />
          {chips?.map((c, i) => {
            const cp = tween(t, [chipsAt + i * 0.08, chipsAt + 0.2 + i * 0.08], [0, 1]);
            return (
              <div key={c} style={{ padding: "5px 12px", borderRadius: 999, fontSize: 14, background: i === chips.length - 1 ? palette.accent : palette.surface, color: i === chips.length - 1 ? palette.deep : palette.fg, opacity: cp, transform: `scale(${0.85 + 0.15 * cp})` }}>
                {c}
              </div>
            );
          })}
          {badge ? (
            <div style={{ padding: "5px 12px", borderRadius: 999, fontSize: 14, background: hexToRgba(palette.yellow, 0.18), color: palette.yellow, border: `1px solid ${hexToRgba(palette.yellow, 0.5)}`, boxShadow: `0 0 ${10 + 8 * badgePulse}px ${hexToRgba(palette.yellow, 0.25 + 0.2 * badgePulse)}` }}>{badge}</div>
          ) : null}
        </div>
        <div style={{ flex: 1, display: "flex", minHeight: 0 }}>
          <div style={{ flex: 1, padding: "26px 32px", fontSize, lineHeight: 1.55, color: palette.fgStrong, minWidth: 0 }}>
            {draft ? <div style={{ display: "flex", alignItems: "center", fontSize: 14, color: palette.muted, marginBottom: 14, letterSpacing: 1 }}><span>ORIGINAL</span><div style={{ flex: 1 }} /><span style={{ padding: "3px 12px", borderRadius: 999, border: `1px solid ${hexToRgba(palette.fg, 0.16)}`, letterSpacing: 0 }}>Copy</span></div> : null}
            {original}
          </div>
          {draft ? (
            <>
              <div style={{ width: 1, background: hexToRgba(palette.fg, 0.16) }} />
              <div style={{ flex: 1, padding: "26px 32px", fontSize, lineHeight: 1.55, color: palette.fgStrong, background: hexToRgba(palette.fg, 0.025), minWidth: 0 }}>
                <div style={{ display: "flex", alignItems: "center", fontSize: 14, color: palette.muted, marginBottom: 14, letterSpacing: 1 }}><span>{draftTitle.toUpperCase()}</span><div style={{ flex: 1 }} /><span style={{ padding: "3px 12px", borderRadius: 999, border: `1px solid ${hexToRgba(palette.fg, 0.16)}`, letterSpacing: 0 }}>Copy</span></div>
                {draft}
              </div>
            </>
          ) : null}
        </div>
        {composer ? (
          <div style={{ padding: "12px 28px 0", display: "flex", flexDirection: "column", gap: 10 }}>
            <div style={{ display: "flex", gap: 22, alignItems: "center", fontSize: 17, color: palette.fg }}>
              <span style={{ color: palette.accent, fontWeight: 600 }}>Polish</span>
              <span>Structure</span>
              <span>More ▾</span>
            </div>
            <div style={{ display: "flex", gap: 12 }}>
              <div style={{ flex: 1, padding: "12px 16px", borderRadius: 10, border: `1px solid ${hexToRgba(palette.fg, 0.14)}`, color: palette.muted, fontSize: 17 }}>Ask for a rewrite…</div>
              <div style={{ padding: "12px 22px", borderRadius: 10, background: palette.accent, color: palette.deep, fontWeight: 600, fontSize: 17 }}>Rewrite</div>
              <div style={{ padding: "12px 22px", borderRadius: 10, border: `1px solid ${hexToRgba(palette.accent, 0.6)}`, color: palette.accent, fontSize: 17 }}>Dictate</div>
            </div>
          </div>
        ) : null}
        <div style={{ height: 34, flex: "0 0 34px", display: "flex", alignItems: "center", padding: "0 20px", fontSize: 14, color: palette.muted, borderTop: `1px solid ${hexToRgba(palette.fg, 0.08)}` }}>
          <span>{statusLeft}</span>
          <div style={{ flex: 1 }} />
          <span>{statusRight}</span>
        </div>
      </div>
    </div>
  );
};

/** Command surface opened with Ctrl+P. `visible` is 0..1. */
export const CommandPalette: React.FC<{
  visible: number;
  items: string[];
  selected: number;
  query?: string;
  palette: Palette;
  width?: number;
  top?: number;
}> = ({ visible, items, selected, query = "", palette, width = 640, top = 90 }) => (
  <div
    style={{
      position: "absolute",
      left: "50%",
      top,
      width,
      transform: `translateX(-50%) translateY(${(1 - visible) * -14}px) scale(${0.97 + 0.03 * visible})`,
      opacity: visible,
      background: hexToRgba(palette.deep, 0.9),
      border: `1px solid ${hexToRgba(palette.fgStrong, 0.12)}`,
      borderRadius: 14,
      boxShadow: `0 30px 80px ${hexToRgba("#000000", 0.5)}`,
      fontFamily: FONT,
      overflow: "hidden",
    }}
  >
    <div style={{ padding: "16px 20px", fontSize: 20, color: query ? palette.fgStrong : palette.muted, borderBottom: `1px solid ${hexToRgba(palette.fg, 0.1)}` }}>{query || "Type a command"}</div>
    <div style={{ padding: 8 }}>
      {items.map((item, i) => (
        <div
          key={item}
          style={{
            display: "flex",
            alignItems: "center",
            gap: 12,
            padding: "10px 14px",
            borderRadius: 8,
            fontSize: 18,
            background: i === selected ? palette.selection : "transparent",
            color: i === selected ? palette.fgStrong : palette.fg,
          }}
        >
          <div style={{ width: 4, height: 20, borderRadius: 2, background: i === selected ? palette.accent : "transparent" }} />
          <span style={{ flex: 1 }}>{item}</span>
          {i === selected ? <span style={{ fontSize: 14, color: palette.muted }}>↵</span> : null}
        </div>
      ))}
    </div>
  </div>
);

/**
 * Text before/after a rewrite. Until `progress` reaches 0 the original shows; after that the new
 * text shows with changed words marked, the marker fading and settling 3 px upward as progress runs 0→1.
 */
export const DiffText: React.FC<{ before: string; after: string; changed: number[]; progress: number; palette: Palette; tail?: React.ReactNode }> = ({
  before,
  after,
  changed,
  progress,
  palette,
  tail,
}) => {
  if (progress < 0) return <span>{before}</span>;
  const p = Math.min(1, progress);
  return (
    <span>
      {after.split(" ").map((w, i) => {
        const isChanged = changed.includes(i);
        return (
          <span key={i}>
            <span
              style={{
                display: "inline-block",
                background: isChanged ? hexToRgba(palette.yellow, 0.55 * (1 - p)) : "transparent",
                color: isChanged ? (p < 0.7 ? palette.yellow : palette.fgStrong) : "inherit",
                transform: isChanged ? `translateY(${-3 * (1 - p)}px)` : undefined,
                borderRadius: 4,
                padding: isChanged ? "0 2px" : 0,
                margin: isChanged ? "0 -2px" : 0,
              }}
            >
              {w}
            </span>
            {i < after.split(" ").length - 1 ? " " : ""}
          </span>
        );
      })}
      {tail}
    </span>
  );
};

export { Caret };
