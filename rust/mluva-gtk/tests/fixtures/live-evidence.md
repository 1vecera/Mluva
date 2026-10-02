# Native Live draft and capture integration

The immutable reference is Mluva v1.6.0 at `5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f`. Temporary collectors import the external downloaded release, verify each imported application module against that commit and invoke its actual Live prompts, scheduler, application services, GTK builders and callbacks. Collectors and raw logs remain private scratch; maintained native tests read static observations without an interpreter.

| Frozen independent observations | Scope | SHA-256 |
| --- | --- | --- |
| [Live policy](../../../mluva-core/tests/fixtures/released-live-policy.json) | 23 schedules / 320 public transitions and 61 exact prompt decisions | `39943755805db2273aeefa0c78aa9799a715c02287c884deea4d525eabdbb687` |
| [Live controller](released-live-controller.json) | 16 released transactions / 68 GTK states | `2013866bf1c8515762e069b736de2a2056769505161180fa98bcbb187837b550` |
| [Capture with Live](released-capture-live.json) | 10 actual released captures / 54 GTK states | `9e4ba51faef8d23a0f49cd501d2a3463df5314295b49c956defd0a0942def719` |

Policy observations cover all five templates, frozen overrides, custom instructions, Unicode escaping, the initial 40-character threshold, interval/delta coalescing, pause/failure/in-flight/final guards and eighteen context-limit boundaries. Large repeated transcripts are encoded compactly; their complete released prompt SHA-256, character count and UTF-8 byte count are compared. Ordinary prompt bytes remain explicit. Only equivalent integral/floating clock representations and the negative-infinity sentinel are normalized.

The native GUI comparison uses the production `ConversationWorkspace`, `CapturePage`, `LiveController`, `CaptureController`, `CaptureSession`, `DesktopRuntime`, `PromptStore`, actual rewrite factory and compatible SQLite stores. A private executable adapter selects separate native Codex protocol children; real HTTP/WebSocket peers and a synthetic PCM process supply only the external provider/microphone boundaries. No production factory override or test-only access path supplies the Live result. Whole drafts are published after completion, never as half-streamed edits.

The sixteen Live transactions check provisional/final requests, manual revision guards, final coalescing behind an active preview, paused/partial labels, failed providers, successful automatic copy, immutable prompt/config snapshots, cancellation/old-generation suppression, edits during final reconciliation, preservation of another browsed conversation, durable save failure with raw/draft recovery and continued full-source reconciliation. Actual request bytes, child counts/reaping, replies, review labels and private clipboard bytes match the unchanged application.

The ten joined captures run real preparation, recording and Stop/cancel callbacks. They verify immediate realtime notification, final reconciliation and refusal of a second recording during a pending final draft; recording edits; Once disarming on cancellation and batch completion; empty/failed speech preserving the draft; enabling Once during capture; pausing an active provider; and recording into an existing conversation. Entire normalized History, current/persisted preferences, complete draft UI, PCM/HTTP uploads, retention and child cleanup match. Compatible/local recognition previews retain their released polling behavior. The continued-capture case failed before production integration repaired root resolution, prior-edit saving, parent display, preview seeding and appended full-source ownership. The preserved failing comparison is private scratch; the final run passes all 26 transactions and 122 GTK states.

The existing joined capture owner was rerun after the Live/continuation changes: all 46 transactions, 218 states and three native exit/privacy faults still pass. The complete ordinary workspace passes 125 tests with nineteen environment-dependent checks ignored; the new ignored Live owner and existing capture owner were executed separately. Strict workspace Clippy and formatting pass. GTK 4.22.4 and Pango 1.58.2 are pinned by the observations.

```sh
cargo build --locked -p mluva-audio --bins -p mluva-providers --bins
bash dev/run-isolated-browser.sh tmp/native-live -- bash -c '
  export PATH="$OFFSCREEN_SESSION_ROOT/live-codex-tools:$PATH"
  export TZ=UTC
  exec cargo test --locked -p mluva-gtk --test live_controller -- --ignored --test-threads=1 --nocapture
'
```

The runner isolates display, session/accessibility buses, HOME/XDG, network/PID/mount namespaces and memory-backed staging, masks input/audio/GPU devices and selects software rendering. No host input, real microphone, provider account, credential or user content is used. Serialize GUI runs. Application-level provider discovery, welcome/settings, review announcements and shutdown assembly remain unimplemented; complete Grilling/template interaction, compatible/local Live capture, screenshot waiting/context, finalizing-exit behavior, other layouts/scales/platforms, real microphone/delivery, packaging/Python removal and whole-app performance still require acceptance. Component success does not close any complete-workflow row or authorize installing the Rust app.
