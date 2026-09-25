import { Easing } from "remotion";

// Chapter 03's Rewrite node becomes a portal over its last ten frames: it scales from 1 to 22 while
// its centre travels to the middle of the frame. Chapter 04 pre-rolls for those frames and shows its
// sphere clipped to the same box, so the next scene is visible through the node as it grows.
export const PORTAL_FRAMES = 10;
export const PORTAL_NODE = { x: 1560, y: 504, size: 96, radius: 24 };

export const portalAt = (t: number) => {
  const e = Easing.in(Easing.cubic)(Math.min(1, Math.max(0, t)));
  const k = 1 + 21 * e;
  const cx = PORTAL_NODE.x + (960 - PORTAL_NODE.x) * e;
  const cy = PORTAL_NODE.y + (540 - PORTAL_NODE.y) * e;
  const half = (PORTAL_NODE.size / 2) * k;
  return {
    k,
    dx: cx - PORTAL_NODE.x,
    dy: cy - PORTAL_NODE.y,
    clip: `inset(${cy - half}px ${1920 - cx - half}px ${1080 - cy - half}px ${cx - half}px round ${PORTAL_NODE.radius * k}px)`,
  };
};
