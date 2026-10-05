import { AbsoluteFill, Audio, interpolate, Sequence, staticFile, useCurrentFrame } from "remotion";
import { Background, Grain } from "../showreel/parts/Background";
import { BlurDefs } from "../showreel/parts/Blur";
import { LOCKUP, LockupFull, Mark } from "../showreel/parts/Logo";
import { MarkPeriod } from "../showreel/parts/MarkPeriod";
import { C, display, easeIn, easeInOut, easeOut, LABEL, MONO, mono, ramp, ring, T } from "../showreel/theme";
import score from "./score.json";

// Rust-release highlight about the opening speed-up. Every scene change lands on a drop of the
// generated score (score.json, measured by analyze_score.py). Figures come from
// docs/verification/rust-release-performance/release-startup.json (Python 1.6.0 vs Rust 2.0.0, same
// host, warm cache, six measured starts per version, medians) and highlight/README.md (install size).
export const FPS = 60;
const f = (seconds: number) => Math.round(seconds * FPS);
const D = score.drops_s;
// The score is a four-on-the-floor kick at 130 BPM; the pulse is 1 on each kick and decays before the next.
const kick = (globalFrame: number) => {
  const phase = ((((globalFrame / FPS - score.kick_s) / score.beat_s) % 1) + 1) % 1;
  return Math.exp(-9 * phase);
};
export const SCENES = {
  title: { from: 0, to: f(D.d1) },
  startup: { from: f(D.d1), to: f(D.d2) },
  ready: { from: f(D.d2), to: f(D.d3) },
  size: { from: f(D.d3), to: f(D.end) },
  end: { from: f(D.end), to: f(score.duration_s) },
};
export const HIGHLIGHT_DURATION = f(score.duration_s);

// Medians (ms, MB) and their ratios.
const STARTUP = { python: 455.6, rust: 259.8 };
const READY = { python: 216.5, rust: 41.8 };
const SIZE = { python: 161, rust: 49 }; // MiB: 151 environment + 10 app files vs the unpacked 2.0.0 bundle
const ratio = (m: { python: number; rust: number }) => m.python / m.rust;

const Label: React.FC<{ children: React.ReactNode; color?: string; size?: number }> = ({ children, color = LABEL, size = 22 }) => (
  <div style={{ ...mono(size, color), letterSpacing: "0.18em" }}>{children}</div>
);

// Three frames of white on a drop, then gone.
const Flash: React.FC = () => {
  const frame = useCurrentFrame();
  const o = frame < 2 ? 0.9 : ramp(frame, 2, 9, (t) => t, 0.9, 0);
  return o > 0 ? <AbsoluteFill style={{ background: "#fff", opacity: o, mixBlendMode: "screen" }} /> : null;
};

const Footnote: React.FC<{ children: React.ReactNode; frame: number }> = ({ children, frame }) => (
  <div style={{ position: "absolute", left: 96, right: 96, bottom: 52, opacity: ramp(frame, 30, 46, easeOut) }}>
    <div style={{ ...mono(21, "#A0A0A0"), letterSpacing: "0.1em", whiteSpace: "normal", lineHeight: 1.5 }}>{children}</div>
  </div>
);

const Bar: React.FC<{ label: string; value: string; fraction: number; progress: number; hot?: boolean; pulse?: number }> = ({ label, value, fraction, progress, hot, pulse = 0 }) => (
  <div style={{ display: "flex", alignItems: "center", gap: 28, height: 84 }}>
    <div style={{ width: 230, ...mono(22, hot ? C.ink : LABEL), letterSpacing: "0.12em" }}>{label}</div>
    <div style={{ width: 760, height: 38, background: "rgba(245,245,245,0.06)", borderRadius: 3, overflow: "hidden" }}>
      <div
        style={{
          width: `${fraction * 100 * progress}%`,
          height: "100%",
          background: hot ? C.red : "rgba(245,245,245,0.35)",
          boxShadow: hot ? `0 0 ${24 + 26 * pulse}px ${C.redGlow}` : undefined,
        }}
      />
    </div>
    <div style={{ fontFamily: MONO, fontSize: 38, color: hot ? C.ink : C.ink2, width: 220 }}>{value}</div>
  </div>
);

// One stat scene: a multiplier counting up on the drop, the claim, and a Python-vs-Rust bar pair.
const Stat: React.FC<{
  index: string;
  multiplier: number;
  suffix: string;
  headline: string;
  python: { label: string; value: string; amount: number };
  rust: { label: string; value: string; amount: number };
  note: React.ReactNode;
  extra?: string;
  start: number;
  length: number;
}> = ({ index, multiplier, suffix, headline, python, rust, note, extra, start, length }) => {
  const frame = useCurrentFrame();
  const pulse = kick(start + frame);
  const count = ramp(frame, 0, 40, easeOut);
  const shown = 1 + (multiplier - 1) * count;
  const settle = 1 + 0.06 * ring(Math.max(0, frame - 36) / 60, 3, 7) + 0.012 * pulse * ramp(frame, 40, 50, easeOut);
  const bars = ramp(frame, 6, 52, easeInOut);
  const out = ramp(frame, length - 8, length, easeIn, 1, 0);
  return (
    <AbsoluteFill style={{ opacity: out }}>
      <Flash />
      <div style={{ position: "absolute", left: 96, top: 78 }}>
        <Label color={C.red}>{index}</Label>
      </div>
      <div style={{ position: "absolute", left: 96, top: 150, display: "flex", alignItems: "baseline", transform: `scale(${settle})`, transformOrigin: "0 60%" }}>
        <span style={{ ...display(330), letterSpacing: "-0.05em" }}>{shown.toFixed(multiplier >= 3 ? 1 : 2)}</span>
        <span style={{ ...display(330), color: C.red, letterSpacing: "-0.05em", marginLeft: 8 }}>×</span>
        <span style={{ ...display(T.m, 700), color: C.ink2, marginLeft: 40 }}>{suffix}</span>
      </div>
      <div style={{ position: "absolute", left: 96, top: 530, opacity: ramp(frame, 14, 30, easeOut), ...display(T.s, 800) }}>{headline}</div>
      <div style={{ position: "absolute", left: 96, top: 660, opacity: ramp(frame, 20, 36, easeOut) }}>
        <Bar label={python.label} value={python.value} fraction={1} progress={bars} />
        <Bar label={rust.label} value={rust.value} fraction={rust.amount / python.amount} progress={ramp(frame, 12, 58, easeInOut)} hot pulse={pulse} />
      </div>
      {extra ? (
        <div style={{ position: "absolute", left: 96, top: 905, opacity: ramp(frame, 140, 160, easeOut), fontFamily: "'Adwaita Sans', sans-serif", fontSize: 34, fontWeight: 600, color: C.ink2 }}>
          {extra}
        </div>
      ) : null}
      <Footnote frame={frame}>{note}</Footnote>
    </AbsoluteFill>
  );
};

const Title: React.FC<{ length: number }> = ({ length }) => {
  const frame = useCurrentFrame();
  const a = ramp(frame, 8, 34, easeOut);
  const b = ramp(frame, 22, 48, easeOut);
  // Tension before the drop: the mark swells and the label flickers.
  const build = ramp(frame, length - 70, length, easeIn);
  return (
    <AbsoluteFill>
      <div style={{ position: "absolute", left: 96, top: 380 }}>
        <div style={{ opacity: 1 - 0.5 * Math.abs(Math.sin(frame * 0.9)) * build }}>
          <Label color={C.red}>Mluva 2.0 · Native Rust</Label>
        </div>
        <div style={{ ...display(T.l), marginTop: 28, opacity: a, transform: `translateY(${(1 - a) * 30}px)` }}>Opens</div>
        <div style={{ ...display(T.l), marginTop: 8, display: "flex", alignItems: "flex-end", opacity: b, transform: `translateY(${(1 - b) * 30}px)` }}>
          <span>faster</span>
          <MarkPeriod fontSize={T.l} local={frame} at={40} after="faster" />
        </div>
      </div>
      <Mark
        box={{ x: 1180, y: 250 + 10 * Math.sin(frame / 22), w: 520, h: 520 }}
        style={{ opacity: ramp(frame, 20, 44, easeOut), transform: `scale(${0.9 + 0.1 * ramp(frame, 20, 50, easeOut) + 0.12 * build})` }}
      />
      <div style={{ position: "absolute", left: 96, bottom: 52, opacity: ramp(frame, 40, 60, easeOut) }}>
        <Label size={21}>Published release 2.0.0 · no Python runtime</Label>
      </div>
    </AbsoluteFill>
  );
};

const End: React.FC<{ length: number }> = ({ length }) => {
  const frame = useCurrentFrame();
  const scale = 640 / LOCKUP.w;
  const left = 960 - 320;
  const top = 280;
  const settle = 1.035 - 0.035 * ramp(frame, 0, 20, easeOut);
  const b = ramp(frame, 12, 30, easeOut);
  const c = ramp(frame, 22, 40, easeOut);
  return (
    <AbsoluteFill style={{ opacity: ramp(frame, length - 24, length - 2, easeIn, 1, 0) }}>
      <Flash />
      <div
        style={{
          position: "absolute",
          left: 960 - 280,
          top: top + (LOCKUP.h * scale) / 2 - 300,
          width: 560,
          height: 560,
          borderRadius: "50%",
          background: "radial-gradient(circle, rgba(233,27,39,0.24), transparent 64%)",
        }}
      />
      <LockupFull left={left} top={top} scale={scale} style={{ transform: `scale(${settle})`, transformOrigin: "960px 340px" }} />
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
      <div style={{ position: "absolute", left: 0, right: 0, bottom: 52, textAlign: "center", opacity: c }}>
        <Label size={21}>Open source · Apache-2.0 · Omarchy x86_64 · Release 2.0.0</Label>
      </div>
    </AbsoluteFill>
  );
};

const len = (s: { from: number; to: number }) => s.to - s.from;

export const MluvaRustHighlight: React.FC = () => {
  const frame = useCurrentFrame();
  const glowX = interpolate(frame, [0, HIGHLIGHT_DURATION], [0.3, 0.7], { extrapolateRight: "clamp" });
  return (
    <AbsoluteFill style={{ backgroundColor: "#000" }}>
      <BlurDefs />
      <Background glowX={glowX} glowY={0.5} />
      <Sequence from={SCENES.title.from} durationInFrames={len(SCENES.title)} layout="none">
        <Title length={len(SCENES.title)} />
      </Sequence>
      <Sequence from={SCENES.startup.from} durationInFrames={len(SCENES.startup)} layout="none">
        <Stat
          index="01 / First window"
          multiplier={ratio(STARTUP)}
          suffix="sooner"
          headline="456 → 260 ms to the first window (−43%)"
          python={{ label: "Python 1.6.0", value: "456 ms", amount: STARTUP.python }}
          rust={{ label: "Rust 2.0.0", value: "260 ms", amount: STARTUP.rust }}
          note="Python 1.6.0 vs published Rust 2.0.0 · same host, warm cache · medians of six starts each · startup only, not an inference-speed claim"
          extra="Rust’s slowest start (298 ms) beat Python’s fastest (402 ms)."
          start={SCENES.startup.from}
          length={len(SCENES.startup)}
        />
      </Sequence>
      <Sequence from={SCENES.ready.from} durationInFrames={len(SCENES.ready)} layout="none">
        <Stat
          index="02 / Ready on the bus"
          multiplier={ratio(READY)}
          suffix="sooner"
          headline="217 → 42 ms until the app answers D-Bus"
          python={{ label: "Python 1.6.0", value: "217 ms", amount: READY.python }}
          rust={{ label: "Rust 2.0.0", value: "42 ms", amount: READY.rust }}
          note="Service name ownership, before the window is visible; not recording readiness · same runs, medians of six starts each"
          start={SCENES.ready.from}
          length={len(SCENES.ready)}
        />
      </Sequence>
      <Sequence from={SCENES.size.from} durationInFrames={len(SCENES.size)} layout="none">
        <Stat
          index="03 / On disk"
          multiplier={ratio(SIZE)}
          suffix="smaller"
          headline="About 161 → 49 MiB installed"
          python={{ label: "Python 1.6.0", value: "~161 MiB", amount: SIZE.python }}
          rust={{ label: "Rust 2.0.0", value: "49 MiB", amount: SIZE.rust }}
          note="1.6.0: locked Python environment (151 MiB, no dev tools) plus app files (10 MiB) · 2.0.0: the unpacked published bundle · models and caches excluded"
          start={SCENES.size.from}
          length={len(SCENES.size)}
        />
      </Sequence>
      <Sequence from={SCENES.end.from} durationInFrames={len(SCENES.end)} layout="none">
        <End length={len(SCENES.end)} />
      </Sequence>
      <Grain />
      <Audio src={staticFile(score.file)} />
    </AbsoluteFill>
  );
};
