import { Camera, useDuration, useT } from "../components/Camera";
import { HISTORY, Workspace } from "../components/Workspace";
import { NORD, hexToRgba, revealed, tween } from "../theme";
import type { SceneTiming } from "../timing";
import { HERO, HERO_FONT, POLISHED } from "./S06Polish";

const QUERY = "review";

/** S07 · Search history, land on the same note, revisions intact. */
export const S07History: React.FC<{ scene: SceneTiming }> = () => {
  const t = useT();
  const d = useDuration();
  const typed = QUERY.slice(0, revealed(t, 0.3, 0.09, QUERY.length));
  const filtered = tween(t, [1.0, 1.4], [1, 0.1]);
  const showChips = t >= 1.5;
  return (
    <Camera zoom={tween(t, [0, d], [1.22, 1.12])} originX={tween(t, [1.3, 2.1], [20, 50])} originY={26}>
      <div style={{ position: "absolute", left: HERO.x, top: HERO.y, width: HERO.w, height: HERO.h, border: `2px solid ${NORD.border}`, borderRadius: 16, overflow: "hidden", boxSizing: "border-box", boxShadow: `0 40px 100px ${hexToRgba("#000000", 0.5)}` }}>
        <Workspace
          width={HERO.w - 4}
          height={HERO.h - 4}
          palette={NORD}
          t={t}
          fontSize={HERO_FONT}
          query={typed}
          items={HISTORY}
          selected={0}
          itemOpacities={HISTORY.map((item, i) => (i === 0 || item.title.toLowerCase().includes("review") ? 1 : filtered))}
          noteMeta="Today · 14:31 · 2 revisions"
          chips={showChips ? ["Original", "Polished"] : undefined}
          chipsAt={1.5}
          statusLeft={`${typed ? "1 match" : "5 notes"} · Ctrl+H History`}
          original={<span>{POLISHED} Thanks!</span>}
        />
      </div>
    </Camera>
  );
};
