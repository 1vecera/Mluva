import { AbsoluteFill, useCurrentFrame, useVideoConfig } from "remotion";
import { NORD } from "../theme";

/**
 * Procedural water. Slowly drifting light bands are pushed through a horizontally stretched
 * fractal-noise displacement field, so the highlights wander like ripples on a dark lagoon.
 * Two layers move against each other for depth; a top light and a vignette keep the UI readable.
 */
export const WaterBackground: React.FC<{ intensity?: number }> = ({ intensity = 1 }) => {
  const frame = useCurrentFrame();
  const { fps, width, height } = useVideoConfig();
  const t = frame / fps;

  const driftA = (t * 26) % 1200;
  const driftB = (t * -17) % 1200;
  const breathe = 0.5 + 0.5 * Math.sin((t / 10) * Math.PI * 2);

  return (
    <AbsoluteFill style={{ backgroundColor: NORD.deep }}>
      <AbsoluteFill
        style={{
          background: `radial-gradient(130% 100% at 50% 40%, #2b3140 0%, ${NORD.deep} 62%, #0d0f14 100%)`,
        }}
      />
      <svg
        width={width}
        height={height}
        viewBox={`0 0 ${width} ${height}`}
        style={{ position: "absolute", inset: 0, opacity: 0.3 * intensity }}
      >
        <defs>
          <filter id="water-a" x="-10%" y="-10%" width="120%" height="120%">
            <feTurbulence type="fractalNoise" baseFrequency="0.0016 0.011" numOctaves="3" seed="11" result="noise" />
            <feDisplacementMap in="SourceGraphic" in2="noise" scale={60 + breathe * 20} xChannelSelector="R" yChannelSelector="G" />
            <feGaussianBlur stdDeviation="1.2" />
          </filter>
          <filter id="water-b" x="-10%" y="-10%" width="120%" height="120%">
            <feTurbulence type="fractalNoise" baseFrequency="0.0028 0.016" numOctaves="2" seed="4" result="noise" />
            <feDisplacementMap in="SourceGraphic" in2="noise" scale={60} xChannelSelector="G" yChannelSelector="B" />
            <feGaussianBlur stdDeviation="2" />
          </filter>
          <linearGradient id="grad-a" gradientUnits="userSpaceOnUse" x1={0} y1={0} x2={260} y2={1200} gradientTransform={`translate(${driftA * 0.25} ${driftA})`} spreadMethod="reflect">
            <stop offset="0" stopColor="#2e3440" stopOpacity="0" />
            <stop offset="0.42" stopColor="#5e81ac" stopOpacity="0.5" />
            <stop offset="0.5" stopColor="#8fbcbb" stopOpacity="0.55" />
            <stop offset="0.58" stopColor="#5e81ac" stopOpacity="0.5" />
            <stop offset="1" stopColor="#2e3440" stopOpacity="0" />
          </linearGradient>
          <linearGradient id="grad-b" gradientUnits="userSpaceOnUse" x1={0} y1={0} x2={-180} y2={900} gradientTransform={`translate(${driftB * -0.2} ${driftB})`} spreadMethod="reflect">
            <stop offset="0" stopColor="#191c23" stopOpacity="0" />
            <stop offset="0.5" stopColor="#81a1c1" stopOpacity="0.35" />
            <stop offset="1" stopColor="#191c23" stopOpacity="0" />
          </linearGradient>
        </defs>
        <rect x={-200} y={-200} width={width + 400} height={height + 400} fill="url(#grad-a)" filter="url(#water-a)" />
        <rect x={-200} y={-200} width={width + 400} height={height + 400} fill="url(#grad-b)" filter="url(#water-b)" style={{ mixBlendMode: "screen" }} />
      </svg>
      <AbsoluteFill
        style={{
          background: "linear-gradient(180deg, rgba(143,188,187,0.10) 0%, rgba(0,0,0,0) 35%, rgba(0,0,0,0) 70%, rgba(6,8,12,0.45) 100%)",
        }}
      />
      <AbsoluteFill
        style={{
          background: "radial-gradient(85% 70% at 50% 50%, rgba(0,0,0,0) 55%, rgba(6,8,12,0.6) 100%)",
        }}
      />
      <svg width={width} height={height} viewBox={`0 0 ${width} ${height}`} style={{ position: "absolute", inset: 0, opacity: 0.05 }}>
        <defs>
          <filter id="grain">
            <feTurbulence type="fractalNoise" baseFrequency="0.9" numOctaves="2" seed="7" result="n" />
            <feColorMatrix in="n" type="matrix" values="0 0 0 0 1 0 0 0 0 1 0 0 0 0 1 0 0 0 0.6 0" />
          </filter>
        </defs>
        <rect x={0} y={0} width={width} height={height} filter="url(#grain)" />
      </svg>
    </AbsoluteFill>
  );
};
