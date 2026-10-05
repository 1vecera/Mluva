import { AbsoluteFill, Freeze, interpolate, OffthreadVideo, Sequence, staticFile, useCurrentFrame } from "remotion";
import { Background, Grain } from "../showreel/parts/Background";
import { BlurDefs } from "../showreel/parts/Blur";
import { LOCKUP, LockupFull, Mark } from "../showreel/parts/Logo";
import { MarkPeriod } from "../showreel/parts/MarkPeriod";
import { C, display, easeIn, easeInOut, easeOut, LABEL, MONO, mono, ramp, ring, T } from "../showreel/theme";

// Rust-release highlight: 27 s, 1920×1080, 60 fps, silent. Numbers come from
// docs/verification/rust-release-performance/release-startup.json (Python 1.6.0 vs Rust 2.0.0,
// same host, warm cache, six measured starts per version, medians).
export const FPS = 60;
export const SCENES = {
  title: { from: 0, length: 150 },
  dictate: { from: 150, length: 420 },
  polish: { from: 570, length: 300 },
  measured: { from: 870, length: 330 },
  local: { from: 1200, length: 240 },
  end: { from: 1440, length: 180 },
};
export const HIGHLIGHT_DURATION = 1620;

const STARTUP = { python: 456, rust: 260, pct: "−43%" };
const MEMORY = { python: 260.3, rust: 236.5, pct: "−9%" };

const fade = (frame: number, length: number, inFrames = 12, outFrames = 12) =>
  Math.min(ramp(frame, 0, inFrames, easeOut), ramp(frame, length - outFrames, length, easeIn, 1, 0));

const Label: React.FC<{ children: React.ReactNode; color?: string; size?: number }> = ({ children, color = LABEL, size = 20 }) => (
  <div style={{ ...mono(size, color), letterSpacing: "0.18em" }}>{children}</div>
);

// The captured native window, cropped above its status line, with the bottom edge fading out.
const WINDOW_HEIGHT = 700;
const Video: React.FC<{ clip: string; width: number }> = ({ clip, width }) => (
  <OffthreadVideo src={staticFile(`highlight/${clip}.mp4`)} muted style={{ width, height: WINDOW_HEIGHT, display: "block" }} />
);

// The captured native window (pixels are only cropped and scaled), with its cropped bottom edge fading out.
const Window: React.FC<{ clip: string; source: [number, number]; length: number; frame: number }> = ({ clip, source, length, frame }) => {
  const rise = ramp(frame, 0, 22, easeOut);
  const width = Math.round((source[0] * WINDOW_HEIGHT) / source[1]);
  return (
    <div
      style={{
        position: "absolute",
        left: (1920 - width) / 2,
        top: 270,
        width,
        height: WINDOW_HEIGHT,
        opacity: rise,
        transform: `translateY(${(1 - rise) * 24}px)`,
        borderRadius: 16,
        overflow: "hidden",
        boxShadow: "0 40px 140px rgba(233,27,39,0.16), 0 0 0 1px rgba(245,245,245,0.14)",
        WebkitMaskImage: "linear-gradient(to bottom, #000 90%, transparent 100%)",
      }}
    >
      <Sequence durationInFrames={length} layout="none">
        <Video clip={clip} width={width} />
      </Sequence>
      <Sequence from={length} layout="none">
        <Freeze frame={length - 1}>
          <Video clip={clip} width={width} />
        </Freeze>
      </Sequence>
    </div>
  );
};

const Step: React.FC<{ n: string; title: string; sub: React.ReactNode; frame: number }> = ({ n, title, sub, frame }) => {
  const a = ramp(frame, 4, 22, easeOut);
  const b = ramp(frame, 14, 32, easeOut);
  return (
    <div style={{ position: "absolute", left: 96, top: 62, right: 96, display: "flex", alignItems: "flex-end", gap: 56 }}>
      <div style={{ opacity: a, transform: `translateY(${(1 - a) * 16}px)` }}>
        <Label color={C.red}>{n}</Label>
        <div style={{ ...display(T.m), marginTop: 18 }}>
          {title}
          <span style={{ color: C.red }}>.</span>
        </div>
      </div>
      <div
        style={{
          paddingBottom: 12,
          fontFamily: "'Adwaita Sans', sans-serif",
          fontSize: 34,
          lineHeight: 1.25,
          fontWeight: 500,
          color: C.ink2,
          opacity: b,
          transform: `translateY(${(1 - b) * 14}px)`,
        }}
      >
        {sub}
      </div>
    </div>
  );
};

const Footnote: React.FC<{ children: React.ReactNode; frame: number }> = ({ children, frame }) => (
  <div style={{ position: "absolute", left: 96, right: 96, bottom: 52, opacity: ramp(frame, 30, 48, easeOut) }}>
    <div style={{ ...mono(21, "#A0A0A0"), letterSpacing: "0.12em", whiteSpace: "normal", lineHeight: 1.5 }}>{children}</div>
  </div>
);

const Title: React.FC = () => {
  const frame = useCurrentFrame();
  const o = fade(frame, SCENES.title.length, 4, 14);
  const a = ramp(frame, 6, 30, easeOut);
  const b = ramp(frame, 22, 46, easeOut);
  return (
    <AbsoluteFill style={{ opacity: o }}>
      <div style={{ position: "absolute", left: 96, top: 380 }}>
        <Label color={C.red}>Mluva 2.0 · Released</Label>
        <div style={{ ...display(T.l), marginTop: 28, opacity: a, transform: `translateY(${(1 - a) * 30}px)` }}>Now native</div>
        <div style={{ ...display(T.l), marginTop: 8, display: "flex", alignItems: "flex-end", opacity: b, transform: `translateY(${(1 - b) * 30}px)` }}>
          <span>Rust</span>
          <MarkPeriod fontSize={T.l} local={frame} at={34} after="Rust" />
        </div>
      </div>
      <Mark
        box={{ x: 1180, y: 250 + 10 * Math.sin(frame / 22), w: 520, h: 520 }}
        style={{ opacity: ramp(frame, 20, 44, easeOut), transform: `scale(${0.9 + 0.1 * ramp(frame, 20, 50, easeOut) + 0.05 * ring((frame - 30) / 60, 2.4, 6)})` }}
      />
      <div style={{ position: "absolute", left: 96, bottom: 64, opacity: ramp(frame, 40, 60, easeOut) }}>
        <Label size={19}>Same quiet dictation workspace · no Python runtime</Label>
      </div>
    </AbsoluteFill>
  );
};

const Dictate: React.FC = () => {
  const frame = useCurrentFrame();
  const { length } = SCENES.dictate;
  return (
    <AbsoluteFill style={{ opacity: fade(frame, length) }}>
      <Step n="01 / Dictate" title="Speak" sub={<>Qwen3-ASR 1.7B turns it into text<br />on this computer.</>} frame={frame} />
      <Window clip="dictation" source={[2120, 880]} length={408} frame={frame} />
      <Footnote frame={frame}>Real app, published 2.0.0 · synthetic speech · time-lapse (×2, wait ×4)</Footnote>
    </AbsoluteFill>
  );
};

const Polish: React.FC = () => {
  const frame = useCurrentFrame();
  const { length } = SCENES.polish;
  return (
    <AbsoluteFill style={{ opacity: fade(frame, length) }}>
      <Step n="02 / Shape" title="Polish" sub={<>Keep the original and get a cleaner<br />draft beside it.</>} frame={frame} />
      <Window clip="polish" source={[2120, 1060]} length={276} frame={frame} />
      <Footnote frame={frame}>Real app · rewrite text is a prepared example from a local test provider</Footnote>
    </AbsoluteFill>
  );
};

const Bar: React.FC<{ label: string; value: string; fraction: number; progress: number; hot?: boolean }> = ({ label, value, fraction, progress, hot }) => (
  <div style={{ display: "flex", alignItems: "center", gap: 28, height: 70 }}>
    <div style={{ width: 210, ...mono(20, hot ? C.ink : LABEL), letterSpacing: "0.14em" }}>{label}</div>
    <div style={{ width: 880, height: 30, background: "rgba(245,245,245,0.06)", borderRadius: 3, overflow: "hidden" }}>
      <div
        style={{
          width: `${fraction * 100 * progress}%`,
          height: "100%",
          background: hot ? C.red : "rgba(245,245,245,0.35)",
          boxShadow: hot ? `0 0 28px ${C.redGlow}` : undefined,
        }}
      />
    </div>
    <div style={{ fontFamily: MONO, fontSize: 34, color: hot ? C.ink : C.ink2, width: 190 }}>{value}</div>
  </div>
);

const Stat: React.FC<{
  title: string;
  python: string;
  rust: string;
  pct: string;
  bars: [number, number];
  progress: number;
  top: number;
}> = ({ title, python, rust, pct, bars, progress, top }) => (
  <div style={{ position: "absolute", left: 96, top, width: 1728 }}>
    <div style={{ display: "flex", alignItems: "baseline", gap: 44 }}>
      <div style={{ ...display(T.s, 800), width: 540 }}>{title}</div>
      <div style={{ ...display(T.s, 800), color: C.ink3 }}>{python}</div>
      <div style={{ ...display(T.s, 500), color: C.ink3 }}>→</div>
      <div style={{ ...display(T.s, 900) }}>{rust}</div>
      <div
        style={{
          ...display(T.xs, 800),
          color: "#fff",
          background: C.red,
          borderRadius: 10,
          padding: "8px 18px",
          opacity: ramp(progress, 0.55, 0.9, easeOut),
          boxShadow: `0 0 36px ${C.redGlow}`,
        }}
      >
        {pct}
      </div>
    </div>
    <div style={{ marginTop: 22 }}>
      <Bar label="Python 1.6.0" value={python} fraction={1} progress={ramp(progress, 0, 0.5, easeInOut)} />
      <Bar label="Rust 2.0.0" value={rust} fraction={bars[1] / bars[0]} progress={ramp(progress, 0.25, 0.8, easeInOut)} hot />
    </div>
  </div>
);

const Measured: React.FC = () => {
  const frame = useCurrentFrame();
  const { length } = SCENES.measured;
  const head = ramp(frame, 0, 20, easeOut);
  return (
    <AbsoluteFill style={{ opacity: fade(frame, length) }}>
      <div style={{ position: "absolute", left: 96, top: 92, opacity: head }}>
        <Label color={C.red}>03 / Measured</Label>
        <div style={{ ...display(T.s, 800), marginTop: 18 }}>Opens sooner. Uses less memory.</div>
      </div>
      <Stat
        title="First window"
        python={`${STARTUP.python} ms`}
        rust={`${STARTUP.rust} ms`}
        pct={STARTUP.pct}
        bars={[STARTUP.python, STARTUP.rust]}
        progress={ramp(frame, 24, 120, easeInOut)}
        top={330}
      />
      <Stat
        title="Resident memory"
        python={`${MEMORY.python} MiB`}
        rust={`${MEMORY.rust} MiB`}
        pct={MEMORY.pct}
        bars={[MEMORY.python, MEMORY.rust]}
        progress={ramp(frame, 70, 170, easeInOut)}
        top={640}
      />
      <Footnote frame={frame}>
        Python 1.6.0 vs Rust 2.0.0 · same host, warm cache · medians of six starts each · first window and process-tree RSS, not an inference-speed claim
      </Footnote>
    </AbsoluteFill>
  );
};

const Chip: React.FC<{ name: string; tag?: string; frame: number; at: number }> = ({ name, tag, frame, at }) => {
  const p = ramp(frame, at, at + 16, easeOut);
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 16,
        marginBottom: 0,
        opacity: p,
        transform: `translateX(${(1 - p) * -18}px)`,
        fontFamily: "'Adwaita Sans', sans-serif",
        fontSize: 34,
        fontWeight: 700,
        color: C.ink,
      }}
    >
      <span style={{ width: 12, height: 12, borderRadius: 6, background: tag ? C.red : C.ink3, boxShadow: tag ? `0 0 14px ${C.redGlow}` : undefined }} />
      {name}
      {tag ? <span style={{ ...mono(17, C.red), letterSpacing: "0.16em" }}>{tag}</span> : null}
    </div>
  );
};

const Local: React.FC = () => {
  const frame = useCurrentFrame();
  const { length } = SCENES.local;
  return (
    <AbsoluteFill style={{ opacity: fade(frame, length) }}>
      <Step
        n="04 / Local"
        title="On device"
        frame={frame}
        sub={
          <div style={{ display: "flex", gap: 40, marginBottom: 2 }}>
            <Chip name="Qwen3-ASR 1.7B" tag="DEFAULT" frame={frame} at={22} />
            <Chip name="Parakeet v3" frame={frame} at={34} />
            <Chip name="Whisper Tiny" frame={frame} at={46} />
          </div>
        }
      />
      <Window clip="providers" source={[2120, 1050]} length={216} frame={frame} />
      <Footnote frame={frame}>Real Settings · local models keep audio on this device · no Python runtime</Footnote>
    </AbsoluteFill>
  );
};

const End: React.FC = () => {
  const frame = useCurrentFrame();
  const { length } = SCENES.end;
  const scale = 640 / LOCKUP.w;
  const left = 960 - 320;
  const top = 280;
  const a = ramp(frame, 6, 24, easeOut);
  const b = ramp(frame, 16, 34, easeOut);
  const c = ramp(frame, 26, 44, easeOut);
  return (
    <AbsoluteFill style={{ opacity: Math.min(ramp(frame, 0, 8, easeOut), ramp(frame, length - 20, length - 2, easeIn, 1, 0)) }}>
      <div
        style={{
          position: "absolute",
          left: 960 - 280,
          top: top + (LOCKUP.h * scale) / 2 - 280 - 20,
          width: 560,
          height: 560,
          borderRadius: "50%",
          background: "radial-gradient(circle, rgba(233,27,39,0.22), transparent 64%)",
        }}
      />
      <LockupFull left={left} top={top} scale={scale} style={{ opacity: a, transform: `scale(${0.96 + 0.04 * a})`, transformOrigin: "960px 340px" }} />
      <div
        style={{
          ...display(T.s, 700),
          position: "absolute",
          left: 0,
          right: 0,
          top: top + LOCKUP.h * scale + 54,
          textAlign: "center",
          opacity: b,
          transform: `translateY(${(1 - b) * 14}px)`,
        }}
      >
        Native Rust. Same <span style={{ color: C.red }}>quiet</span> workspace.
      </div>
      <div style={{ position: "absolute", left: 0, right: 0, top: top + LOCKUP.h * scale + 170, display: "flex", justifyContent: "center", opacity: c }}>
        <div style={{ fontFamily: MONO, fontSize: 34, color: C.ink, border: `1px solid ${C.line}`, borderRadius: 999, padding: "14px 34px" }}>
          github.com/1vecera/Mluva
        </div>
      </div>
      <div style={{ position: "absolute", left: 0, right: 0, bottom: 64, textAlign: "center", opacity: c }}>
        <Label size={19}>Open source · Apache-2.0 · Omarchy x86_64 · Release 2.0.0</Label>
      </div>
    </AbsoluteFill>
  );
};

export const MluvaRustHighlight: React.FC = () => {
  const frame = useCurrentFrame();
  const glowX = interpolate(frame, [0, HIGHLIGHT_DURATION], [0.3, 0.7], { extrapolateRight: "clamp" });
  return (
    <AbsoluteFill style={{ backgroundColor: "#000" }}>
      <BlurDefs />
      <Background glowX={glowX} glowY={0.5} />
      {(
        [
          ["title", Title],
          ["dictate", Dictate],
          ["polish", Polish],
          ["measured", Measured],
          ["local", Local],
          ["end", End],
        ] as const
      ).map(([key, View]) => (
        <Sequence key={key} from={SCENES[key].from} durationInFrames={SCENES[key].length} layout="none">
          <View />
        </Sequence>
      ))}
      <Grain />
    </AbsoluteFill>
  );
};
