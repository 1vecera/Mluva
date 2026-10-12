// Opt-in synthetic provider experiment. No native microphone, account data or AWS writes.
// AudioContext/Worklet are deterministic PCM adapters; this is transport evidence,
// not physical-browser/network acceptance. Run only with scoped provider authorization.
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { WebSocket } from "ws";
import { CloudSpeech } from "../web/speech.mjs";
if (!process.argv.includes("--run-provider-measurement"))
  throw new Error("Explicit --run-provider-measurement is required.");
const pcm = await readFile(process.argv[2]);
const destination = process.argv[3];
const count = Number(process.argv[4] ?? 30);
if (!Number.isInteger(count) || count < 1 || count > 30 || pcm.length > 960000)
  throw new Error("Bound experiment to 30 pairs and 30 seconds per stream.");
const key =
  process.env.ELEVEN_LABS_STT_TOKEN ??
  process.env.DAS_ITEM_ELEVENLABS__ELEVEN_LABS_STT_TOKEN;
if (!key) throw new Error("Managed speech credential unavailable.");
globalThis.WebSocket = WebSocket;
globalThis.AudioContext = class {
  state = "running";
  audioWorklet = { addModule: async () => {} };
  resume() {
    return Promise.resolve();
  }
  createMediaStreamSource() {
    return { connect() {}, disconnect() {} };
  }
  close() {
    this.state = "closed";
    return Promise.resolve();
  }
};
globalThis.AudioWorkletNode = class {
  constructor() {
    this.port = {
      postMessage: () => this.port.onmessage({ data: { flushed: true } }),
    };
  }
  connect() {}
  disconnect() {}
};
globalThis.MediaRecorder = class extends EventTarget {
  static isTypeSupported() {
    return true;
  }
  state = "inactive";
  start() {
    this.state = "recording";
  }
  stop() {
    this.state = "inactive";
    this.dispatchEvent(new Event("stop"));
  }
};
Object.defineProperty(globalThis, "navigator", {
  value: {
    mediaDevices: {
      getUserMedia: async () => ({
        getTracks: () => [{ stop() {}, addEventListener() {} }],
      }),
    },
  },
});
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
const timestamp = () => performance.now();
async function token() {
  const start = timestamp();
  const response = await fetch(
    "https://api.elevenlabs.io/v1/single-use-token/realtime_scribe",
    {
      method: "POST",
      headers: { "xi-api-key": key },
      signal: AbortSignal.timeout(10000),
    },
  );
  if (!response.ok) throw new Error(`Token HTTP ${response.status}`);
  return { value: (await response.json()).token, ms: timestamp() - start };
}
async function pair(index) {
  const [a, b] = await Promise.all([token(), token()]);
  const observations = {
    baseline: { partial: [], commit: [], text: [] },
    app: { partial: [], commit: [], text: [] },
  };
  let epoch, failure;
  const report = (name, phase, text) => {
    observations[name][phase].push(timestamp() - (epoch ?? timestamp()));
    if (phase === "commit") observations[name].text.push(text);
  };
  const speech = new CloudSpeech({
    token: Promise.resolve(b.value),
    onPartial: (t) => report("app", "partial", t),
    onSegment: (t) => report("app", "commit", t),
    onFailure: (e) => {
      failure = e;
    },
  });
  const appStart = timestamp();
  const appReady = speech.start().then(() => timestamp() - appStart);
  const url = new URL("wss://api.elevenlabs.io/v1/speech-to-text/realtime");
  url.search = new URLSearchParams({
    model_id: "scribe_v2_realtime",
    token: a.value,
    audio_format: "pcm_16000",
    commit_strategy: "manual",
  });
  const direct = new WebSocket(url);
  const directStart = timestamp();
  const directReady = new Promise((resolve, reject) => {
    const timeout = setTimeout(
      () => reject(new Error("Baseline start timeout")),
      12000,
    );
    direct.on("error", () => reject(new Error("Baseline connection failed")));
    direct.on("message", (data) => {
      const event = JSON.parse(data);
      if (event.message_type === "session_started") {
        clearTimeout(timeout);
        resolve(timestamp() - directStart);
      } else if (event.message_type === "partial_transcript")
        report("baseline", "partial", event.text);
      else if (event.message_type === "committed_transcript")
        report("baseline", "commit", event.text);
      else {
        failure = new Error(`Provider ${event.message_type}`);
      }
    });
  });
  try {
    const [directStartup, appStartup] = await Promise.all([
      directReady,
      appReady,
    ]);
    epoch = timestamp();
    let sinceCommit = 0;
    for (let offset = 0; offset < pcm.length; offset += 1600) {
      await wait(Math.max(0, epoch + offset / 32 - timestamp()));
      const bytes = pcm.subarray(offset, Math.min(offset + 1600, pcm.length));
      sinceCommit += bytes.length;
      const commit = sinceCommit >= 160000;
      if (commit) sinceCommit = 0;
      direct.send(
        JSON.stringify({
          message_type: "input_audio_chunk",
          audio_base_64: bytes.toString("base64"),
          sample_rate: 16000,
          commit,
        }),
      );
      speech.node.port.onmessage({
        data: {
          pcm: bytes.buffer.slice(
            bytes.byteOffset,
            bytes.byteOffset + bytes.byteLength,
          ),
        },
      });
      if (failure) throw new Error("Provider interrupted");
    }
    direct.send(
      JSON.stringify({
        message_type: "input_audio_chunk",
        audio_base_64: "",
        sample_rate: 16000,
        commit: true,
      }),
    );
    await speech.finish();
    const deadline = timestamp() + 8000;
    while (
      observations.baseline.commit.length < observations.app.commit.length &&
      timestamp() < deadline
    )
      await wait(25);
    const base = observations.baseline,
      app = observations.app;
    const norm = (t) =>
      t
        .join(" ")
        .toLowerCase()
        .replace(/[^\p{L}\p{N}]+/gu, " ")
        .trim();
    return {
      index,
      tokenMs: { baseline: a.ms, app: b.ms },
      startupMs: { baseline: directStartup, app: appStartup },
      firstPartialMs: { baseline: base.partial[0], app: app.partial[0] },
      committedMs: { baseline: base.commit, app: app.commit },
      pairedExtraCommitMs: app.commit.map((v, i) => v - base.commit[i]),
      complete: Boolean(
        base.text.at(-1)?.includes("experiment") &&
        app.text.at(-1)?.includes("experiment"),
      ),
      sameWords: norm(base.text) === norm(app.text),
      commits: { baseline: base.commit.length, app: app.commit.length },
    };
  } finally {
    direct.close();
    await speech.finish(false).catch(() => {});
  }
}
const results = [];
await mkdir(destination, { recursive: true });
for (let i = 0; i < count; i++) {
  try {
    results.push(await pair(i + 1));
    process.stdout.write(`pair ${i + 1}/${count} completed\n`);
  } catch {
    results.push({
      index: i + 1,
      error: "Provider experiment failed; no token or provider payload logged.",
    });
    break;
  }
  await writeFile(
    `${destination}/samples.json`,
    JSON.stringify(results, null, 2),
  );
}
const percentile = (values, p) => {
  const sorted = values.filter(Number.isFinite).sort((a, b) => a - b);
  return sorted[Math.max(0, Math.ceil(sorted.length * p) - 1)];
};
const extras = results.flatMap((r) => r.pairedExtraCommitMs ?? []);
const summary = {
  date: new Date().toISOString(),
  environment:
    "Lenovo network, Node deterministic PCM adapter driving the unmodified production CloudSpeech transport and a simultaneous direct baseline",
  sessions: results.length,
  successful: results.filter((r) => !r.error).length,
  audioSeconds: pcm.length / 32000,
  p95ExtraCommitMs: percentile(extras, 0.95),
  p50ExtraCommitMs: percentile(extras, 0.5),
  p95FirstPartialExtraMs: percentile(
    results
      .filter((r) => !r.error)
      .map((r) => r.firstPartialMs.app - r.firstPartialMs.baseline),
    0.95,
  ),
  complete: results.every((r) => r.complete),
  sameWords: results.every((r) => r.sameWords),
  gate: "NOT ACCEPTED: one network and synthetic PCM adapters do not cover representative Czech phone/laptop browser capture, token Lambda startup or hosted fan-out; public speech remains disabled",
};
await writeFile(
  `${destination}/summary.json`,
  JSON.stringify(summary, null, 2),
);
console.log(JSON.stringify(summary));
