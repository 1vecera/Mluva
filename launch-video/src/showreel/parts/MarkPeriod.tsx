import { Img } from "remotion";
import { brand, ramp, ring } from "../theme";

// The glossy mark used as a full stop. The blob fills x 4.1–96.1 % and y 6.0–94.1 % of its square
// framing (docs/brand/png/mluva-mark-1024.png), so the image is shifted to seat the blob on the
// baseline with 2 px of overshoot. One size ratio and the brand's own rotation everywhere.
export const MarkPeriod: React.FC<{ fontSize: number; local: number; at?: number }> = ({ fontSize, local, at = 0 }) => {
  const size = fontSize * 0.3;
  const t = local - at;
  const pop = ramp(t, 0, 6) + 0.18 * ring(t / 60, 3.2, 9);
  return (
    <Img
      src={brand("mluva-mark.svg")}
      style={{
        width: size,
        height: size,
        marginLeft: fontSize * 0.03 - size * 0.041,
        transform: `translateY(${size * 0.059 + 2}px) scale(${Math.max(0, pop)})`,
        transformOrigin: "50% 100%",
        flexShrink: 0,
      }}
    />
  );
};
