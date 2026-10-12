# Synthetic provider timing · 12 October 2026

**The public speech gate is not accepted.** Thirty paired sessions from this Lenovo network measured **72.3 ms p95 extra committed-text delay** and **72.5 ms p95 extra first-partial delay**. This supports the direct provider transport choice but does not establish performance on representative Czech phone/laptop networks. Public speech remains disabled.

The experiment used one 14.071-second synthetic English fixture, generated with Fish Audio’s free default voice, converted to 16 kHz mono PCM16. Each pair opened two real ElevenLabs Scribe v2 realtime sockets with independently minted short-lived tokens, identical settings, simultaneous 50 ms paced chunks and five-second manual commits. One socket used a direct protocol baseline; the other used the unmodified production `CloudSpeech` transport with deterministic AudioContext/Worklet/MediaRecorder adapters. No native microphone, personal recording, real account transcript, AWS application write or deployed speech setting was involved. Maximum audio submitted was 30 × 2 × 14.071 seconds (about 14.1 minutes of synthetic audio).

| Quantity | Direct baseline p95 | Production transport p95 | Interpretation |
| --- | ---: | ---: | --- |
| Provider token request | 324.1 ms | 327.1 ms | Startup only; provider API called directly, **not Lambda token mint timing** |
| Socket ready after token | 284.7 ms | 293.9 ms | Startup only; excluded from steady-state pairs |
| First partial after shared audio start | 2230.3 ms | 2219.8 ms | Recognition time, not added application delay |
| Commit event after scheduled commit | 267.4 ms | 273.4 ms | Recognition/transport delay after a manual commit |
| Paired extra committed-text delay | — | 72.3 ms | p95 across 90 paired committed events; median extra 0.07 ms |

All 30 pairs returned the same normalized final words, three committed segments per stream and the final fixture phrase. Those checks establish completeness for this fixture; they are not an accuracy estimate or a representative language corpus. Raw content-free timestamps are in [samples.json](samples.json); [summary.json](summary.json) records the failed acceptance status and [aggregate.json](aggregate.json) separates startup and recognition.

Device fan-out is a separate text-only path. The three-device local browser suite records click-to-receiver-render wall time in [browser evidence](../checks/browser.json); that includes UI automation and a deliberately lost acknowledgement on loopback. It is neither AWS fan-out latency nor a one-way latency measurement. Prior AWS text tests are linked in the main evidence report; no new production fan-out writes were authorized here.

To reproduce the bounded **billable provider experiment**, first obtain authorization for that run and use a managed `ELEVEN_LABS_STT_TOKEN` (or its supported item-scoped alias). It is read from the environment, never put in arguments or artifacts. Do not substitute private native audio.

```sh
mkdir -p tmp/speech-measurement
ffmpeg -nostdin -i docs/verification/account-sync/speech/synthetic.wav -ar 16000 -ac 1 -f s16le tmp/speech-measurement/source.pcm
node hosted/scripts/measure-speech.mjs tmp/speech-measurement/source.pcm tmp/speech-measurement 30 --run-provider-measurement
```

Before activation, repeat with real phone/laptop browser capture on representative networks, report cold Lambda token startup separately, measure paired steady-state delay and missing-final-word/interruption recovery, and measure hosted text fan-out separately. Keep the p95 added steady-state gate **strictly below 100 ms**. No server audio relay is proposed.
