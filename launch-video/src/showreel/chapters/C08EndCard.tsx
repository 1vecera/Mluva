import { AbsoluteFill, Easing, useCurrentFrame } from "remotion";
import { beatFrame, DURATION } from "../timing";
import { brand, C, display, easeIn, easeInOut, easeOut, LABEL, mono, ramp, T } from "../theme";
import { lockupBoxes, LOCKUP, LockupFull } from "../parts/Logo";

const LENGTH = DURATION - beatFrame(28);
export const END_SCALE = 760 / LOCKUP.w;
export const END_LEFT = 960 - 380;
export const END_TOP = 462 - (LOCKUP.h * END_SCALE) / 2;
const BOXES = lockupBoxes(END_LEFT, END_TOP, END_SCALE);
const BOTTOM = END_TOP + LOCKUP.h * END_SCALE;
const SETTLE = 20;

// The end card cuts from the flash to the finished lockup (chapter 05 already built it), which
// settles from 1.035 to 1, then tagline, link and licence follow at +8, +14 and +20 frames.
// `ambient` false hides the glow and `push` the slow push-in, for the pixel check in LockupCheck.tsx.
export const C08EndCard: React.FC<{ ambient?: boolean; push?: boolean }> = ({ ambient = true, push = true }) => {
  const frame = useCurrentFrame();
  // Three frames of white on the final impact, then gone: no grey haze over the mark.
  const flash = frame < 3 ? 1 : ramp(frame, 3, 6, (t) => t, 1, 0);
  const settled = frame >= SETTLE;
  const settle = settled ? 1 : 1.035 - 0.035 * Easing.out(Easing.exp)(frame / SETTLE);
  const pushIn = push ? 1 + 0.02 * ramp(frame, SETTLE, LENGTH, easeInOut) : 1;
  const scale = settle * pushIn;
  const sweep = ramp(frame, 80, 100, (t) => t);
  const tagline = ramp(frame, 8, 20, easeOut);
  const pill = ramp(frame, 14, 26, easeOut);
  const meta = ramp(frame, 20, 32, easeOut);
  const fade = ramp(frame, LENGTH - 26, LENGTH - 2, easeIn, 1, 0);
  return (
    <AbsoluteFill>
      <AbsoluteFill style={{ opacity: fade, transform: scale !== 1 ? `scale(${scale})` : undefined, transformOrigin: "960px 470px" }}>
        <div
          style={{
            position: "absolute",
            left: BOXES.mark.x + BOXES.mark.w / 2 - 260,
            top: BOXES.mark.y + BOXES.mark.h / 2 - 260,
            width: 520,
            height: 520,
            borderRadius: "50%",
            background: "radial-gradient(circle, rgba(233,27,39,0.24), transparent 64%)",
            opacity: ambient ? 1 : 0,
          }}
        />
        <LockupFull left={END_LEFT} top={END_TOP} scale={END_SCALE} />
        {sweep > 0 && sweep < 1 ? (
          <div
            style={{
              position: "absolute",
              left: BOXES.mark.x,
              top: BOXES.mark.y,
              width: BOXES.mark.w,
              height: BOXES.mark.h,
              WebkitMaskImage: `url(${brand("mluva-mark-flat.svg")})`,
              WebkitMaskSize: "100% 100%",
              background: `linear-gradient(115deg, transparent ${sweep * 150 - 30}%, rgba(255,255,255,0.45) ${sweep * 150 - 16}%, transparent ${
                sweep * 150 - 4
              }%)`,
              mixBlendMode: "screen",
            }}
          />
        ) : null}
        <div
          style={{
            ...display(T.xs, 500),
            letterSpacing: "-0.01em",
            color: C.ink2,
            position: "absolute",
            left: 0,
            right: 0,
            top: BOTTOM + 30,
            textAlign: "center",
            opacity: tagline,
            transform: `translateY(${(1 - tagline) * 14}px)`,
          }}
        >
          The most delightful dictation for Omarchy.
        </div>
        <div style={{ position: "absolute", left: 0, right: 0, top: BOTTOM + 112, display: "flex", justifyContent: "center" }}>
          <div
            style={{
              ...mono(19, C.ink),
              letterSpacing: "0.06em",
              textTransform: "none",
              padding: "15px 30px",
              borderRadius: 999,
              border: "1.5px solid rgba(245,245,245,0.28)",
              background: "rgba(245,245,245,0.04)",
              opacity: pill,
              transform: `scale(${0.9 + 0.1 * pill})`,
              display: "flex",
              gap: 16,
              alignItems: "center",
            }}
          >
            github.com/1vecera/Mluva <span style={{ color: C.red }}>→</span>
          </div>
        </div>
        <div style={{ ...mono(18, LABEL), position: "absolute", left: 0, right: 0, top: BOTTOM + 196, textAlign: "center", opacity: meta }}>
          FREE · OPEN SOURCE · APACHE-2.0
        </div>
      </AbsoluteFill>
      <AbsoluteFill style={{ background: "#fff", opacity: flash, pointerEvents: "none" }} />
    </AbsoluteFill>
  );
};
