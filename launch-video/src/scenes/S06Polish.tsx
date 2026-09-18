import { Audio } from "@remotion/media";
import { Sequence, staticFile, useVideoConfig } from "remotion";
import { Camera, useT } from "../components/Camera";
import { KeyCombo } from "../components/Keycap";
import { Caret, CommandPalette, DiffText, Workspace } from "../components/Workspace";
import { FONT, NORD, hexToRgba, revealed, tween } from "../theme";
import type { SceneTiming } from "../timing";
import { DICTATION } from "./S03F9";

export const POLISHED = "Could we reschedule the review for Tuesday? I'll send the draft tomorrow.";
const CHANGED = [2, 5];
const COMMANDS = ["Polish text", "Rewrite with instructions", "Live rewrite · Experimental", "Copy text", "Save note", "Settings…"];

const OPEN = 0.85;
const SELECT = 2.05;
const APPLY = 2.4;
const TYPE_FROM = 4.1;
const TAIL = " Thanks!";

/** Hero frame used by the workspace scenes: the app window floating over the water. */
export const HERO = { x: 100, y: 64, w: 1720, h: 880 };
export const HERO_FONT = 26;

/** S06 · Done talking? Ctrl+P, Polish, and the changed words settle in. Then a hand edit. */
export const S06Polish: React.FC<{ scene: SceneTiming }> = () => {
  const t = useT();
  const { fps } = useVideoConfig();
  const paletteVisible = Math.min(tween(t, [OPEN, OPEN + 0.2], [0, 1]), tween(t, [SELECT, SELECT + 0.2], [1, 0]));
  const query = "pol".slice(0, revealed(t, OPEN + 0.35, 0.12, 3));
  const progress = t < APPLY ? -1 : tween(t, [APPLY, APPLY + 1.8], [0, 1]);
  const typed = TAIL.slice(0, revealed(t, TYPE_FROM, 0.08, TAIL.length));
  const combo = Math.min(tween(t, [0.45, 0.65], [0, 1]), tween(t, [1.7, 2.0], [1, 0]));
  const press = Math.min(tween(t, [OPEN - 0.12, OPEN], [0, 1]), tween(t, [OPEN, OPEN + 0.3], [1, 0]));
  const kept = tween(t, [APPLY + 0.5, APPLY + 1.1], [0, 1]);
  const punchIn = Math.min(tween(t, [APPLY, APPLY + 0.12], [0, 0.03]), tween(t, [APPLY + 0.12, APPLY + 0.5], [0.03, 0]));
  return (
    <>
      <Camera zoom={tween(t, [0.3, 2.3], [1.0, 1.18]) + punchIn} originX={58} originY={40}>
        <div style={{ position: "absolute", left: HERO.x, top: HERO.y, width: HERO.w, height: HERO.h, border: `2px solid ${NORD.border}`, boxSizing: "border-box", boxShadow: `0 40px 100px ${hexToRgba("#000000", 0.5)}` }}>
          <Workspace
            width={HERO.w - 4}
            height={HERO.h - 4}
            palette={NORD}
            t={t}
            fontSize={HERO_FONT}
            noteMeta={t >= APPLY ? "Today · 14:31 · Polished" : "Today · 14:31 · Whisper (local)"}
            statusLeft={t >= APPLY + 1.3 ? "Original preserved" : t >= APPLY ? "Polished · 2 words changed" : "Dictation ready. Automatic copying is off."}
            original={
              <div style={{ display: "flex", flexDirection: "column", gap: 34 }}>
                <div>
                  <DiffText
                    before={DICTATION}
                    after={POLISHED}
                    changed={CHANGED}
                    progress={progress}
                    palette={NORD}
                    tail={
                      t >= TYPE_FROM - 0.3 ? (
                        <>
                          <span>{typed}</span>
                          <Caret t={t} color={NORD.frost} height={26} />
                        </>
                      ) : null
                    }
                  />
                </div>
                {kept > 0 ? (
                  <div style={{ opacity: kept, transform: `translateY(${(1 - kept) * 10}px)`, padding: "16px 18px", borderRadius: 10, background: hexToRgba(NORD.deep, 0.7), border: `1px solid ${hexToRgba(NORD.fg, 0.1)}`, fontFamily: FONT, display: "flex", flexDirection: "column", gap: 8 }}>
                    <div style={{ fontSize: 13, letterSpacing: 1, color: NORD.muted }}>ORIGINAL · KEPT</div>
                    <div style={{ fontSize: HERO_FONT - 4, color: NORD.fg, opacity: 0.8, lineHeight: 1.5 }}>{DICTATION}</div>
                  </div>
                ) : null}
              </div>
            }
          />
          <CommandPalette visible={paletteVisible} items={COMMANDS} selected={0} query={query} palette={NORD} top={110} />
          <div style={{ position: "absolute", right: 48, bottom: 64, opacity: combo, transform: `translateY(${(1 - combo) * 12}px)` }}>
            <KeyCombo keys={["Ctrl", "P"]} size={72} press={press} palette={NORD} />
          </div>
        </div>
      </Camera>
      <Sequence from={Math.round(OPEN * fps)} layout="none">
        <Audio src={staticFile("sfx/mouse-click.wav")} volume={0.3} />
      </Sequence>
      <Sequence from={Math.round(SELECT * fps)} layout="none">
        <Audio src={staticFile("sfx/switch.wav")} volume={0.26} />
      </Sequence>
    </>
  );
};
