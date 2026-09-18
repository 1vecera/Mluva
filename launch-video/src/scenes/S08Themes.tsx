import { Audio } from "@remotion/media";
import { Easing, Sequence, staticFile, useVideoConfig } from "remotion";
import { Camera, useDuration, useT } from "../components/Camera";
import { BAR_H, DesktopChrome, GAP, SCREEN_H, SCREEN_W, Window } from "../components/Desktop";
import { Workspace } from "../components/Workspace";
import { FONT, NORD, ROSE, TOKYO, hexToRgba, tween, type Palette } from "../theme";
import type { SceneTiming } from "../timing";
import { POLISHED } from "./S06Polish";

const FULL = { x: GAP, y: BAR_H + GAP, w: SCREEN_W - GAP * 2, h: SCREEN_H - BAR_H - GAP * 2 };
const TO_TOKYO = 1.1;
const TO_ROSE = 2.9;
const FADE = 0.6;

const Layer: React.FC<{ palette: Palette; opacity: number; t: number }> = ({ palette, opacity, t }) => (
  <div style={{ position: "absolute", inset: 0, opacity }}>
    <DesktopChrome palette={palette} title="Mluva">
      <Window rect={FULL} palette={palette} active>
        <Workspace width={FULL.w - 4} height={FULL.h - 4} palette={palette} t={t} noteMeta="Today · 14:31 · Polished" original={<span>{POLISHED} Thanks!</span>} statusLeft="Ready · F9 to dictate" />
      </Window>
      <div
        style={{
          position: "absolute",
          right: 90,
          top: BAR_H + 36,
          padding: "10px 18px",
          borderRadius: 10,
          background: hexToRgba(palette.deep, 0.9),
          border: `2px solid ${palette.border}`,
          color: palette.fgStrong,
          fontFamily: FONT,
          fontSize: 22,
          fontWeight: 500,
        }}
      >
        {palette.name}
      </div>
    </DesktopChrome>
  </div>
);

/** S08 · The same window, three Omarchy themes, real border colors included. */
export const S08Themes: React.FC<{ scene: SceneTiming }> = () => {
  const t = useT();
  const d = useDuration();
  const { fps } = useVideoConfig();
  const toTokyo = tween(t, [TO_TOKYO, TO_TOKYO + FADE], [0, 1]);
  const toRose = tween(t, [TO_ROSE, TO_ROSE + FADE], [0, 1]);
  return (
    <>
      <Camera zoom={tween(t, [0, d], [1.04, 1.0], Easing.linear)}>
        <Layer palette={NORD} opacity={1} t={t} />
        <Layer palette={TOKYO} opacity={toTokyo} t={t} />
        <Layer palette={ROSE} opacity={toRose} t={t} />
      </Camera>
      <Sequence from={Math.round(TO_TOKYO * fps)} layout="none">
        <Audio src={staticFile("sfx/switch.wav")} volume={0.25} />
      </Sequence>
      <Sequence from={Math.round(TO_ROSE * fps)} layout="none">
        <Audio src={staticFile("sfx/switch.wav")} volume={0.25} />
      </Sequence>
    </>
  );
};
