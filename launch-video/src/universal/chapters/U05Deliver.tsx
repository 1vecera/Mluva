import { useCurrentFrame } from "remotion";
import { beatFrame } from "../timing";
import { Delivery, Shot } from "../parts/Delivery";

const B0 = 32;
const LENGTH = beatFrame(44) - beatFrame(B0);

// The drop: F12 already chose the Codex thread "Release notes" on the wheel. Take times are the
// delivery take's own (src/universal/delivery-take.json): recording view at 4.27 s, transcript at
// 7.78 s, countdown to 9.58 s, "Sent" at 10.03 s and the turn in the Codex pane at 10.10 s.
// Recording and countdown run faster than real time (3× and 1.4×); from the send on, real speed.
const SHOTS: Shot[] = [
  { beat: 0, from: 4.45, rate: 3, framing: "wide", label: "F12 · RECORDING" },
  { beat: 1, from: 5.85, rate: 3, framing: "mluva", move: 12, label: "F12 · RECORDING" },
  { beat: 2, from: 7.7, rate: 1, framing: "mluva", push: 0.05, label: "TRANSCRIBED" },
  { beat: 3, from: 8.17, rate: 1.4, framing: "status", move: 12, label: "COUNTDOWN · SENDING TO RELEASE NOTES" },
  { beat: 5, from: 9.48, rate: 1, framing: "status", push: 0.05, label: "SENT · THE THREAD TOOK IT AS ITS NEXT TURN" },
  { beat: 6.5, from: 10.18, rate: 1, framing: "codexClose", move: 16, push: 0.08, label: "CODEX · THE DICTATION, THEN THE MODEL'S REPLY" },
  { beat: 10.5, from: 12.05, rate: 1, framing: "wide", move: 14, label: "MLUVA AND CODEX · ONE PRIVATE SCREEN" },
];

export const U05Deliver: React.FC = () => {
  const frame = useCurrentFrame();
  return <Delivery frame={frame} b0={B0} shots={SHOTS} length={LENGTH} note="REAL APP · SCRIPTED DEMO · SPED UP · LOOPBACK MODELS" />;
};
