import { AbsoluteFill, useCurrentFrame } from "remotion";
import { beatAt, HEIGHT, WIDTH } from "../timing";
import { C } from "../theme";

// A kick-reactive pulse: 1 on each beat, decaying before the next.
export const beatPulse = (frame: number, decay = 7) => {
  const b = beatAt(frame);
  const phase = b - Math.floor(b);
  return b < 0 ? 0 : Math.exp(-decay * phase);
};

export const Background: React.FC<{ glowX?: number; glowY?: number; glow?: number }> = ({
  glowX = 0.5,
  glowY = 0.5,
  glow = 1,
}) => {
  const frame = useCurrentFrame();
  const pump = beatPulse(frame);
  return (
    <AbsoluteFill style={{ backgroundColor: C.bg, overflow: "hidden" }}>
      <AbsoluteFill
        style={{
          background: `radial-gradient(ellipse 58% 62% at ${glowX * 100}% ${glowY * 100}%, rgba(233,27,39,${
            (0.06 + 0.025 * pump) * glow
          }) 0%, rgba(120,12,18,${0.035 * glow}) 38%, rgba(0,0,0,0) 72%)`,
        }}
      />
      <AbsoluteFill
        style={{
          backgroundImage: "radial-gradient(rgba(245,245,245,0.09) 1px, transparent 1.4px)",
          backgroundSize: "36px 36px",
          backgroundPosition: `${(frame * 0.12) % 36}px ${(frame * 0.05) % 36}px`,
          opacity: 0.55,
          maskImage: "radial-gradient(ellipse 70% 70% at 50% 50%, black 20%, transparent 85%)",
        }}
      />
      <AbsoluteFill
        style={{ background: "radial-gradient(ellipse 85% 85% at 50% 50%, transparent 55%, rgba(0,0,0,0.75) 100%)" }}
      />
    </AbsoluteFill>
  );
};

export const Grain: React.FC<{ opacity?: number }> = ({ opacity = 0.07 }) => {
  const frame = useCurrentFrame();
  return (
    <AbsoluteFill style={{ pointerEvents: "none", mixBlendMode: "overlay", opacity }}>
      <svg width={WIDTH} height={HEIGHT}>
        <filter id="showreel-grain">
          <feTurbulence type="fractalNoise" baseFrequency="0.85" numOctaves="2" seed={frame % 13} stitchTiles="stitch" />
          <feColorMatrix type="saturate" values="0" />
        </filter>
        <rect width={WIDTH} height={HEIGHT} filter="url(#showreel-grain)" />
      </svg>
    </AbsoluteFill>
  );
};
