// Stage a recorded wheel take for the universal showreel and prove the staged frames are the capture.
//
// usage: node showreel/stage-universal.mjs <take-dir>             the wheel take
//        node showreel/stage-universal.mjs delivery <take-dir>    the delivery take
// (run from launch-video/)
//
// <take-dir> holds `frames/*.png`, `wheel-take.mkv` and `wheel-take.json` from
// showreel/capture/showreel_wheel_take.rs. The PNGs are copied to public/universal/wheel/; every
// staged frame's decoded RGB is MD5-compared with the same frame decoded from the lossless take,
// and src/universal/wheel-take.json receives the timeline and the fixtures' captions.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

// Per-frame RGB MD5s; each row is [stream, dts, pts, duration, size, hash] and `tb` the time base.
const md5s = (args) => {
  const text = execFileSync("ffmpeg", ["-v", "error", ...args, "-f", "framemd5", "-c:v", "rawvideo", "-pix_fmt", "rgb24", "-"], {
    maxBuffer: 1 << 28,
  }).toString();
  const [num, den] = (text.match(/^#tb 0: (\d+)\/(\d+)/m) ?? [0, 1, 60]).slice(1).map(Number);
  const rows = text
    .split("\n")
    .filter((l) => l && !l.startsWith("#"))
    .map((l) => l.split(",").map((v) => v.trim()));
  rows.tb = num / den;
  return rows;
};

if (process.argv[2] === "delivery") {
  // The delivery take is a real-time x11grab recording: frames carry their own capture times,
  // so the composition picks frames by time. Only the stretch the reel uses is staged.
  const dir = resolve(process.argv[3] ?? "");
  const mkv = join(dir, "delivery-take.mkv");
  const [from, to] = [3.5, 26.0];
  const out = "public/universal/delivery";
  rmSync(out, { recursive: true, force: true });
  mkdirSync(out, { recursive: true });
  const rows = md5s(["-i", mkv, "-fps_mode", "passthrough"]);
  const tb = rows.tb;
  const all = rows.map(([, , pts, , , hash], i) => ({ i, t: Number(pts) * tb, hash }));
  const t0 = all[0].t;
  const used = all.filter((f) => f.t - t0 >= from && f.t - t0 <= to);
  execFileSync("ffmpeg", ["-v", "error", "-i", mkv, "-fps_mode", "passthrough", "-vf", `select='between(n\,${used[0].i}\,${used.at(-1).i})'`, "-start_number", "0", join(out, "%05d.png")]);
  const staged = md5s(["-framerate", "60", "-i", join(out, "%05d.png")]).map((r) => r.at(-1));
  const mismatches = used.filter((f, k) => staged[k] !== f.hash).length;
  if (staged.length !== used.length || mismatches) throw new Error(`delivery: staged ${staged.length}, wanted ${used.length}, ${mismatches} differ`);
  const timeline = readFileSync(join(dir, "timeline.jsonl"), "utf8").trim().split("\n").map((l) => JSON.parse(l));
  const agents = JSON.parse(readFileSync(join(dir, "agents-list.json"), "utf8")).recipients.map((r) => ({ title: r.title, kind: r.kind, route: r.route, delivery: r.delivery }));
  const probe = execFileSync("ffprobe", ["-v", "error", "-show_entries", "stream=width,height", "-of", "csv=p=0", mkv]).toString().trim().split(",").map(Number);
  const record = { screen: probe, times: used.map((f) => Math.round((f.t - t0) * 1000) / 1000), timeline, agents, md5: { frames: staged.length, matching: staged.length - mismatches } };
  // One key per line keeps the 1300 frame times out of the diff's way.
  const body = Object.entries(record).map(([k, v]) => ` ${JSON.stringify(k)}: ${JSON.stringify(v)}`).join(",\n");
  writeFileSync("src/universal/delivery-take.json", `{\n${body}\n}\n`);
  console.log(`staged ${used.length} delivery frames (${from}–${to} s); RGB MD5 match ${staged.length - mismatches}/${used.length}`);
  process.exit(0);
}

const takeDir = resolve(process.argv[2] ?? "");
const take = JSON.parse(readFileSync(join(takeDir, "wheel-take.json"), "utf8"));
const out = "public/universal/wheel";
rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
const frames = readdirSync(join(takeDir, "frames")).filter((f) => f.endsWith(".png")).sort();
for (const f of frames) copyFileSync(join(takeDir, "frames", f), join(out, f));

const staged = md5s(["-framerate", "60", "-i", join(out, "%05d.png")]).map((r) => r.at(-1));
const captured = md5s(["-i", join(takeDir, "wheel-take.mkv")]).map((r) => r.at(-1));
const mismatches = staged.filter((h, i) => h !== captured[i]).length;
if (staged.length !== captured.length || mismatches) {
  throw new Error(`staged ${staged.length} frames, capture ${captured.length}, ${mismatches} differ`);
}

// What the reel's margins say for each fixture: the level its route delivers, as the target model
// names it, and where the session runs. Codex sends only to an idle thread.
const caption = (t) => {
  if (t.delivery === "Send" && t.route === "codex_queue") return ["SEND · IF IDLE", "ITS NEXT TURN · BUSY: DRAFT KEPT"];
  if (t.delivery === "Send") return ["SEND", "SUBMITTED · WITH HERDR'S RECEIPT"];
  if (t.delivery === "Queue") return ["QUEUE", "TAKEN WHEN THE AGENT IS READY"];
  if (t.delivery === "Prefill") return ["PREFILL", "A NEW CHAT · YOU PRESS SEND"];
  return ["DRAFT ONLY", "AN EDITABLE DRAFT IN MLUVA"];
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
