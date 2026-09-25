import { AbsoluteFill, useCurrentFrame } from "remotion";
import { beatAt, chapterAt, DURATION, FPS } from "../timing";
import { C, easeOut, mono, ramp } from "../theme";
import { scramble } from "./Scramble";

const pad = (n: number) => String(n).padStart(2, "0");
const INSET = 56;
const ARM = 22;

const Bracket: React.FC<{ corner: "tl" | "tr" | "bl" | "br"; offset: number; opacity: number }> = ({
  corner,
  offset,
  opacity,
}) => {
  const top = corner[0] === "t";
  const left = corner[1] === "l";
  return (
    <div
      style={{
        position: "absolute",
        width: ARM,
        height: ARM,
        opacity,
        [top ? "top" : "bottom"]: INSET - offset,
        [left ? "left" : "right"]: INSET - offset,
        borderColor: C.ink2,
        borderStyle: "solid",
        borderWidth: 0,
        [top ? "borderTopWidth" : "borderBottomWidth"]: 1.5,
        [left ? "borderLeftWidth" : "borderRightWidth"]: 1.5,
      }}
    />
  );
};

export const Hud: React.FC<{ tone?: number }> = ({ tone = 1 }) => {
  const frame = useCurrentFrame();
  const boot = ramp(frame, 0, 22, easeOut);
  const chapter = chapterAt(frame);
  const since = frame - chapter.from;
  const label = scramble(chapter.name, ramp(since, 0, 12), `hud-${chapter.index}`, frame);
  const beat = Math.floor(beatAt(frame));
  const beatInBar = ((beat % 4) + 4) % 4;
  const secs = Math.floor(frame / FPS);
  const timecode = `00:00:${pad(secs)}:${pad(frame % FPS)}`;
  const progress = Math.min(1, frame / (DURATION - 1));
  const fade = ramp(frame, DURATION - 24, DURATION - 2, easeOut, 1, 0) * tone;
  const type = (text: string, delay: number) => scramble(text, ramp(frame, delay, delay + 16), text, frame);

  return (
    <AbsoluteFill style={{ opacity: fade, pointerEvents: "none" }}>
      {(["tl", "tr", "bl", "br"] as const).map((corner) => (
        <Bracket key={corner} corner={corner} offset={(1 - boot) * 18} opacity={boot} />
      ))}
      <div style={{ position: "absolute", left: 94, top: 62 }}>
        <div style={mono(15, C.ink)}>{type("SHOWREEL '26", 2)}</div>
        <div style={{ ...mono(15, C.ink3), marginTop: 9 }}>{type("MOTION DESIGN — MLUVA", 6)}</div>
        <div style={{ display: "flex", gap: 6, marginTop: 12, opacity: boot }}>
          {[0, 1, 2, 3].map((i) => (
            <div
              key={i}
              style={{
                width: 8,
                height: 8,
                background: i === beatInBar && frame > 0 ? C.red : C.ink4,
                boxShadow: i === beatInBar ? `0 0 10px ${C.redGlow}` : "none",
              }}
            />
          ))}
        </div>
      </div>
      <div style={{ position: "absolute", right: 94, top: 62, textAlign: "right" }}>
        <div style={{ ...mono(15, C.ink), letterSpacing: "0.16em", opacity: boot }}>{timecode}</div>
        <div style={{ ...mono(15, C.ink3), marginTop: 9 }}>{type("1920×1080 · 60 FPS · 128 BPM", 8)}</div>
      </div>
      <div style={{ position: "absolute", left: 94, bottom: 70 }}>
        <div style={{ ...mono(14, C.ink3), opacity: boot }}>/ 08</div>
        <div style={{ ...mono(15, C.ink), marginTop: 9, opacity: boot }}>
          <span style={{ color: C.red }}>{pad(chapter.index)}</span>
          <span style={{ marginLeft: 14 }}>{label}</span>
        </div>
      </div>
      <div style={{ position: "absolute", right: 94, bottom: 70, textAlign: "right" }}>
        <div style={{ width: 220, height: 1, background: C.ink4, marginLeft: "auto", position: "relative", opacity: boot }}>
          <div
            style={{
              position: "absolute",
              left: 0,
              top: -0.5,
              height: 2,
              width: 220 * progress,
              background: C.red,
              boxShadow: `0 0 8px ${C.redGlow}`,
            }}
          />
        </div>
        <div style={{ ...mono(15, C.ink), marginTop: 16 }}>{type("GITHUB.COM/1VECERA/MLUVA", 10)}</div>
      </div>
    </AbsoluteFill>
  );
};
