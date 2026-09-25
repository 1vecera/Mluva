import { AbsoluteFill, Img, Sequence } from "remotion";
import { brand } from "./theme";
import { LOCKUP } from "./parts/Logo";
import { C05Logo, LOCKUP_LEFT, LOCKUP_SCALE, LOCKUP_TOP } from "./chapters/C05Logo";
import { C08EndCard, END_LEFT, END_SCALE, END_TOP } from "./chapters/C08EndCard";

export type LockupCheckMode = "logo-svg" | "logo-chapter" | "end-svg" | "end-chapter";

// Pixel check: the resting frames of chapters 05 and 08 against the brand lockup SVG itself.
export const LockupCheck: React.FC<{ mode: LockupCheckMode }> = ({ mode }) => {
  const svg = (left: number, top: number, scale: number) => (
    <Img
      src={brand("mluva-logo-large-mark-on-dark.svg")}
      style={{ position: "absolute", left, top, width: LOCKUP.w * scale, height: LOCKUP.h * scale }}
    />
  );
  return (
    <AbsoluteFill style={{ backgroundColor: "#000" }}>
      {mode === "logo-svg" ? svg(LOCKUP_LEFT, LOCKUP_TOP, LOCKUP_SCALE) : null}
      {mode === "end-svg" ? svg(END_LEFT, END_TOP, END_SCALE) : null}
      {mode === "logo-chapter" ? (
        <Sequence from={-80} layout="none">
          <C05Logo ambient={false} />
        </Sequence>
      ) : null}
      {mode === "end-chapter" ? (
        <Sequence from={-70} layout="none">
          <C08EndCard ambient={false} push={false} />
        </Sequence>
      ) : null}
    </AbsoluteFill>
  );
};
