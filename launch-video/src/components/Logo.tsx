import { Img } from "remotion";
import { BRAND } from "../brand";
import { EASE_IN_OUT, FONT, NORD, tween } from "../theme";

/** The glossy mark at a given height; width follows the SVG's own aspect. */
export const Mark: React.FC<{ height: number; style?: React.CSSProperties }> = ({ height, style }) => (
  <Img src={BRAND.markSrc} style={{ height, width: "auto", display: "block", ...style }} />
);

export const Wordmark: React.FC<{ height: number; style?: React.CSSProperties }> = ({ height, style }) => (
  <Img src={BRAND.wordmarkSrc} style={{ height, width: "auto", display: "block", ...style }} />
);

/**
 * Brand lockup reveal. The official lockup file is animated as one object so the mark/wordmark
 * ratio and gap stay exactly as designed: it blooms in from a soft blur while a wipe travels
 * left to right, so the mark lands first and the wordmark follows. The descriptor settles last.
 */
export const Lockup: React.FC<{
  t: number;
  height?: number;
  descriptor?: string;
  descriptorSize?: number;
  delay?: number;
  align?: "left" | "center";
}> = ({ t, height = 220, descriptor, descriptorSize = 36, delay = 0, align = "left" }) => {
  const tt = t - delay;
  const bloom = tween(tt, [0, 1.0], [0, 1]);
  const wipe = tween(tt, [0.15, 1.35], [0, 1], EASE_IN_OUT);
  return (
    <div style={{ display: "flex", flexDirection: "column", alignItems: align === "center" ? "center" : "flex-start", gap: Math.round(height * 0.16) }}>
      <div
        style={{
          opacity: bloom,
          transform: `scale(${0.9 + 0.1 * bloom}) translateY(${(1 - bloom) * 12}px)`,
          filter: `blur(${(1 - bloom) * 10}px)`,
          clipPath: `inset(-10% ${(1 - wipe) * 100}% -10% -2%)`,
        }}
      >
        <Img src={BRAND.lockupSrc} style={{ height, width: "auto", display: "block" }} />
      </div>
      {descriptor ? (
        <div
          style={{
            fontFamily: FONT,
            fontWeight: 400,
            fontSize: descriptorSize,
            letterSpacing: -0.5,
            color: NORD.fg,
          }}
        >
          {descriptor.split(" ").map((w, i) => {
            const wp = tween(tt, [1.2 + i * 0.06, 1.5 + i * 0.06], [0, 1]);
            return (
              <span key={i} style={{ display: "inline-block", opacity: wp, transform: `translateY(${(1 - wp) * 14}px)`, marginRight: "0.28em" }}>
                {w}
              </span>
            );
          })}
        </div>
      ) : null}
    </div>
  );
};
