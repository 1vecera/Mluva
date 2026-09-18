import { AbsoluteFill, useCurrentFrame, useVideoConfig } from "remotion";
import { EASE_OUT, tween } from "../theme";

/** Seconds since the enclosing sequence started. */
export const useT = (): number => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  return frame / fps;
};

/** Local scene duration in seconds. */
export const useDuration = (): number => {
  const { fps, durationInFrames } = useVideoConfig();
  return durationInFrames / fps;
};

/**
 * A film camera over the scene: zoom and pan are plain numbers computed by the scene,
 * so each shot decides its own move. `x`/`y` pan the content in pixels at zoom 1.
 */
export const Camera: React.FC<{
  zoom: number;
  x?: number;
  y?: number;
  originX?: number;
  originY?: number;
  children: React.ReactNode;
}> = ({ zoom, x = 0, y = 0, originX = 50, originY = 50, children }) => (
  <AbsoluteFill
    style={{
      transform: `scale(${zoom}) translate(${x}px, ${y}px)`,
      transformOrigin: `${originX}% ${originY}%`,
    }}
  >
    {children}
  </AbsoluteFill>
);

/** Fades scene content in over `inSec` and out over the last `outSec` of its sequence, with a subtle scale for depth instead of a flat opacity fade. */
export const SceneFade: React.FC<{ inSec?: number; outSec?: number; children: React.ReactNode }> = ({
  inSec = 0.28,
  outSec = 0.3,
  children,
}) => {
  const t = useT();
  const d = useDuration();
  const opacity = Math.min(tween(t, [0, inSec], [0, 1]), tween(t, [d - outSec, d], [1, 0], EASE_OUT));
  const scaleIn = tween(t, [0, inSec + 0.3], [0.985, 1]);
  const scaleOut = tween(t, [d - outSec - 0.2, d], [1, 1.015]);
  const scale = Math.min(scaleIn, scaleOut);
  return <AbsoluteFill style={{ opacity, transform: `scale(${scale})` }}>{children}</AbsoluteFill>;
};
