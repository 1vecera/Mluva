import { AbsoluteFill, Img, staticFile, useCurrentFrame } from "remotion";
import { beatFrame } from "../timing";
import take from "../wheel-take.json";
import { BLOOM, C, display, easeIn, easeOut, LABEL, mono, ramp, ring } from "../../showreel/theme";
import { Keycap } from "../../showreel/parts/Keycap";
import { scramble } from "../../showreel/parts/Scramble";

const B0 = beatFrame(24);
const k = (beat: number) => beatFrame(24 + beat) - B0;
const LENGTH = k(8);

// The take is the full private screen; the panel shows `crop` of it at `panel_scale`, on the left,
// and every caption sits in the column to its right.
const SCREEN = { w: take.screen[0], h: take.screen[1] };
const CROP = take.crop;
const SCALE = take.panel_scale;
const PANEL = { w: CROP.w * SCALE, h: CROP.h * SCALE };
const LEFT = 120;
const TOP = 546 - PANEL.h / 2;
const COLUMN = LEFT + PANEL.w + 72;

// Take frame for a chapter frame: the wheel opens on the chapter's first beat and the hand moves
// on each later beat, as the take was recorded on the same beat (wheel-take.json `timeline`).
const OPEN = take.timeline[0].frame;
const takeFrame = (frame: number) => Math.min(take.frames - 1, Math.max(0, OPEN + frame));

// The chosen session at a chapter frame, from the take's own record of when it changed.
const chosenAt = (frame: number) => {
  let index = take.timeline[0].selected;
  let since = 0;
  take.timeline.forEach((event) => {
    if (takeFrame(frame) >= event.frame) {
      index = event.selected;
      since = event.frame - OPEN;
    }
  });
  return { fixture: take.fixtures[index], since };
};

export const U04Wheel: React.FC = () => {
  const frame = useCurrentFrame();
  const enter = ramp(frame, 0, 10, easeOut);
  const exit = ramp(frame, LENGTH - 6, LENGTH, easeIn);
  const n = takeFrame(frame);
  const { fixture, since } = chosenAt(frame);
  const local = frame - since;
  const settle = ramp(local, 0, 6);
  const pop = 1 + 0.05 * ring(local / 60, 3, 8);
  const press = frame < 9 ? Math.sin((frame / 9) * Math.PI) : 0;
  const level = fixture.caption;
  return (
    <AbsoluteFill style={{ opacity: 1 - exit, transform: `scale(${1 + 0.05 * exit})` }}>
      <div
        style={{
          position: "absolute",
          left: LEFT,
          top: TOP,
          width: PANEL.w,
          height: PANEL.h,
          overflow: "hidden",
          opacity: enter,
          transform: `scale(${0.97 + 0.03 * enter})`,
          boxShadow: "0 0 0 1px rgba(255,255,255,0.1), 0 50px 140px rgba(0,0,0,0.8)",
        }}
      >
        <Img
          src={staticFile(`universal/wheel/${String(n).padStart(5, "0")}.png`)}
          style={{
            position: "absolute",
            left: -CROP.x * SCALE,
            top: -CROP.y * SCALE,
            width: SCREEN.w * SCALE,
            height: SCREEN.h * SCALE,
            maxWidth: "none",
          }}
        />
      </div>
      {/* The column only: nothing is drawn over the captured pixels. */}
      <div style={{ position: "absolute", left: COLUMN, top: 190, display: "flex", alignItems: "center", gap: 26, opacity: ramp(frame, 0, 8) }}>
        <Keycap label="F12" size={78} press={press} lit={press} />
        <div style={{ ...mono(15, LABEL) }}>OPENS THE WHEEL</div>
      </div>
      <div style={{ position: "absolute", left: COLUMN, top: 400, width: 1792 - COLUMN, opacity: ramp(frame, 4, 12) }}>
        <div style={{ ...mono(15, LABEL) }}>CHOSEN</div>
        <div style={{ ...display(38, 800), letterSpacing: "-0.01em", marginTop: 14, transform: `scale(${pop})`, transformOrigin: "0 50%" }}>
          {scramble(fixture.kind.toUpperCase(), settle, `wk-${since}`, frame)}
        </div>
        <div style={{ ...mono(15, C.ink2), marginTop: 12 }}>{scramble(fixture.place.toUpperCase(), settle, `wp-${since}`, frame)}</div>
      </div>
      <div style={{ position: "absolute", left: COLUMN, top: 600, width: 1792 - COLUMN, opacity: ramp(frame, 4, 12) }}>
        <div style={{ ...mono(15, LABEL) }}>DELIVERY</div>
        <div
          style={{
            ...display(56, 900),
            letterSpacing: "-0.02em",
            marginTop: 12,
            color: level === "DRAFT ONLY" ? C.ink2 : C.ink,
            textShadow: level === "DRAFT ONLY" ? undefined : BLOOM,
            transform: `scale(${pop})`,
            transformOrigin: "0 50%",
          }}
        >
          {scramble(level, settle, `wl-${since}`, frame)}
        </div>
        <div style={{ ...mono(13, LABEL), marginTop: 14, letterSpacing: "0.14em" }}>{fixture.line}</div>
      </div>
      <div style={{ position: "absolute", left: COLUMN, top: 860, opacity: ramp(frame, 6, 14) }}>
        <div style={{ ...mono(13, LABEL), letterSpacing: "0.14em" }}>REAL APP · FIXTURE SESSIONS</div>
        <div style={{ ...mono(13, C.ink3), letterSpacing: "0.14em", marginTop: 8 }}>
          {take.screen[0]}×{take.screen[1]} TAKE · SHOWN AT 1:2
        </div>
      </div>
    </AbsoluteFill>
  );
};
