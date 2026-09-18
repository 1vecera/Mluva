import { Camera, useDuration, useT } from "../components/Camera";
import { Caret, Workspace } from "../components/Workspace";
import { NORD, hexToRgba, revealed, tween, type Palette } from "../theme";
import type { SceneTiming } from "../timing";
import { HERO, HERO_FONT } from "./S06Polish";

export const SOURCE =
  "We need to export notes to Markdown, preserve the original wording, and save everything locally. The first version is for personal use. Let's include the date in each file name.";
const SOURCE_WORDS = SOURCE.split(" ");

export type Block = { text: string; kind: "h1" | "h2" | "li" | "p" | "ok"; at: number };
export const DRAFT: Block[] = [
  { text: "Notes export", kind: "h1", at: 1.1 },
  { text: "Requirements", kind: "h2", at: 1.6 },
  { text: "Preserve the original wording.", kind: "li", at: 2.0 },
  { text: "Save files locally.", kind: "li", at: 2.45 },
  { text: "Include the date in each file name.", kind: "li", at: 2.95 },
  { text: "Audience", kind: "h2", at: 3.35 },
  { text: "Personal use.", kind: "p", at: 3.7 },
];

export const MarkdownBlocks: React.FC<{ blocks: Block[]; t: number; palette: Palette; fontSize?: number }> = ({ blocks, t, palette, fontSize = 22 }) => (
  <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
    {blocks.map((b, i) => {
      const p = tween(t, [b.at, b.at + 0.25], [0, 1]);
      if (p <= 0) return null;
      const style: React.CSSProperties = {
        opacity: p,
        transform: `translateY(${(1 - p) * 8}px)`,
        fontSize: b.kind === "h1" ? fontSize * 1.35 : b.kind === "h2" ? fontSize * 1.1 : fontSize,
        fontWeight: b.kind === "h1" || b.kind === "h2" ? 700 : 400,
        color: b.kind === "ok" ? palette.green : palette.fgStrong,
        marginTop: b.kind === "h2" ? 10 : 0,
        display: "flex",
        gap: 12,
      };
      return (
        <div key={i} style={style}>
          {b.kind === "li" ? <span style={{ color: palette.accent }}>–</span> : null}
          {b.kind === "ok" ? <span>✓</span> : null}
          <span>{b.text}</span>
        </div>
      );
    })}
  </div>
);

/** S09 · Live rewrite: the note takes shape beside the continuous original. */
export const S09Live: React.FC<{ scene: SceneTiming }> = () => {
  const t = useT();
  const d = useDuration();
  const shown = revealed(t, 0.45, 0.075, SOURCE_WORDS.length);
  return (
    <Camera zoom={tween(t, [0, d], [1.0, 1.14])} originX={56} originY={44}>
      <div style={{ position: "absolute", left: HERO.x, top: HERO.y, width: HERO.w, height: HERO.h, border: `2px solid ${NORD.border}`, boxSizing: "border-box", boxShadow: `0 40px 100px ${hexToRgba("#000000", 0.5)}` }}>
        <Workspace
          width={HERO.w - 4}
          height={HERO.h - 4}
          palette={NORD}
          t={t}
          fontSize={HERO_FONT}
          selected={1}
          noteTitle="Notes export requirements"
          noteMeta="Live · Whisper (local) · Claude"
          badge="Live rewrite · Experimental"
          statusLeft="Recording · F9 to stop"
          statusRight="Ctrl+1 Original · Ctrl+2 Draft"
          original={
            <span>
              {SOURCE_WORDS.slice(0, shown).join(" ")}
              <Caret t={t} color={NORD.frost} height={24} />
            </span>
          }
          draft={<MarkdownBlocks blocks={DRAFT} t={t} palette={NORD} />}
        />
      </div>
    </Camera>
  );
};
