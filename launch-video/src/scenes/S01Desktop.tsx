import { Easing } from "remotion";
import { Camera, useDuration, useT } from "../components/Camera";
import { BrowserWindow, DesktopChrome, HerdrWindow, TerminalWindow, tile } from "../components/Desktop";
import { Recorder } from "../components/Recorder";
import { NORD, tween, EASE_IN_OUT } from "../theme";
import type { SceneTiming } from "../timing";

/** S01 · A real-feeling Omarchy desktop at work: browser, Ghostty, Herdr. The recorder arrives last. */
export const S01Desktop: React.FC<{ scene: SceneTiming }> = () => {
  const t = useT();
  const d = useDuration();
  const enter = (at: number) => tween(t, [at, at + 0.35], [0, 1]);
  const e0 = enter(0.1);
  const e1 = enter(0.28);
  const e2 = enter(0.46);
  const p = tween(t, [1.9, 2.5], [0, 1]);
  const over = tween(t, [1.9, 2.2], [0, 0.05]) - tween(t, [2.2, 2.7], [0, 0.05]);
  const popScale = 0.9 + 0.1 * p + over;
  const slot = (e: number): React.CSSProperties => ({
    position: "absolute",
    inset: 0,
    opacity: e,
    transform: `translateY(${(1 - e) * 26}px) scale(${0.98 + 0.02 * e})`,
  });
  return (
    <Camera zoom={tween(t, [0, d], [1.0, 1.06], EASE_IN_OUT)} x={tween(t, [0, d], [0, -18], EASE_IN_OUT)} y={tween(t, [0, d], [0, 6], EASE_IN_OUT)}>
      <DesktopChrome palette={NORD} title="ghostty — ~/code/mluva">
        <div style={slot(e0)}>
          <BrowserWindow rect={tile(0, 0, 2, 2, 1, 2)} palette={NORD} t={t} />
        </div>
        <div style={slot(e1)}>
          <TerminalWindow rect={tile(1, 0, 2, 2)} palette={NORD} active t={t} />
        </div>
        <div style={slot(e2)}>
          <HerdrWindow rect={tile(1, 1, 2, 2)} palette={NORD} t={t} />
        </div>
        <div
          style={{
            position: "absolute",
            left: 1120,
            top: 560,
            opacity: p,
            transform: `scale(${popScale}) translateY(${(1 - p) * 18}px)`,
          }}
        >
          <Recorder width={640} palette={NORD} text="" visibleWords={0} state="idle" seconds={0} t={t} lines={3} />
        </div>
      </DesktopChrome>
    </Camera>
  );
};
