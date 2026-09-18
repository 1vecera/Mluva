import { Audio } from "@remotion/media";
import { Sequence, staticFile, useVideoConfig } from "remotion";
import { Camera, useT } from "../components/Camera";
import { Window } from "../components/Desktop";
import { Keycap } from "../components/Keycap";
import { Recorder } from "../components/Recorder";
import { EASE_IN_OUT, FONT, NORD, hexToRgba, revealed, tween } from "../theme";
import type { SceneTiming } from "../timing";

export const DICTATION = "Could we move the review to Tuesday? I'll send the draft tomorrow.";
const WORDS = DICTATION.split(" ").length;

const PRESS_1 = 0.75;
const WORDS_FROM = 1.05;
const CADENCE = 0.13;
const PRESS_2 = 3.7;
const PASTE = 4.5;

const pressCurve = (t: number, at: number) => Math.min(tween(t, [at - 0.12, at], [0, 1]), tween(t, [at, at + 0.28], [1, 0]));

/** Quick zoom bump that lands on a beat: up in 0.12 s, ease back over the next 0.35 s. */
const punch = (t: number, at: number, amp = 0.035) =>
  Math.min(tween(t, [at, at + 0.12], [0, amp]), tween(t, [at + 0.12, at + 0.47], [amp, 0]));

/** S03 · F9, talk, F9, paste. One key, one widget, one editor. */
export const S03F9: React.FC<{ scene: SceneTiming }> = () => {
  const t = useT();
  const { fps } = useVideoConfig();
  const press = Math.max(pressCurve(t, PRESS_1), pressCurve(t, PRESS_2));
  const state: "idle" | "recording" | "ready" = t < PRESS_1 + 0.05 ? "idle" : t < PRESS_2 ? "recording" : "ready";
  const visible = revealed(t, WORDS_FROM, CADENCE, WORDS);
  const label = t < PRESS_1 ? "Press F9" : t < PRESS_2 - 0.4 ? "Speak" : t < PASTE - 0.2 ? "Press F9 again" : "Pasted";
  const stepAt = t < PRESS_1 ? 0 : t < PRESS_2 - 0.4 ? PRESS_1 : t < PASTE - 0.2 ? PRESS_2 - 0.4 : PASTE - 0.2;
  const stepP = tween(t, [stepAt, stepAt + 0.25], [0, 1]);
  const ghost = Math.min(tween(t, [PASTE - 0.38, PASTE - 0.24], [0, 1]), tween(t, [PASTE - 0.1, PASTE], [1, 0]));
  const ghostP = tween(t, [PASTE - 0.38, PASTE], [0, 1], EASE_IN_OUT);
  const pasted = t >= PASTE;
  const flash = tween(t, [PASTE, PASTE + 0.9], [0.5, 0]);
  const idlePulse = 0.5 + 0.5 * Math.sin((t / 2.4) * Math.PI * 2);
  const idleGlow = t < PRESS_1 ? `drop-shadow(0 0 ${18 + 10 * idlePulse}px ${hexToRgba(NORD.frost, 0.25 + 0.2 * idlePulse)})` : "none";
  return (
    <>
      <Camera zoom={1.07 + punch(t, PRESS_1) + punch(t, PRESS_2) + punch(t, PASTE, 0.045)} x={tween(t, [3.4, 5.0], [70, -80], EASE_IN_OUT)} y={24}>
        <div style={{ position: "absolute", left: 210, top: 360, display: "flex", flexDirection: "column", alignItems: "center", gap: 34 }}>
          <div style={{ filter: idleGlow }}>
            <Keycap label="F9" size={210} press={press} palette={NORD} />
          </div>
          <div style={{ fontFamily: FONT, fontSize: 26, fontWeight: 500, color: NORD.fg, opacity: 0.9 * stepP, transform: `translateY(${(1 - stepP) * 10}px)` }}>{label}</div>
        </div>
        <div style={{ position: "absolute", left: 540, top: 330 }}>
          <Recorder width={760} palette={NORD} text={DICTATION} visibleWords={visible} state={state} seconds={state === "recording" ? t - PRESS_1 : state === "ready" ? PRESS_2 - PRESS_1 : 0} t={t} lines={4} />
        </div>
        <Window rect={{ x: 1350, y: 300, w: 470, h: 470 }} palette={NORD} title="meeting-notes.md — editor" active={pasted}>
          <div style={{ padding: "22px 24px", fontFamily: FONT, fontSize: 19, lineHeight: 1.55, color: NORD.fg, display: "flex", flexDirection: "column", gap: 16 }}>
            <div style={{ color: NORD.fgStrong, fontWeight: 700 }}>## Tuesday review</div>
            <div style={{ opacity: 0.55 }}>- agenda: rollout metrics, logo study</div>
            <div style={{ opacity: 0.55 }}>- owner: Daniel</div>
            <div style={{ background: hexToRgba(NORD.frost, flash), borderRadius: 6, margin: "0 -6px", padding: "2px 6px", color: NORD.fgStrong, minHeight: 30 }}>
              {pasted ? DICTATION : ""}
              <span style={{ display: "inline-block", width: 2, height: 20, background: NORD.frost, marginLeft: 2, verticalAlign: "text-bottom", opacity: Math.floor(t * 2.2) % 2 === 0 ? 1 : 0 }} />
            </div>
          </div>
        </Window>
        <div
          style={{
            position: "absolute",
            left: 560 + ghostP * 830,
            top: 400 - ghostP * 40,
            width: 700 - ghostP * 280,
            opacity: ghost,
            transform: `scale(${1 - ghostP * 0.35})`,
            fontFamily: FONT,
            fontSize: 24,
            lineHeight: 1.4,
            color: NORD.fgStrong,
            background: hexToRgba(NORD.deep, 0.85),
            border: `1px solid ${hexToRgba(NORD.frost, 0.6)}`,
            padding: 14,
            pointerEvents: "none",
          }}
        >
          {DICTATION}
        </div>
      </Camera>
      <Sequence from={Math.round(PRESS_1 * fps)} layout="none">
        <Audio src={staticFile("sfx/switch.wav")} volume={0.3} />
      </Sequence>
      <Sequence from={Math.round(PRESS_2 * fps)} layout="none">
        <Audio src={staticFile("sfx/switch.wav")} volume={0.3} />
      </Sequence>
      <Sequence from={Math.round(PASTE * fps)} layout="none">
        <Audio src={staticFile("sfx/ding.wav")} volume={0.28} />
      </Sequence>
    </>
  );
};
