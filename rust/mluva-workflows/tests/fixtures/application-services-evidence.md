# Application recovery, settings and capture services

The immutable reference is released Mluva v1.6.0 at `5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f`. Temporary collectors import the external release and verify every imported application module byte-for-byte against that commit. They invoke the actual application initialization, Scratchpad restoration, settings-save handler, capture factory and recording callbacks. Maintained Rust checks consume static expected outputs and native production owners; no interpreter or patched reference supplies their results.

| Independent observations | Scope | SHA-256 |
| --- | --- | --- |
| [Local application services](released-application-services.json) | Eleven startup/recovery transactions and 36 settings-save transactions | `a0d068d00e92de41382dccec3c5332f9d69fd16b4aeb1a057cfe4152451a8be1` |
| [Capture factory](../../../mluva-gtk/tests/fixtures/released-capture-factory.json) | Six actual initialization transactions, including two recordings/eight GTK states | `4a1c2814d92043bd824dbb3231aef270f5ca7d89320de0670b92c6c7eb48266f` |
| [Title jobs](../../../mluva-gtk/tests/fixtures/released-title-jobs.json) | 28 actual title transactions/98 owner states and exact prompts/models | `31140703a9f532e3b1ffd99ad385d4bda94e7d4eb557fe31d57692c9db891dd5` |

`ApplicationServices` opens compatible settings, personalization/prompts, History/conversations, screenshots, Scratchpad and diagnostics before speech credentials. Interrupted screenshot captures recover into failed History entries unless Incognito is active. Malformed documents stay intact, unresolved Scratchpad identities remain excluded from retention, invalid persisted private drafts erase only managed audio, and runtime/data/database permissions match. The first comparison failed because restoration did not remember Scratchpad mode; the native owner now uses the saved-mode path and all eleven cases match.

Settings changes preserve the released distinction between presentation/Live and provider changes. Presentation remains editable while busy; Live changes reject preparation, processing, final/rewrite ownership and private/non-dictation recording. Provider changes reject active capture/rewrite/Live work. Skip disarms Live and automatic titles, invalid input is refused, unchanged choices succeed without a write, and failed saves keep current settings and drafts. The parent still owns applying returned effects to the exact active desktop/provider services.

The capture comparison uses `ApplicationServices.capture_services` and `CaptureServices.launch`, the real GTK capture controller, native provider factories and actual external PCM/HTTP boundaries. Both compatible-endpoint recordings match complete GUI states, final results, immutable recognition/working History, WAV/multipart requests and capture-frozen personalization after a mid-recording edit. `/v1` remains in the actual request path. All three retained local models initialize without cloud credentials; this checks readiness construction, not inference. Missing ElevenLabs credentials keep recovery stores usable. No cloud request, real microphone or model inference runs in this check. Existing physical model evidence retains its separate scope. The synthetic HTTP observer initially rejected the API prefix; its endpoint parser now accepts the same speech suffix as the source observer. This was test support, not a production/reference repair.

`ConversationTitleJobs` writes a local fallback before bounded model work, permits at most twenty queued requests behind one active request, and freezes instructions and capture/default model at dispatch. Document rewrite model/thinking/Fast settings do not enter title turns. Native GLib ownership preserves rename/clear/delete compare-and-set, current privacy/provider/title settings, cancellation, shutdown and identity checks across a new generation. Invalid/oversized/failed output keeps the fallback. Queued renamed/deleted notes are skipped; later queued jobs use current instructions/model. The test compares title revisions, immutable raw text, changed callbacks, queue/active state and complete prompt/model requests, with exact child/workspace reaping. An additional native owner-exit fault produces no late title or callback after 600 ms.

The capture/title checks execute separately inside private display/session/accessibility, HOME/XDG, network/PID/mount and masked-device boundaries. Only capture observations are GTK widget states; title observations are GLib-owned queue/SQLite states. Renderer versions are GTK 4.22.4/Pango 1.58.2, with motion and cursor blinking disabled. Generated identities and measured recording milliseconds alone are normalized. Source collectors, raw logs and the original recovery mismatch remain private ignored scratch.

```sh
cargo test --locked -p mluva-workflows --test services
cargo build --locked -p mluva-audio --bins -p mluva-providers --bin codex-fixture-peer
bash dev/run-isolated-browser.sh tmp/native-capture-factory -- bash -c '
  export PATH="$OFFSCREEN_SESSION_ROOT/factory-tools:$PATH"
  export TZ=UTC
  exec cargo test --locked -p mluva-gtk --test capture_factory -- --ignored --test-threads=1 --nocapture
'
bash dev/run-isolated-browser.sh tmp/native-title-jobs -- bash -c '
  export PATH="$OFFSCREEN_SESSION_ROOT/title-codex-tools:$PATH"
  export TZ=UTC
  exec cargo test --locked -p mluva-gtk --test title_jobs -- --ignored --test-threads=1 --nocapture
'
```

This establishes local store/recovery ownership, settings-save policy, capture readiness/session construction and title jobs. Complete window/service assembly, audio-device refresh, credential/catalog/settings/welcome UI, active reconfiguration and shutdown, retained Command/Scratchpad/retry acceptance, Meeting/archive, Live-final review, screenshot picker/Tensaku, portal/global keys, all targets/platforms/scales, distribution/Python removal and whole-app performance remain required. No complete runnable native application is claimed. Every complete-workflow matrix row stays Pending; installed app/widget remain verified v1.6.0 and the full Rust goal stays active.
