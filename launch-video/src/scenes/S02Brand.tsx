import { Easing } from "remotion";
import { BRAND } from "../brand";
import { Camera, useDuration, useT } from "../components/Camera";
import { Lockup } from "../components/Logo";
import { tween, EASE_IN_OUT } from "../theme";
import type { SceneTiming } from "../timing";

/** S02 · The lockup blooms over the water. */
export const S02Brand: React.FC<{ scene: SceneTiming }> = () => {
  const t = useT();
  const d = useDuration();
  return (
    <Camera zoom={tween(t, [0, d], [1, 1.045], EASE_IN_OUT)} originX={tween(t, [0, d], [30, 34], EASE_IN_OUT)} originY={45}>
      <div style={{ position: "absolute", left: 240, top: 330 }}>
        <Lockup t={t} delay={0.15} height={256} descriptor={BRAND.tagline} descriptorSize={40} />
      </div>
    </Camera>
  );
};
