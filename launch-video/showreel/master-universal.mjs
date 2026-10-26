// Mix and master the universal showreel's sound onto its silent render.
//
// usage: node showreel/master-universal.mjs   (run from launch-video/)
//
// Needs out/mluva-showreel-universal-silent.mp4, public/universal/audio/score.wav and the effects in
// public/showreel/audio/. Every effect from src/universal/cues.json is placed sample-accurately so
// its peak lands on its frame, mixed with the score, faded over the last frames, mastered to
// −14 LUFS with a −1.5 dBTP ceiling (a gentle peak limiter, then FFmpeg's two-pass loudnorm as one linear gain; its LRA target sits above the score's own range) and muxed as 320 kbit/s
// AAC. The video stream is copied untouched, so every cut stays frame-aligned.
import { execFileSync, spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";

const cues = JSON.parse(readFileSync("src/universal/cues.json", "utf8"));
const RATE = 48000;
const silent = "out/mluva-showreel-universal-silent.mp4";
const output = "out/mluva-showreel-universal.mp4";
const duration = cues.duration_frames / cues.fps;
const fadeFrom = cues.score.fade_out_from_frame / cues.fps;

const inputs = ["-i", `public/universal/audio/${cues.score.file}`];
const chains = [`[0:a]aresample=${RATE},aformat=channel_layouts=stereo,atrim=0:${duration},volume=${cues.score.gain_db}dB[s]`];
cues.cues.forEach((cue, i) => {
  inputs.push("-i", `public/showreel/audio/${cue.sfx}.mp3`);
  const start = cue.frame / cues.fps - cues.sfx_peaks_s[cue.sfx];
  const shift = start >= 0 ? `adelay=delays=${Math.round(start * RATE)}S:all=1` : `atrim=start=${-start},asetpts=PTS-STARTPTS`;
  chains.push(`[${i + 1}:a]aresample=${RATE},aformat=channel_layouts=stereo,${shift},volume=${cue.gain_db}dB[c${i}]`);
});
const mixed = `${cues.cues.map((_, i) => `[c${i}]`).join("")}[s]amix=inputs=${cues.cues.length + 1}:normalize=0:dropout_transition=0,atrim=0:${duration},afade=t=out:st=${fadeFrom}:d=${duration - fadeFrom}`;

// The score arrives mastered to 0 dBFS; a gentle sample-peak limiter keeps its own hits and the
// effects stacked on them under the ceiling, so loudnorm can then apply one linear gain.
const LIMIT = "alimiter=limit=0.79:attack=2:release=40:level=disabled";
const graph = (tail) => [...chains, `${mixed},${LIMIT},${tail}`].join(";");
// FFmpeg reports loudness on stderr.
const stderr = (args) => {
  const run = spawnSync("ffmpeg", args, { encoding: "utf8", maxBuffer: 1 << 26 });
  if (run.status !== 0) throw new Error(run.stderr.slice(-2000));
  return run.stderr;
};
const measure = stderr(["-hide_banner", "-nostdin", ...inputs, "-filter_complex", graph("loudnorm=I=-14:TP=-1.5:LRA=20:print_format=json"), "-f", "null", "-"]);
const m = JSON.parse(measure.slice(measure.lastIndexOf("{"), measure.lastIndexOf("}") + 1));
const norm = `loudnorm=I=-14:TP=-1.5:LRA=20:measured_I=${m.input_i}:measured_TP=${m.input_tp}:measured_LRA=${m.input_lra}:measured_thresh=${m.input_thresh}:offset=${m.target_offset}:linear=true`;
// A small trim keeps the true peak under the ceiling after AAC encoding.
const master = (trim) => {
  execFileSync("ffmpeg", [
    "-hide_banner", "-nostdin", "-y", "-v", "error",
    "-i", silent, ...inputs,
    "-filter_complex", graph(`${norm},volume=${trim}dB,aresample=${RATE}[a]`).replaceAll(/\[(\d+):a\]/g, (_, n) => `[${Number(n) + 1}:a]`),
    "-map", "0:v", "-map", "[a]", "-c:v", "copy", "-c:a", "aac", "-b:a", "320k", "-ar", String(RATE), "-movflags", "+faststart", output,
  ]); // prettier-ignore
  const check = stderr(["-hide_banner", "-nostdin", "-i", output, "-map", "0:a", "-af", "ebur128=peak=true", "-f", "null", "-"]);
  return { lufs: Number(check.match(/I:\s+(-?[\d.]+) LUFS/g).pop().match(/-?[\d.]+/)[0]), peak: Number(check.match(/Peak:\s+(-?[\d.]+) dBFS/g).pop().match(/-?[\d.]+/)[0]) };
};
// Confirm the second pass stayed linear (one gain for the whole mix, no dynamic compression).
const second = stderr(["-hide_banner", "-nostdin", ...inputs, "-filter_complex", graph(norm.replace(":linear=true", ":linear=true:print_format=json")), "-f", "null", "-"]);
const normalization = JSON.parse(second.slice(second.lastIndexOf("{"), second.lastIndexOf("}") + 1)).normalization_type;
// One measured gain correction lands the encoded file on −14 LUFS; then any peak trim.
let result = master(0);
let trim = Math.round((-14 - result.lufs) * 10) / 10;
if (trim !== 0) result = master(trim);
while (result.peak > -1.5 && trim > -1.5) {
  trim -= 0.2;
  result = master(trim);
}
console.log(JSON.stringify({ output, integrated_lufs: result.lufs, true_peak_dbtp: result.peak, trim_db: Number(trim.toFixed(1)), normalization, first_pass: m }, null, 1));
