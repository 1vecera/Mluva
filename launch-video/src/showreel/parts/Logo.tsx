import { Img } from "remotion";
import { brand } from "../theme";

// docs/brand geometry: the large-mark lockup is 341.9 × 125.1 units. Inside it the mark's square
// framing sits at (-3.0508, -5.6653) with side 136.4291 (translate(-9.0723 -11.4237) scale(0.11478)
// of the 1188.62 framing at 52.46, 50.17) and the wordmark's tight box at (156.90, 34.73), 183.68 × 56.68.
export const LOCKUP = { w: 341.9, h: 125.1 };
export const MARK_BOX = { x: 52.46 * 0.11478 - 9.0723, y: 50.17 * 0.11478 - 11.4237, size: 1188.62 * 0.11478 };
export const WORD_BOX = { x: 157.99 - 1.09, y: 35.82 - 1.09, w: 183.68, h: 56.68 };
// Empty space between the mark (ends near x = 133) and the wordmark (starts at 156.9).
const SPLIT = 146;

export type Box = { x: number; y: number; w: number; h: number };

export const lockupBoxes = (left: number, top: number, scale: number) => ({
  mark: { x: left + MARK_BOX.x * scale, y: top + MARK_BOX.y * scale, w: MARK_BOX.size * scale, h: MARK_BOX.size * scale },
  word: { x: left + WORD_BOX.x * scale, y: top + WORD_BOX.y * scale, w: WORD_BOX.w * scale, h: WORD_BOX.h * scale },
});

export const Mark: React.FC<{ box: Box; style?: React.CSSProperties; flat?: boolean }> = ({ box, style, flat }) => (
  <Img
    src={brand(flat ? "mluva-mark-flat.svg" : "mluva-mark.svg")}
    style={{ position: "absolute", left: box.x, top: box.y, width: box.w, height: box.h, ...style }}
  />
);

// One half of the real lockup SVG. At rest (no transform) the two halves together are the brand file,
// pixel for pixel; `markTo` moves the mark half so its framing box lands on another box.
export const LockupHalf: React.FC<{
  part: "mark" | "word";
  left: number;
  top: number;
  scale: number;
  markTo?: Box;
  transform?: string;
  style?: React.CSSProperties;
}> = ({ part, left, top, scale, markTo, transform = "", style }) => {
  const width = LOCKUP.w * scale;
  const height = LOCKUP.h * scale;
  const home = lockupBoxes(left, top, scale).mark;
  let move = "";
  if (part === "mark" && markTo) {
    const k = markTo.w / home.w;
    const dx = markTo.x - home.x;
    const dy = markTo.y - home.y;
    if (Math.abs(dx) > 0.01 || Math.abs(dy) > 0.01 || Math.abs(k - 1) > 1e-4) {
      move = `translate(${dx}px, ${dy}px) scale(${k})`;
    }
  }
  const clip = part === "mark" ? `inset(-40% ${(1 - SPLIT / LOCKUP.w) * 100}% -40% -40%)` : `inset(-40% -10% -40% ${(SPLIT / LOCKUP.w) * 100}%)`;
  return (
    <div
      style={{
        position: "absolute",
        left,
        top,
        width,
        height,
        transformOrigin: part === "mark" ? `${home.x - left}px ${home.y - top}px` : "50% 50%",
        transform: `${move} ${transform}`.trim() || undefined,
        ...style,
      }}
    >
      <Img src={brand("mluva-logo-large-mark-on-dark.svg")} style={{ width, height, display: "block", clipPath: clip }} />
    </div>
  );
};

// The untouched brand lockup, used whenever both halves are at rest.
export const LockupFull: React.FC<{ left: number; top: number; scale: number; style?: React.CSSProperties }> = ({
  left,
  top,
  scale,
  style,
}) => (
  <Img
    src={brand("mluva-logo-large-mark-on-dark.svg")}
    style={{ position: "absolute", left, top, width: LOCKUP.w * scale, height: LOCKUP.h * scale, display: "block", ...style }}
  />
);
