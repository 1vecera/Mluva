# Managed preview Stop, Cancel and a fresh recording

5 October 2026: an actual native application can briefly publish `processing` during Cancel when its queued widget timer runs while asynchronous microphone/provider cleanup drains. The unchanged 1.6.0 application retains its recording feedback until cancellation clears the overlay. The prepared repair preserves that feedback during `Cancelling`; ordinary Stop still publishes `processing` and completes recognition. Internal cancellation ownership and cleanup remain active.

[The frozen fixture](released-bootstrap-managed-capture.json), SHA-256 `3de193467c4c89b3ed372fecf7ad2cd74adb09021e69f784b9e3b9627bdbe29a`, contains two independently observed actual released processes and six public states. The observer checks all 91 packaged application modules/resources against immutable commit `5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f` before launch. It invokes only public GTK D-Bus actions, records the nine-field `StateChanged` payloads, reads actual SQLite History and retained WAVs, observes owned runtime PIDs/key paths, and captures complete 1100×800 RGB windows.

Both scenarios feed exactly eight seconds/256,000 bytes of pinned public English PCM through real recorder pipes, in native external endpoint fragments of 320 bytes paced at 32,000 bytes per second. Sparse pinned readiness files admit the ordinary managed CPU Qwen client; a native executable serves actual authenticated loopback HTTP and holds its first process's preview. The pending application must still show `Waiting for speech…`, with no History, one live runtime and its readable private key. This is controlled recognition behavior, not physical model inference or microphone acceptance.

| Public action | Required result |
| --- | --- |
| Stop during held preview | Reap the preview runtime/key, start fresh final recognition, save `hello` with the complete eight-second WAV and ordinary History metadata, then reap both runtime/key pairs. |
| Cancel, then Record and Stop | Reap the first runtime/key, clear the overlay, leave History and retained audio empty; a fresh capture shows provisional `hello hello`, then saves only final `hello` and the complete WAV. |

The exact authenticated payloads include two requests for Stop and four for Cancel/restart. Full wave headers, audio/body hashes, arguments, controlled environment, key/directory modes and all History columns remain compared. Generated UUIDs, creation time and retained filename normalize only after validation; recording elapsed/level and recognition timing are bounded sampled fields. Random runtime PID/key paths and health retry counts normalize while the complete health-status set and unauthenticated health requirement stay exact. Phase order starts at the action's first `preparing`: a queued earlier startup/status replay remains in the raw receipts, and no later phase is removed.

Three stable window observations match complete RGB hashes: the cancelled window is `b5c44b24fb2c57525e1b8afdf5286c0007c644d1ffbf169910d303bfeab9f6c0`, and both final windows are `3ff2e89d650231aeb3576df4284959e02ed07f433fc8509b844f0d74364d55ac`. Detailed inspection of the pending Stop pair finds 59 changed pixels across the sampled `00:07`/`00:08` elapsed display and the preview text edge. The Cancel pair has three changed pixels at that text edge. These raw frames remain recorded without masks or a transient-pixel parity claim.

The existing `released_process_actions_residency_and_headless_dispatch` Rust owner exercises these two scenarios through separate real relocated application processes. It additionally repeats Cancel with a 500 ms delayed external microphone close, ensuring a queued widget tick occurs during cleanup. That native timing stress uses the same frozen public phases, final states, protocol and pixels; it is not a third independently observed release scenario. The existing 38 public bootstrap states, thirteen action descriptors, five headless cases, cold Record and three startup faults stay in the same owner.

The initial full-process native comparison failed on an extra `processing` phase during Cancel; stable final pixels, History, audio and protocol already matched. Its raw observations remain in `tmp/qwen-joined-capture-native-owner/session.aB9M2L/native-bootstrap/` and `tmp/qwen-joined-capture/native-owner.log`. Independent source/native collection is retained in `tmp/qwen-joined-capture-maintained-peer/session.N5iGrg/qwen-joined-capture/results.json`. Two earlier observation errors, waiting too little for the fixed input and expecting a different overlay phase, are excluded and preserved separately. They did not change either application or the source oracle.

Build the native prerequisites, reuse the already frozen public English frontend input, then run the process owner. The decompressed full PCM must have SHA-256 `6b358de4826842f192a25fee6cd7be3a30f4a609ec74e69b6274c9a6fc265c91`:

```sh
cargo build --locked -p mluva-gtk --bin mluva \
  -p mluva-audio --bin mluva-audio-cleanup --bin audio-fixture-peer \
  -p mluva-providers --bin credential-fixture-peer --bin qwen-fixture-peer
mkdir -p tmp/native-bootstrap-input
gzip -dc rust/mluva-asr/tests/fixtures/nemo-public-en.pcm16le.gz \
  > tmp/native-bootstrap-input/asr-en.pcm
bash dev/run-isolated-browser.sh tmp/native-managed-bootstrap -- \
  env MLUVA_TEST_QWEN_PCM="$PWD/tmp/native-bootstrap-input/asr-en.pcm" \
  cargo test --locked -p mluva-gtk --test bootstrap \
  released_process_actions_residency_and_headless_dispatch -- --ignored --exact --nocapture
```

The [physical Qwen evidence](../../../mluva-providers/tests/fixtures/qwen-inference-evidence.md) identifies the public audio and its 16 kHz PCM normalization. This process owner needs neither weights/inference nor Python; its managed runtime and audio endpoints are development binaries excluded from distribution. The private runner isolates the display, session/accessibility buses, HOME/XDG, network, PIDs and devices. No host focus/pointer/input, microphone, provider account or user-content upload is used.

The final process owner passes all three managed cases/ten states alongside its unchanged bootstrap/cold-record/startup checks in `tmp/qwen-joined-capture/native-final.log`. The 500 ms negative control failed only on the extra cancellation phase, in `tmp/qwen-joined-capture/native-delay-negative.log`. Actual native gates pass 141 ordinary tests, zero failures and 71 ignored checks across 98 suites, both strict Clippy configurations, formatting and generated consistency. The affected assembled application separately matches fifteen workflows/147 states, with an open GtkText sizing warning during `plain-saved` continuation; its assertion pass is not clean GUI acceptance.

The [worker lifecycle record](../../../../docs/verification/rust-worker-lifecycle/README.md#joined-gtk-preview-stop-cancel-and-fresh-recording) retains validation and compact raw/source receipts. Complete Live/revision/target workflows, the continuation warning, crash recovery/filesystem cleanup, the inherited incomplete-line response deadline, physical keys/devices and supported-platform acceptance remain open. This bounded check does not close any complete-workflow matrix row or authorize a release/install; published/installed 2.0.0 remains unchanged.

The subsequent [managed Live process comparison](bootstrap-managed-live-evidence.md) extends this same owner to held finalization with a manual edit and final Cancel, without adding another ignored test or changing production. Its eleven independently observed states and the original checks pass together. The [continuation baseline audit](../../../../docs/verification/rust-worker-lifecycle/README.md#continuation-warning-baseline) separately establishes that the narrow GtkText warning is shared with the unchanged release; it still is not clean diagnostics or full layout/platform acceptance.
