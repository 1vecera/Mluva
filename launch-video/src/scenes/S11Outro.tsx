import { Easing } from "remotion";
import { BRAND } from "../brand";
import { Camera, useDuration, useT } from "../components/Camera";
import { Lockup } from "../components/Logo";
import { FONT, NORD, tween, EASE_IN_OUT } from "../theme";
import type { SceneTiming } from "../timing";

/** S11 · Back to the lockup; the closing line and the repository hold the last beat. */
export const S11Outro: React.FC<{ scene: SceneTiming }> = () => {
  const t = useT();
  const d = useDuration();
  const closing = tween(t, [1.3, 2.1], [0, 1]);
  const tracking = tween(t, [1.2, 2.2], [2, -1]);
  const metaA = tween(t, [2.0, 2.6], [0, 1]);
  const metaB = tween(t, [2.2, 2.8], [0, 1]);
  return (
    <Camera zoom={tween(t, [0, d], [1.0, 1.04], EASE_IN_OUT)}>
      <div style={{ position: "absolute", inset: 0, display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", gap: 44, paddingBottom: 60 }}>
        <Lockup t={t} delay={0.1} height={300} align="center" />
        <div style={{ fontFamily: FONT, fontSize: 46, fontWeight: 500, color: NORD.fgStrong, letterSpacing: tracking, opacity: closing, transform: `translateY(${(1 - closing) * 14}px)` }}>{BRAND.closing}</div>
        <div style={{ fontFamily: FONT, fontSize: 22, color: NORD.fg, display: "flex", gap: 28 }}>
          <span style={{ opacity: 0.85 * metaA, transform: `translateY(${(1 - metaA) * 10}px)` }}>{BRAND.license}</span>
          <span style={{ color: NORD.frost, opacity: 0.85 * metaB, transform: `translateY(${(1 - metaB) * 10}px)` }}>{BRAND.repo}</span>
        </div>
      </div>
    </Camera>
  );
};
