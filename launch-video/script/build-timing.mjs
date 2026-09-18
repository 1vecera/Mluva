#!/usr/bin/env node
// Build the film timing from the narration transcript.
//
// Input : script/narration.txt        one narration line per paragraph (11 lines)
//         script/narration-scribe.json ElevenLabs Scribe v2 word timestamps of the WAV
// Output: src/generated/timing.json    per-line source cut points, timeline placement,
//                                      scene bounds and caption words
//
// The edit is text: change a hold below, re-run `node script/build-timing.mjs`,
// re-render. Nothing is scrubbed by hand.

import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "..");
const args = Object.fromEntries(
  process.argv.slice(2).map((a) => {
    const [k, v] = a.replace(/^--/, "").split("=");
    return [k, v ?? "true"];
  }),
);

const textFile = resolve(root, args.text ?? "script/narration.txt");
const scribeFile = resolve(root, args.scribe ?? "script/narration-scribe.json");
const outFile = resolve(root, "src/generated/timing.json");

// Beat plan, seconds. leadIn: how long the scene is on screen before its line starts.
// holdAfter: how long the scene stays after its line ends (room for the demo to finish).
const BEATS = [
  { id: "S01", leadIn: 1.0, holdAfter: 0.3 },
  { id: "S02", leadIn: 0.4, holdAfter: 0.7 },
  { id: "S03", leadIn: 0.3, holdAfter: 2.2 },
  { id: "S04", leadIn: 0.3, holdAfter: 0.7 },
  { id: "S05", leadIn: 0.3, holdAfter: 0.9 },
  { id: "S06", leadIn: 0.3, holdAfter: 1.0 },
  { id: "S07", leadIn: 0.3, holdAfter: 0.6 },
  { id: "S08", leadIn: 0.3, holdAfter: 0.6 },
  { id: "S09", leadIn: 0.3, holdAfter: 0.7 },
  { id: "S10", leadIn: 0.3, holdAfter: 0.9 },
  { id: "S11", leadIn: 0.4, holdAfter: 3.0 },
];
const MAX_TOTAL = 60;
const PAD = 0.1; // seconds of silence kept around each spoken line when cutting the WAV

const normalize = (s) => s.toLowerCase().replace(/[^a-z0-9]+/g, "");
const lines = readFileSync(textFile, "utf8")
  .split(/\n\s*\n/)
  .map((l) => l.trim())
  .filter(Boolean);
if (lines.length !== BEATS.length) {
  throw new Error(`Expected ${BEATS.length} narration lines, found ${lines.length}`);
}

const scribe = JSON.parse(readFileSync(scribeFile, "utf8"));
const words = scribe.words.filter((w) => w.type === "word");

// Walk the transcript words along the script words. Scribe may mis-hear a proper noun,
// so alignment is by count: each script line consumes as many transcript words as it has.
let cursor = 0;
const aligned = lines.map((line, i) => {
  const scriptWords = line.split(/\s+/).filter(Boolean);
  const slice = words.slice(cursor, cursor + scriptWords.length);
  if (slice.length !== scriptWords.length) {
    throw new Error(`Line ${i + 1} ran out of transcript words`);
  }
  // Report mismatches so a mispronunciation is visible in the log.
  const mismatches = scriptWords
    .map((w, j) => [w, slice[j].text])
    .filter(([a, b]) => normalize(a) !== normalize(b) && !normalize(a).startsWith(normalize(b)) && !normalize(b).startsWith(normalize(a)));
  cursor += scriptWords.length;
  // Captions show the script's spelling; only the timing comes from the transcript.
  const lineWords = slice.map((w, j) => ({ ...w, text: scriptWords[j] }));
  return { text: line, words: lineWords, mismatches };
});

// Cut points: half-way into the silence gap on both sides, at most PAD seconds.
const cuts = aligned.map((l, i) => {
  const first = l.words[0].start;
  const last = l.words[l.words.length - 1].end;
  const prevEnd = i > 0 ? aligned[i - 1].words.at(-1).end : 0;
  const nextStart = i < aligned.length - 1 ? aligned[i + 1].words[0].start : last + 1;
  const srcStart = Math.max(prevEnd, first - Math.min(PAD, (first - prevEnd) / 2));
  const srcEnd = Math.min(nextStart, last + Math.min(PAD, (nextStart - last) / 2));
  return { srcStart: +srcStart.toFixed(3), srcEnd: +srcEnd.toFixed(3) };
});

// Lay the lines on the timeline with the beat plan.
let t = 0;
const scenes = [];
BEATS.forEach((beat, i) => {
  const sceneStart = t;
  const lineStart = sceneStart + beat.leadIn;
  const lineDuration = cuts[i].srcEnd - cuts[i].srcStart;
  const lineEnd = lineStart + lineDuration;
  t = lineEnd + beat.holdAfter;
  scenes.push({
    id: beat.id,
    text: aligned[i].text,
    sceneStart: +sceneStart.toFixed(3),
    lineStart: +lineStart.toFixed(3),
    lineEnd: +lineEnd.toFixed(3),
    sceneEnd: null, // filled below
    srcStart: cuts[i].srcStart,
    srcEnd: cuts[i].srcEnd,
    words: aligned[i].words.map((w) => ({
      text: w.text,
      // absolute timeline seconds
      start: +(lineStart + (w.start - cuts[i].srcStart)).toFixed(3),
      end: +(lineStart + (w.end - cuts[i].srcStart)).toFixed(3),
    })),
  });
});
scenes.forEach((s, i) => {
  s.sceneEnd = i < scenes.length - 1 ? scenes[i + 1].sceneStart : +t.toFixed(3);
});
const total = +t.toFixed(3);
if (total > MAX_TOTAL) {
  throw new Error(`Film is ${total}s, over the ${MAX_TOTAL}s limit. Trim holds in BEATS.`);
}

mkdirSync(dirname(outFile), { recursive: true });
writeFileSync(
  outFile,
  JSON.stringify(
    {
      generatedFrom: { text: textFile.replace(root + "/", ""), scribe: scribeFile.replace(root + "/", "") },
      narrationFile: args.audio ?? "audio/narration.wav",
      totalSeconds: total,
      scenes,
    },
    null,
    2,
  ) + "\n",
);

console.log(`total ${total}s across ${scenes.length} scenes -> ${outFile.replace(root + "/", "")}`);
for (const s of scenes) {
  console.log(
    `${s.id} ${s.sceneStart.toFixed(2).padStart(6)}-${s.sceneEnd.toFixed(2).padStart(6)}  line ${s.lineStart.toFixed(2)}-${s.lineEnd.toFixed(2)}  src ${s.srcStart}-${s.srcEnd}`,
  );
}
const mism = aligned.flatMap((l, i) => l.mismatches.map(([a, b]) => `line ${i + 1}: script "${a}" heard "${b}"`));
if (mism.length) console.log("heard differently:\n  " + mism.join("\n  "));
