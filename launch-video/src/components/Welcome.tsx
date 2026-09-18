import { FONT, hexToRgba, type Palette } from "../theme";

type Option = { name: string; tag: "local" | "cloud" | "custom" };

const SPEECH: Option[] = [
  { name: "Whisper", tag: "local" },
  { name: "ElevenLabs Scribe", tag: "cloud" },
  { name: "Custom endpoint", tag: "custom" },
];
const REWRITE: Option[] = [
  { name: "Local model", tag: "local" },
  { name: "Claude", tag: "cloud" },
  { name: "OpenAI", tag: "cloud" },
];

const tagColor = (p: Palette, tag: Option["tag"]) => (tag === "local" ? p.green : tag === "cloud" ? p.accent : p.purple);

const Row: React.FC<{ title: string; hint: string; options: Option[]; selected: number; palette: Palette }> = ({ title, hint, options, selected, palette }) => (
  <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
    <div style={{ display: "flex", alignItems: "baseline", gap: 16 }}>
      <div style={{ fontSize: 22, fontWeight: 500, color: palette.fgStrong }}>{title}</div>
      <div style={{ fontSize: 16, color: palette.muted }}>{hint}</div>
    </div>
    <div style={{ display: "flex", gap: 14 }}>
      {options.map((o, i) => {
        const on = i === selected;
        return (
          <div
            key={o.name}
            style={{
              flex: 1,
              padding: "18px 20px",
              borderRadius: 12,
              border: `2px solid ${on ? palette.accent : hexToRgba(palette.fg, 0.14)}`,
              background: on ? hexToRgba(palette.accent, 0.14) : palette.deep,
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              boxShadow: on ? `0 0 0 4px ${hexToRgba(palette.accent, 0.18)}` : "none",
            }}
          >
            <div style={{ fontSize: 20, color: on ? palette.fgStrong : palette.fg }}>{o.name}</div>
            <div style={{ fontSize: 13, padding: "3px 10px", borderRadius: 999, color: tagColor(palette, o.tag), border: `1px solid ${hexToRgba(tagColor(palette, o.tag), 0.5)}`, letterSpacing: 0.5 }}>{o.tag}</div>
          </div>
        );
      })}
    </div>
  </div>
);

/** First-run welcome: the two provider choices, exactly the controls Settings exposes later. */
export const Welcome: React.FC<{ width: number; palette: Palette; speech: number; rewrite: number }> = ({ width, palette, speech, rewrite }) => (
  <div
    style={{
      width,
      fontFamily: FONT,
      background: palette.bg,
      border: `2px solid ${palette.border}`,
      boxSizing: "border-box",
      padding: "44px 52px 40px",
      display: "flex",
      flexDirection: "column",
      gap: 36,
      color: palette.fg,
      boxShadow: `0 40px 100px ${hexToRgba("#000000", 0.45)}`,
    }}
  >
    <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
      <div style={{ fontSize: 38, fontWeight: 700, color: palette.fgStrong, letterSpacing: -1 }}>Welcome to Mluva</div>
      <div style={{ fontSize: 19, color: palette.fg, opacity: 0.85 }}>Pick a speech model and a rewrite model. Change either later in Settings (Ctrl+,).</div>
    </div>
    <Row title="Speech recognition" hint="turns your voice into text" options={SPEECH} selected={speech} palette={palette} />
    <Row title="Rewriting" hint="polishes, restructures, answers" options={REWRITE} selected={rewrite} palette={palette} />
    <div style={{ display: "flex", justifyContent: "flex-end", alignItems: "center", gap: 18 }}>
      <div style={{ fontSize: 15, color: palette.muted }}>Keys stay on this machine.</div>
      <div style={{ padding: "12px 26px", borderRadius: 10, background: palette.accent, color: palette.deep, fontSize: 18, fontWeight: 700 }}>Continue</div>
    </div>
  </div>
);
