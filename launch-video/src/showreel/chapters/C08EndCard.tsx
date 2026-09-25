import { AbsoluteFill, useCurrentFrame } from "remotion";
import { beatFrame, DURATION } from "../timing";
import { brand, C, display, easeIn, easeOut, mono, ramp, ring } from "../theme";
import { lockupBoxes, LOCKUP, LockupFull, LockupHalf } from "../parts/Logo";

const LENGTH = DURATION - beatFrame(28);
export const END_SCALE = 760 / LOCKUP.w;
const SCALE = END_SCALE;
export const END_LEFT = 960 - 380;
const LEFT = END_LEFT;
export const END_TOP = 462 - (LOCKUP.h * SCALE) / 2;
const TOP = END_TOP;
const BOXES = lockupBoxes(LEFT, TOP, SCALE);
const BOTTOM = TOP + LOCKUP.h * SCALE;

// `ambient` false hides the glow for the pixel check in LockupCheck.tsx.
export const C08EndCard: React.FC<{ ambient?: boolean }> = ({ ambient = true }) => {
  const frame = useCurrentFrame();
  const flash = ramp(frame, 0, 10, easeOut, 1, 0);
  const mark = ramp(frame, 1, 16, easeOut);
  const markRest = frame > 60;
  const jelly = markRest ? 0 : 0.07 * ring((frame - 2) / 60, 2.4, 5);
  const word = ramp(frame, 7, 22, easeOut);
  const sweep = ramp(frame, 22, 46, (t) => t);
  const tagline = ramp(frame, 18, 32, easeOut);
  const pill = ramp(frame, 28, 40, easeOut);
  const meta = ramp(frame, 36, 48, easeOut);
  const fade = ramp(frame, LENGTH - 26, LENGTH - 2, easeIn, 1, 0);
  const s = 0.72 + 0.28 * mark;
  return (
    <AbsoluteFill>
      <AbsoluteFill style={{ opacity: fade }}>
        <div
          style={{
            position: "absolute",
            left: BOXES.mark.x + BOXES.mark.w / 2 - 260,
            top: BOXES.mark.y + BOXES.mark.h / 2 - 260,
            width: 520,
            height: 520,
            borderRadius: "50%",
            background: "radial-gradient(circle, rgba(233,27,39,0.24), transparent 64%)",
            opacity: ambient ? mark : 0,
          }}
        />
        {markRest && word >= 1 ? (
          <LockupFull left={LEFT} top={TOP} scale={SCALE} />
        ) : (
          <>
        <AbsoluteFill
          style={{
            transformOrigin: `${BOXES.mark.x + BOXES.mark.w / 2}px ${BOXES.mark.y + BOXES.mark.h * 0.6}px`,
            transform: markRest ? undefined : `scale(${s * (1 + jelly)}, ${s * (1 - jelly)})`,
            opacity: mark,
            filter: markRest ? undefined : `blur(${(1 - mark) * 10}px)`,
          }}
        >
          <LockupHalf part="mark" left={LEFT} top={TOP} scale={SCALE} />
        </AbsoluteFill>
        <LockupHalf
          part="word"
          left={LEFT}
          top={TOP}
          scale={SCALE}
          transform={word >= 1 ? "" : `translateX(${(1 - word) * -24}px)`}
          style={
            word >= 1
              ? undefined
              : {
                  clipPath: `inset(-20% ${(1 - word) * (1 - (BOXES.word.x - LEFT) / (LOCKUP.w * SCALE)) * 100}% -20% 0)`,
                  filter: `blur(${(1 - word) * 5}px)`,
                }
          }
        />
          </>
        )}
        {sweep > 0 && sweep < 1 ? (
          <div
            style={{
              position: "absolute",
              left: LEFT,
              top: TOP,
              width: LOCKUP.w * SCALE,
              height: LOCKUP.h * SCALE,
              WebkitMaskImage: `url(${brand("mluva-logo-large-mark-on-dark.svg")})`,
              WebkitMaskSize: "100% 100%",
              background: `linear-gradient(110deg, transparent ${sweep * 150 - 30}%, rgba(255,255,255,0.7) ${sweep * 150 - 18}%, transparent ${
                sweep * 150 - 6
              }%)`,
              mixBlendMode: "screen",
            }}
          />
        ) : null}
        <div
          style={{
            ...display(38, 500),
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
        <div style={{ ...mono(14, C.ink3), position: "absolute", left: 0, right: 0, top: BOTTOM + 196, textAlign: "center", opacity: meta }}>
          FREE · OPEN SOURCE · APACHE-2.0
        </div>
      </AbsoluteFill>
      <AbsoluteFill style={{ background: "#fff", opacity: flash, pointerEvents: "none" }} />
    </AbsoluteFill>
  );
};
