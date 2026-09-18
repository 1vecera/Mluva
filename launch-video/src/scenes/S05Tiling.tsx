import { Audio } from "@remotion/media";
import { Easing, Sequence, staticFile, useVideoConfig } from "remotion";
import { Camera, useDuration, useT } from "../components/Camera";
import { BAR_H, BrowserWindow, DesktopChrome, GAP, HerdrWindow, lerpRect, SCREEN_H, SCREEN_W, tile, Window, type Rect } from "../components/Desktop";
import { KeyCombo } from "../components/Keycap";
import { Recorder } from "../components/Recorder";
import { Workspace } from "../components/Workspace";
import { NORD, tween, EASE_IN_OUT } from "../theme";
import type { SceneTiming } from "../timing";
import { DICTATION } from "./S03F9";

const FLOATING: Rect = { x: 650, y: 330, w: 620, h: 300 };
const TILED = tile(0, 0, 2, 1);
const FULL: Rect = { x: GAP, y: BAR_H + GAP, w: SCREEN_W - GAP * 2, h: SCREEN_H - BAR_H - GAP * 2 };

const TILE_AT = 1.35;
const FULL_AT = 3.35;

/** S05 · The same window floats, tiles, then opens up. Real Omarchy borders and gaps. */
export const S05Tiling: React.FC<{ scene: SceneTiming }> = () => {
  const t = useT();
  const d = useDuration();
  const { fps } = useVideoConfig();
  const p1 = tween(t, [TILE_AT, TILE_AT + 0.7], [0, 1]);
  const p2 = tween(t, [FULL_AT, FULL_AT + 0.7], [0, 1]);
  const rect = p2 > 0 ? lerpRect(TILED, FULL, p2) : lerpRect(FLOATING, TILED, p1);
  const browserRect = lerpRect(tile(0, 0, 1, 1), tile(1, 0, 2, 2), p1);
  const companions = 1 - p2;
  const showWorkspace = p2 > 0.5;
  const combo1 = Math.min(tween(t, [0.85, 1.05], [0, 1]), tween(t, [2.3, 2.6], [1, 0]));
  const combo2 = Math.min(tween(t, [2.85, 3.05], [0, 1]), tween(t, [4.4, 4.7], [1, 0]));
  const pressAt = (at: number) => Math.min(tween(t, [at - 0.12, at], [0, 1]), tween(t, [at, at + 0.3], [1, 0]));
  return (
    <>
      <Camera zoom={tween(t, [0, d], [1.03, 1.0], EASE_IN_OUT)}>
        <DesktopChrome palette={NORD} title={showWorkspace ? "Mluva" : "Chromium — Mluva · Docs"}>
          <div style={{ position: "absolute", inset: 0, opacity: companions }}>
            <BrowserWindow rect={browserRect} palette={NORD} t={t} />
          </div>
          <div style={{ position: "absolute", inset: 0, opacity: p1 * companions }}>
            <HerdrWindow rect={tile(1, 1, 2, 2)} palette={NORD} t={t} />
          </div>
          <Window rect={rect} palette={NORD} active contentStyle={{ background: NORD.bg }}>
            {showWorkspace ? (
              <div style={{ opacity: tween(p2, [0.5, 1], [0, 1]) }}>
                <Workspace width={rect.w - 4} height={rect.h - 4} palette={NORD} t={t} original={<span>{DICTATION}</span>} />
              </div>
            ) : (
              <div style={{ opacity: 1 - tween(p2, [0, 0.5], [0, 1]) }}>
                <Recorder width={rect.w - 4} palette={NORD} text={DICTATION} visibleWords={99} state="ready" seconds={4} t={t} lines={Math.max(3, Math.floor((rect.h - 150) / 34))} />
              </div>
            )}
          </Window>
          <div style={{ position: "absolute", left: 60, bottom: 170, opacity: combo1, transform: `translateY(${(1 - combo1) * 12}px)` }}>
            <KeyCombo keys={["Super", "T"]} press={pressAt(TILE_AT)} palette={NORD} />
          </div>
          <div style={{ position: "absolute", left: 60, bottom: 170, opacity: combo2, transform: `translateY(${(1 - combo2) * 12}px)` }}>
            <KeyCombo keys={["Super", "F"]} press={pressAt(FULL_AT)} palette={NORD} />
          </div>
        </DesktopChrome>
      </Camera>
      <Sequence from={Math.round(TILE_AT * fps)} layout="none">
        <Audio src={staticFile("sfx/whoosh.wav")} volume={0.36} />
      </Sequence>
      <Sequence from={Math.round(FULL_AT * fps)} layout="none">
        <Audio src={staticFile("sfx/whoosh.wav")} volume={0.36} />
      </Sequence>
    </>
  );
};
