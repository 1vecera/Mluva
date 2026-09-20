import fs from "node:fs";
import { execFileSync } from "node:child_process";

const transcript = JSON.parse(
  fs.readFileSync("reference/narration-scribe.json", "utf8"),
);
const words = transcript.words.filter((w) => w.type === "word");
const ranges = [
  [0, 1],
  [2, 10],
  [11, 13],
  [14, 19],
  [20, 23],
  [24, 27],
  [28, 34],
  [35, 39],
  [40, 43],
  [44, 51],
  [52, 55],
  [56, 61],
  [62, 65],
  [66, 71],
  [72, 75],
  [76, 81],
  [82, 82],
];
const voiceStart = 1;
const phrases = [
  "Meet Mluva.",
  "A home for the things you think out loud.",
  "Press F9.",
  "Say it in your own words.",
  "Your original stays editable.",
  "Polish a rough thought",
  "or turn it into a structured note.",
  "Need to think something through?",
  "Try experimental Live rewrite.",
  "It can build a draft as you talk",
  "and surface unanswered questions.",
  "Come back to a saved conversation.",
  "Find it. Keep working.",
  "Choose your speech and rewrite providers.",
  "Mluva follows your theme.",
  "Talk it through. Make it useful.",
  "Mluva.",
];
const script = fs
  .readFileSync("script/narration-intro.txt", "utf8")
  .replace(/<\|phoneme_start\|>.*?<\|phoneme_end\|>/g, "Mluva")
  .replace(/\[[^\]]+\]/g, "");
const normalized = (text) =>
  text
    .toLowerCase()
    .replace(/f9/g, "f nine")
    .replace(/[^a-z0-9\s]/g, "")
    .trim()
    .replace(/\s+/g, " ");
if (normalized(script) !== normalized(words.map((w) => w.text).join(" ")))
  throw new Error(
    "Narration and Scribe words differ. Review the take before retiming.",
  );
if (normalized(script) !== normalized(phrases.join(" ")))
  throw new Error("Caption copy differs from the script.");
if (words.length !== 83)
  throw new Error("Update reviewed caption ranges for the new take.");
const voiceDuration = Number(
  execFileSync(
    "ffprobe",
    [
      "-v",
      "error",
      "-show_entries",
      "format=duration",
      "-of",
      "csv=p=0",
      "public/audio/intro-voice.wav",
    ],
    { encoding: "utf8" },
  ).trim(),
);
const captions = ranges.map(([first, last], index) => ({
  start: words[first].start + voiceStart,
  end: Math.min(
    (words[last + 1]?.start ?? 43) + voiceStart - 0.07,
    words[last].end + voiceStart + 0.28,
  ),
  text: phrases[index],
}));
const cue = (index) => words[index].start + voiceStart;
const shots = [
  { id: "intro", start: 0, label: "Think out loud." },
  { id: "record", start: cue(11) - 2.8, label: "Talk naturally." },
  { id: "edit", start: cue(20) - 0.2, label: "Keep your meaning." },
  { id: "live", start: cue(35) - 0.2, label: "Find the missing pieces." },
  { id: "history", start: cue(56) - 0.2, label: "Find it again." },
  { id: "models", start: cue(66) - 0.2, label: "Your tools. Your theme." },
  {
    id: "outro",
    start: cue(76) - 0.2,
    label: "Talk it through. Make it useful.",
  },
];
const duration = Math.ceil((voiceStart + voiceDuration + 2.8) * 30) / 30;
shots.forEach((shot, i) => (shot.end = shots[i + 1]?.start ?? duration));
const actions = {
  key: cue(12),
  polish: cue(24),
  structure: cue(28),
  live: cue(40),
  draft: cue(44) - 0.2,
  search: cue(62),
  providers: cue(66),
  theme: cue(72),
};
const manifest = JSON.parse(fs.readFileSync("reference/captures.json", "utf8"));
if (manifest.errors.length)
  throw new Error("The native capture was incomplete.");
const captures = manifest.captures;
const clock = JSON.parse(fs.readFileSync("reference/clock.json", "utf8"));
fs.mkdirSync("src/generated", { recursive: true });
fs.writeFileSync(
  "src/generated/intro.json",
  JSON.stringify(
    {
      voiceStart,
      voiceDuration,
      duration,
      captions,
      shots,
      actions,
      clock,
      width: captures[0].logical_width ?? captures[0].width,
      height: captures[0].logical_height ?? captures[0].height,
    },
    null,
    2,
  ) + "\n",
);
