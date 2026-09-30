import { Img } from "remotion";
import { brand, ramp, ring } from "../theme";

// The glossy mark used as a full stop. The blob fills x 4.1–96.1 % and y 6.0–94.1 % of its square
// framing (docs/brand/png/mluva-mark-1024.png), so the image is shifted to seat the blob on the
// baseline with 2 px of overshoot. One size ratio and the brand's own rotation everywhere.
// After letters with an open lower right (T, V, W, Y) the stop tucks in under the arm.
export const MarkPeriod: React.FC<{ fontSize: number; local: number; at?: number; after?: string }> = ({
  fontSize,
  local,
  at = 0,
  after = "",
}) => {
  const size = fontSize * 0.3;
  const last = after.slice(-1);
  const kern = last && "TVWY".includes(last) ? -0.12 * fontSize : 0;
  const t = local - at;
  const pop = ramp(t, 0, 6) + 0.18 * ring(t / 60, 3.2, 9);
  return (
    <Img
      src={brand("mluva-mark.svg")}
      style={{
        width: size,
        height: size,
        marginLeft: fontSize * 0.03 - size * 0.041 + kern,
        transform: `translateY(${size * 0.059 + 2}px) scale(${Math.max(0, pop)})`,
        transformOrigin: "50% 100%",
        flexShrink: 0,
      }}
    />
  );
};
