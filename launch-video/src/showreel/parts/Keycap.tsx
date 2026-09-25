import { C, DISPLAY } from "../theme";

// A dark keycap; `press` 0..1 sinks it into its base.
export const Keycap: React.FC<{ label: string; size?: number; press?: number; lit?: number; wide?: number }> = ({
  label,
  size = 96,
  press = 0,
  lit = 0,
  wide = 1,
}) => {
  const depth = size * 0.1 * (1 - press);
  return (
    <div style={{ position: "relative", width: size * wide, height: size + size * 0.1 }}>
      <div
        style={{
          position: "absolute",
          left: 0,
          top: size * 0.1,
          width: size * wide,
          height: size,
          borderRadius: size * 0.2,
          background: "#0b0b0b",
          boxShadow: `0 ${size * 0.18}px ${size * 0.4}px rgba(0,0,0,0.6)`,
        }}
      />
      <div
        style={{
          position: "absolute",
          left: 0,
          top: size * 0.1 - depth,
          width: size * wide,
          height: size,
          borderRadius: size * 0.2,
          background: "linear-gradient(180deg, #2b2b2b 0%, #161616 100%)",
          border: `1px solid rgba(245,245,245,${0.12 + 0.3 * lit})`,
          boxShadow: `inset 0 1px 0 rgba(255,255,255,0.1), 0 0 ${24 * lit}px rgba(233,27,39,${0.55 * lit})`,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          fontFamily: DISPLAY,
          fontWeight: 700,
          fontSize: size * 0.34,
          letterSpacing: "-0.01em",
          color: lit > 0.5 ? C.ink : "#d8d8d8",
        }}
      >
        {label}
      </div>
    </div>
  );
};
