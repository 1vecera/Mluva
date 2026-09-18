import { Easing } from "remotion";
import { BRAND } from "../brand";
import { Camera, useDuration, useT } from "../components/Camera";
import { Lockup } from "../components/Logo";
import { FONT, NORD, tween } from "../theme";
import type { SceneTiming } from "../timing";

/** S11 · Back to the lockup; the closing line and the repository hold the last beat. */
export const S11Outro: React.FC<{ scene: SceneTiming }> = () => {
  const t = useT();
  const d = useDuration();
  const closing = tween(t, [1.3, 2.1], [0, 1]);
  const meta = tween(t, [2.0, 2.8], [0, 1]);
  return (
    <Camera zoom={tween(t, [0, d], [1.0, 1.04], Easing.linear)}>
      <div style={{ position: "absolute", inset: 0, display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", gap: 44, paddingBottom: 60 }}>
        <Lockup t={t} delay={0.1} height={300} align="center" />
        <div style={{ fontFamily: FONT, fontSize: 46, fontWeight: 500, color: NORD.fgStrong, letterSpacing: -1, opacity: closing, transform: `translateY(${(1 - closing) * 14}px)` }}>{BRAND.closing}</div>
        <div style={{ fontFamily: FONT, fontSize: 22, color: NORD.fg, opacity: 0.85 * meta, display: "flex", gap: 28, transform: `translateY(${(1 - meta) * 10}px)` }}>
          <span>{BRAND.license}</span>
          <span style={{ color: NORD.frost }}>{BRAND.repo}</span>
        </div>
      </div>
    </Camera>
  );
};
