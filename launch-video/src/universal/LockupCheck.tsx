import { AbsoluteFill, Img, Sequence } from "remotion";
import { brand } from "../showreel/theme";
import { LOCKUP } from "../showreel/parts/Logo";
import { END_REST, LOCKUP_LEFT, LOCKUP_SCALE, LOCKUP_TOP, U07End } from "./chapters/U07End";

export type UniversalLockupCheckMode = "svg" | "chapter";

// Pixel check: the end chapter's resting lockup against the brand lockup SVG itself.
export const UniversalLockupCheck: React.FC<{ mode: UniversalLockupCheckMode }> = ({ mode }) => (
  <AbsoluteFill style={{ backgroundColor: "#000" }}>
    {mode === "svg" ? (
      <Img
        src={brand("mluva-logo-large-mark-on-dark.svg")}
        style={{ position: "absolute", left: LOCKUP_LEFT, top: LOCKUP_TOP, width: LOCKUP.w * LOCKUP_SCALE, height: LOCKUP.h * LOCKUP_SCALE }}
      />
    ) : (
      <Sequence from={-END_REST} layout="none">
        <U07End ambient={false} lines={false} />
      </Sequence>
    )}
  </AbsoluteFill>
);
