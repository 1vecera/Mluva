import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import vm from "node:vm";

const code = await readFile(new URL("../web/pcm-worklet.js", import.meta.url), "utf8");
for (const rate of [16000, 44100, 48000, 96000]) {
  let Processor;
  const packets = [];
  vm.runInNewContext(code, {
    sampleRate: rate,
    AudioWorkletProcessor: class { constructor() { this.port = { postMessage: (packet) => packets.push(packet) }; } },
    registerProcessor: (_, value) => { Processor = value; },
  });
  const processor = new Processor();
  // A real iPhone/Chromium hardware rate need not be 16 kHz. Vary render block
  // boundaries; the result must still contain one exact second of LE PCM.
  let remaining = rate;
  while (remaining) {
    const count = Math.min(remaining, 128);
    processor.process([[new Float32Array(count).fill(0.5)]]);
    remaining -= count;
  }
  processor.port.onmessage({ data: "flush" });
  const buffers = packets.filter((p) => p.pcm).map((p) => Buffer.from(p.pcm));
  const pcm = Buffer.concat(buffers);
  assert.equal(pcm.length, 32000, `rate ${rate}`);
  assert.equal(pcm.readInt16LE(0), 16384);
  assert.equal(pcm.readInt16LE(pcm.length - 2), 16384);
  assert(buffers.every((b) => b.length <= 16000));
  assert.equal(packets.at(-1).flushed, true);
  processor.process([[new Float32Array(128).fill(1)]]);
  assert.equal(packets.filter((p) => p.pcm).length, buffers.length, "flush stops capture");
}
console.log("PASS: actual worklet resamples 16/44.1/48/96 kHz to mono PCM16 LE and drains once at Stop.");
