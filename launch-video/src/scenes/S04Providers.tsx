import { Audio } from "@remotion/media";
import { Easing, Sequence, staticFile, useVideoConfig } from "remotion";
import { Camera, useDuration, useT } from "../components/Camera";
import { Welcome } from "../components/Welcome";
import { NORD, tween } from "../theme";
import type { SceneTiming } from "../timing";

const SWITCH_SPEECH = 1.7;
const SWITCH_REWRITE = 3.0;

/** S04 · Local or cloud, your choice, changed with a click. */
export const S04Providers: React.FC<{ scene: SceneTiming }> = () => {
  const t = useT();
  const d = useDuration();
  const { fps } = useVideoConfig();
  return (
    <>
      <Camera zoom={tween(t, [0, d], [1.0, 1.1], Easing.linear)} originX={50} originY={42}>
        <div style={{ position: "absolute", left: 340, top: 120 }}>
          <Welcome width={1240} palette={NORD} speech={t < SWITCH_SPEECH ? 0 : 1} rewrite={t < SWITCH_REWRITE ? 0 : 1} />
        </div>
      </Camera>
      <Sequence from={Math.round(SWITCH_SPEECH * fps)} layout="none">
        <Audio src={staticFile("sfx/mouse-click.wav")} volume={0.4} />
      </Sequence>
      <Sequence from={Math.round(SWITCH_REWRITE * fps)} layout="none">
        <Audio src={staticFile("sfx/mouse-click.wav")} volume={0.4} />
      </Sequence>
    </>
  );
};
