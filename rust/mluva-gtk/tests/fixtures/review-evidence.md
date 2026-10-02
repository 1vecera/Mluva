# Native document rewriting and review

The immutable comparison is unchanged Mluva v1.6.0 at `5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f`. Temporary collectors import the external released application, verify every imported application module byte-for-byte against that commit and invoke its actual service initialization, capture/workspace builders, rewrite worker and review callbacks. Maintained tests read the resulting static fixtures and use native production owners; no interpreter or patched reference supplies their results.

| Independently frozen observations | Scope | SHA-256 |
| --- | --- | --- |
| [Document/review controller](released-review-controller.json) | 29 released transactions / 102 actual GTK states, durable replies and exact text/image requests | `2b9935ad3bd01adb68efc84b5b223d1fbf2849113c746f827acea2046e33176c` |
| [Recording/review projection](released-overlay-state.json) | 78 bounded projection cases and ten actual private-bus signal receipts | `b789183be925a309d4fa79cdcc0a159cb76c0ab38dac190425e0ddc237e7851a` |

`ReviewController` owns one real rewrite client, frozen copy preference and prompt/image request, a bounded-cadence provisional preview bridge and a weak completion gate. Actual GTK workspace buffers, notices, button sensitivity, selected conversation, typed widget projection, durable SQLite replies, clipboard bytes and exact child/workspace cleanup match the release. The native default factory creates the client; a private first-PATH executable adapter selects separate native Codex protocol children at the external process boundary. Gated discovery/turns and paced deltas make the independently observed request and stream states reproducible without a production hook.

The cases cover successful streaming/completion, automatic copy and a later copy-preference edit, a manual prompt edit during the request, another conversation/review selected during the request, deletion, cancellation, privacy changes, provider failure, invalid thinking/Fast selection, duplicate requests, latest-reply versus edited-source Copy, stale/unknown actions, dismiss/open/continue dispatch, saved polish overrides/custom styles and disabled/Incognito/busy/blank request guards. Continue is observed at the application callback boundary; actual continued recording has separate [Live/capture evidence](live-evidence.md). The source image case deletes the actual screenshot file while model discovery is gated; both implementations still dispatch identical previously frozen PNG bytes and narration offset. Full text/image input arrays are compared, not only prompt substrings.

The first native comparison failed because streamed review snapshots retained rewrite options; the release hides those options while streaming and restores them for a completed review. The production projection now preserves that distinction, and all 29 transactions/102 states pass. The external peer's document-control parser was also corrected to accept the reference's appended image-context paragraph; this was collector support, not a reference or production repair. One additional native exit fault drops the owner during a real gated turn: after 600 ms there is no late SQLite reply, clipboard write or review content, and exact provider children/workspaces are gone.

`OverlayState` preserves both released D-Bus contracts: `/com/mluva/Linux/RecordingStatus`, interface `com.mluva.Linux.RecordingStatus`, `StateChanged (bssussdss)` and `ShellStateChanged (a{sv})`. Independent cases check visible/review/hidden phases, casefold versus exact shell phase handling, Unicode whitespace/character limits, complete-word preview boundaries, integer/finite-level clamps, options/identifier/message bounds and widget settings. `OverlayPublisher` retains only the bounded variants on the caller's existing connection. A second actual private connection observes publish, replay, review, clear and replay-after-clear signals, and a closed connection fails safely. Replay after Clear contains only the erased projections.

Renderer versions are GTK 4.22.4 and Pango 1.58.2, with animations/cursor blinking disabled. Generated History identities alone become symbolic owners; source custom-style identities remain the concrete input. First-text elapsed numbers in notices are normalized because they are measurements, not deterministic content. No animation or performance claim follows. The fixtures are not derived from native outputs.

```sh
cargo build --locked -p mluva-providers --bin codex-fixture-peer
cargo test --locked -p mluva-gtk --test overlay_state
bash dev/run-isolated-browser.sh tmp/native-review -- bash -c '
  export PATH="$OFFSCREEN_SESSION_ROOT/review-codex-tools:$PATH"
  export TZ=UTC
  exec cargo test --locked -p mluva-gtk --test review_controller -- --ignored --test-threads=1 --nocapture
'
bash dev/run-isolated-browser.sh tmp/native-overlay -- \
  cargo test --locked -p mluva-gtk --test overlay_state -- --ignored --test-threads=1 --nocapture
```

All GUI/bus checks run inside private display/session/accessibility buses, HOME/XDG, network/PID/mount and masked-device boundaries. Serialize desktop tests. No visible host desktop/input, real microphone, provider account, managed credential or user content is used. Source collectors, logs and the original publication mismatch remain private ignored scratch.

This establishes the document/review owner and projection transport, not the complete application's action wiring, review timeout/lifecycle, full Live-final review integration, credential/model discovery, title owners, welcome/settings, Meeting/archive, screenshot picker/Tensaku, portal/global keys or installation. The existing Live/capture comparison was rerun after the shared peer and public final-owner changes; its bounded scope remains unchanged. Layouts/scales, real microphone/all delivery targets, complete package/Python removal and whole-app performance remain required. Every complete-workflow row stays Pending; the installed app/widget remain v1.6.0 and the full Rust goal stays active.
