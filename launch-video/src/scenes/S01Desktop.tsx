import { Easing } from "remotion";
import { Camera, useDuration, useT } from "../components/Camera";
import { BrowserWindow, DesktopChrome, HerdrWindow, TerminalWindow, tile } from "../components/Desktop";
import { Recorder } from "../components/Recorder";
import { NORD, tween } from "../theme";
import type { SceneTiming } from "../timing";

/** S01 · A real-feeling Omarchy desktop at work: browser, Ghostty, Herdr. The recorder arrives last. */
export const S01Desktop: React.FC<{ scene: SceneTiming }> = () => {
  const t = useT();
  const d = useDuration();
  const pop = tween(t, [1.9, 2.5], [0, 1]);
  return (
    <Camera zoom={tween(t, [0, d], [1.0, 1.06], Easing.linear)} x={tween(t, [0, d], [0, -18], Easing.linear)} y={tween(t, [0, d], [0, 6], Easing.linear)}>
      <DesktopChrome palette={NORD} title="ghostty — ~/code/mluva">
        <BrowserWindow rect={tile(0, 0, 2, 2, 1, 2)} palette={NORD} t={t} />
        <TerminalWindow rect={tile(1, 0, 2, 2)} palette={NORD} active t={t} />
        <HerdrWindow rect={tile(1, 1, 2, 2)} palette={NORD} t={t} />
        <div
          style={{
            position: "absolute",
            left: 1120,
            top: 560,
            opacity: pop,
            transform: `scale(${0.94 + 0.06 * pop}) translateY(${(1 - pop) * 18}px)`,
          }}
        >
          <Recorder width={640} palette={NORD} text="" visibleWords={0} state="idle" seconds={0} t={t} lines={3} />
        </div>
      </DesktopChrome>
    </Camera>
  );
};
