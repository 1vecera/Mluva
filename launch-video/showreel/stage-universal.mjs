// Stage a recorded wheel take for the universal showreel and prove the staged frames are the capture.
//
// usage: node showreel/stage-universal.mjs <take-dir>   (run from launch-video/)
//
// <take-dir> holds `frames/*.png`, `wheel-take.mkv` and `wheel-take.json` from
// showreel/capture/showreel_wheel_take.rs. The PNGs are copied to public/universal/wheel/; every
// staged frame's decoded RGB is MD5-compared with the same frame decoded from the lossless take,
// and src/universal/wheel-take.json receives the timeline and the fixtures' captions.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

const takeDir = resolve(process.argv[2] ?? "");
const take = JSON.parse(readFileSync(join(takeDir, "wheel-take.json"), "utf8"));
const out = "public/universal/wheel";
rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
const frames = readdirSync(join(takeDir, "frames")).filter((f) => f.endsWith(".png")).sort();
for (const f of frames) copyFileSync(join(takeDir, "frames", f), join(out, f));

const md5s = (args) =>
  execFileSync("ffmpeg", ["-v", "error", ...args, "-f", "framemd5", "-c:v", "rawvideo", "-pix_fmt", "rgb24", "-"], {
    maxBuffer: 1 << 28,
  })
    .toString()
    .split("\n")
    .filter((l) => l && !l.startsWith("#"))
    .map((l) => l.split(",").pop().trim());
const staged = md5s(["-framerate", "60", "-i", join(out, "%05d.png")]);
const captured = md5s(["-i", join(takeDir, "wheel-take.mkv")]);
const mismatches = staged.filter((h, i) => h !== captured[i]).length;
if (staged.length !== captured.length || mismatches) {
  throw new Error(`staged ${staged.length} frames, capture ${captured.length}, ${mismatches} differ`);
}

// What the reel's margins say for each fixture: the level its route delivers, as the target model
// names it, and where the session runs. Codex sends only to an idle thread.
const caption = (t) => {
  if (t.delivery === "Send" && t.route === "codex_queue") return ["SEND · IF IDLE", "STARTED BY THAT THREAD · ELSE KEPT"];
  if (t.delivery === "Send") return ["SEND", "SUBMITTED TO THAT SESSION · CONFIRMED"];
  if (t.delivery === "Queue") return ["QUEUE", "TAKEN AT ITS NEXT TURN"];
  if (t.delivery === "Prefill") return ["PREFILL", "IN A NEW CHAT · YOU PRESS SEND"];
  return ["DRAFT ONLY", "KEPT AS AN EDITABLE DRAFT IN MLUVA"];
};
const place = (t) => {
  if (t.place.startsWith("Herdr")) return "in Herdr";
  if (t.place.startsWith("tmux")) return "in tmux";
  if (t.place === "terminal") return "in a terminal";
  if (t.place === "background") return "in the background";
  if (t.route === "claude_new_chat") return "a new chat";
  return t.kind.endsWith("Desktop") && t.route === "claude_inbox" ? "a Code session" : "desktop app";
};
const scale = take.scale;
const record = {
  screen: take.screen,
  scale,
  frames: frames.length,
  timeline: take.timeline,
  // The dial and every name, in capture pixels (the content spans x 698–2907, y 227–1795 of the
  // 3840×2160 take); the panel shows this crop at half size, a clean 2:1 downsample.
  crop: { x: 310 * scale, y: 100 * scale, w: 1180 * scale, h: 815 * scale },
  panel_scale: 1 / scale,
  fixtures: take.fixtures.map((t) => {
    const [level, line] = caption(t);
    return { name: t.name, kind: t.kind, place: place(t), route: t.route, caption: level, line };
  }),
  md5: { frames: staged.length, matching: staged.length - mismatches },
};
writeFileSync("src/universal/wheel-take.json", JSON.stringify(record, null, 2) + "\n");
console.log(`staged ${frames.length} wheel frames; RGB MD5 match ${staged.length}/${captured.length}`);
