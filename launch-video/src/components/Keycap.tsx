import { FONT, hexToRgba, type Palette } from "../theme";

/** A physical-looking key. `press` is 0..1: 1 is fully pressed, with a frost glow. */
export const Keycap: React.FC<{ label: string; size?: number; press: number; palette: Palette; wide?: boolean }> = ({
  label,
  size = 120,
  press,
  palette,
  wide = false,
}) => {
  const travel = 8;
  return (
    <div style={{ position: "relative", width: wide ? size * 1.6 : size, height: size, fontFamily: FONT }}>
      <div
        style={{
          position: "absolute",
          inset: 0,
          borderRadius: size * 0.16,
          background: `linear-gradient(180deg, ${palette.surface} 0%, ${palette.selection} 100%)`,
          border: `2px solid ${hexToRgba(palette.fgStrong, 0.12)}`,
          boxShadow: `0 ${travel - travel * press + 2}px 0 ${palette.deep}, 0 ${18 - 10 * press}px ${40}px ${hexToRgba("#000000", 0.45)}, 0 0 ${60 * press}px ${hexToRgba(palette.frost, 0.65 * press)}`,
          transform: `translateY(${travel * press}px)`,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          color: press > 0.5 ? palette.frost : palette.fgStrong,
          fontWeight: 700,
          fontSize: size * (label.length > 3 ? 0.26 : 0.36),
          letterSpacing: -1,
        }}
      >
        {label}
      </div>
    </div>
  );
};

export const KeyCombo: React.FC<{ keys: string[]; size?: number; press: number; palette: Palette }> = ({ keys, size = 84, press, palette }) => (
  <div style={{ display: "flex", alignItems: "center", gap: 14, fontFamily: FONT }}>
    {keys.map((k, i) => (
      <div key={k} style={{ display: "flex", alignItems: "center", gap: 14 }}>
        <Keycap label={k} size={size} press={press} palette={palette} wide={k.length > 2} />
        {i < keys.length - 1 ? <div style={{ fontSize: size * 0.4, color: palette.muted, fontWeight: 500 }}>+</div> : null}
      </div>
    ))}
  </div>
);
