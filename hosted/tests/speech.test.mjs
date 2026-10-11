import { test } from "node:test";
import assert from "node:assert/strict";
import { CloudSpeech } from "../web/speech.mjs";

// Actual finalization owner with fake device boundaries, independent of the browser peer.
function finalizingSpeech() {
  const speech = Object.create(CloudSpeech.prototype),
    actions = [];
  Object.assign(speech, {
    closed: false,
    commitsSent: 1,
    commitsSeen: 0,
    pcmSinceCommit: 1600,
    chunks: [new Uint8Array([1, 2])],
    context: {
      state: "running",
      async close() {
        actions.push("context closed");
      },
    },
    stream: {
      getTracks: () => [
        {
          stop() {
            actions.push("microphone released");
          },
        },
      ],
    },
    source: {
      disconnect() {
        actions.push("source disconnected");
      },
    },
    node: {
      disconnect() {
        actions.push("worklet disconnected");
      },
      port: {
        postMessage(message) {
          assert.equal(message, "flush");
          queueMicrotask(() => speech.flushed());
        },
      },
    },
    socket: {
      readyState: WebSocket.OPEN,
      send() {},
      close() {
        actions.push("provider closed");
      },
    },
  });
  return { speech, actions };
}

test("an older delayed commit cannot settle Stop before the final audio response", async () => {
  const { speech, actions } = finalizingSpeech();
  let finalSent;
  const sent = new Promise((resolve) => {
    finalSent = resolve;
  });
  speech.socket.send = (encoded) => {
    assert.equal(JSON.parse(encoded).commit, true);
    finalSent();
  };
  let completed = false;
  const finishing = speech.finish().then(() => {
    completed = true;
  });
  await sent;
  speech.commitsSeen = 1;
  speech.committed();
  await Promise.resolve();
  await Promise.resolve();
  assert.equal(completed, false);
  assert.ok(!actions.includes("microphone released"));
  speech.commitsSeen = 2;
  speech.committed();
  await finishing;
  assert.ok(actions.includes("microphone released"));
  assert.ok(actions.includes("provider closed"));
  assert.ok(speech.audio.size > 0);
});

test("failed final audio flush still releases microphone and keeps the stopped backup", async () => {
  const { speech, actions } = finalizingSpeech();
  speech.node.port.postMessage = () => {
    throw new Error("Interrupted audio worklet");
  };
  await assert.rejects(speech.finish(), /Interrupted audio worklet/);
  assert.equal(speech.closed, true);
  assert.ok(actions.includes("microphone released"));
  assert.ok(actions.includes("context closed"));
  assert.ok(speech.audio.size > 0);
});
