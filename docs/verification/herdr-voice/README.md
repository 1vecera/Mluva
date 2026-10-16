# Herdr voice verification

Verified on Lenovo on 16 October 2026 from the isolated `feat/herdr-voice-routing` worktree, based on Mluva `58d1a28f1187f7ce6aa778202ec6020285d18fef`. The installed Mluva/Herdr, desktop, microphone, clipboard and private recorder were not changed by these tests. All GUI/process fixtures use synthetic content and private display, D-Bus, network, device and HOME/XDG state.

## Reproduce

```sh
make linux-test
make linux-shortcut-test
make linux-application-test
make linux-agent-voice-test
shellcheck install.sh linux/*.sh linux/tests/*.sh dev/*.sh linux/mluva-shell
# Build the pinned dependency using integrations/herdr/README.md, then:
bash linux/tests/run_herdr_voice_smoke.sh /absolute/patched/herdr
```

The standard native prerequisites apply. The optional real-agent proof also uses `uv`, bubblewrap, installed Codex/Claude CLIs and the patched Herdr binary. It creates its evidence under `tmp/herdr-voice/`, mounts a short private path internally and has no external network or inherited credentials. Its HTTP model responses are synthetic, with no tools requested. No live Herdr socket is exposed through its private `/run`.

| Boundary | Evidence |
| --- | --- |
| State machine | Five focused core tests cover finalization before grace, cancellation in each phase, edits, replacement/restart/blocked identity, lost release, stale timers, empty/oversized recognition and private settings. |
| Herdr client | Four protocol tests cover metadata-only old servers, private socket checks, accepted-but-lost acknowledgement with receipt-only reconciliation, and stale/mismatched receipts. Requests contain final text and routing metadata only. |
| Assembled Mluva | [Application result](application-result.json): ten actual private PCM captures, nine recognitions, four guarded submissions and seven cancelled/disarmed captures. Includes a working target, duplicate display names, saved edits with immutable raw text, explicit draft sending and editing during its renewed countdown, saved routing opt-in, rename/replacement, lost acknowledgement, uncertainty, normal Stop, cancellation and Incognito. Clipboard is unchanged despite automatic copy/paste settings being enabled; the selected STT provider is unchanged. |
| Global shortcuts | The real private D-Bus owner passes the existing 21 portal workflows/59 observed states plus agent hold/release, repeated presses, foreign-session events, Keep draft, existing F9 and post-shutdown silence. |
| Native recipients | [Native result](native-agent-result.json): actual Codex 0.160.1 and Claude Code 2.1.301 TUIs in patched Herdr 0.9.5 each submit one synthetic message to their own loopback model endpoint. Same-operation retries return receipts without another message. Rename invalidates the guard while both recipients stay running; a stale server epoch is rejected. |
| Native session setup | Claude's actual SessionStart integration reports its identity. A fresh Codex run stops at hook review and is unavailable for guarded sending. The transport fixture disables automatic hooks and reports its actual newly created session through the reviewed Herdr hook. Automatic Codex hook-trust acceptance remains a manual setup check. |
| Speech output | Three local WebSocket/player tests verify v4 voice registration, short-message flushing, no key/audio in request bodies, actionable permission errors without provider-detail leakage, and cancellation that kills/reaps a stalled player before returning. |
| Compatibility | The required native gate passes 167 tests, with 75 explicitly ignored integration/resource tests; changed GUI boundaries were run separately as listed above. Both strict Clippy configurations, formatting, generated-feature consistency, existing assembled application/shell regressions and shell lint pass. Herdr's 51 schema, 71 frozen-wire and 24 PTY tests pass, along with its guarded receipt tests and strict Clippy. The original released maturity table is checked unchanged after excluding the added Experimental row. |

## Timing and live provider boundary

The application observation uses one synthetic HTTP recognition sample: release → final text **259 ms**, intentional grace **2,092 ms**, dispatch → fixture receipt **8 ms**. With n=1, percentiles would be misleading and are not reported as performance evidence.

The actual native-agent transport has n=2 observations: **320 ms** and **406 ms** from dispatch to receipt, including Herdr's intentional 300 ms paste/Enter separation and receipt polling. Sample median is **363 ms**; nearest-rank sample p95 is **406 ms**. This small deterministic fixture run is not a production latency benchmark, recognition measurement or correlated agent-response measurement.

Live `eleven_v4_turbo` WebSocket connections were attempted with a synthetic sentence and existing authorized credentials. Both the saved speech key and managed fallback returned `missing_permissions`: `text_to_speech` is absent. No audio was received or played. [Provider result](provider-result.json) records the failed gate; reply-to-first-audio and total playback p50/p95 are unavailable. No provider speed or natural-voice quality claim is made, and no ongoing paid voice setting was enabled. With an appropriately scoped key, `cargo run --locked -p mluva-providers --example spoken_status_probe -- tmp/voice-proof VOICE_ID` records six bounded English/Czech samples and first-received-PCM timings without playing them.

## Native visual check

The existing Mluva editor, typography and controls are retained. Impeccable's craft review identified and removed a duplicate output editor, preserved normal editor space when routing is disabled, and checked the narrow recovery layout. Its HTML/CSS detector returned no findings for the Rust targets; that result is not an automated GTK accessibility or contrast certification. A narrow-window transition still logs a GtkText allocation warning; visible controls and editable content remain usable in the captured state. The settings popover has bounded width and scrollable content; the application test asserts its rendered allocation and saved enable/disable behavior.

| State | Rendered evidence |
| --- | --- |
| Ready | [Laptop](screenshots/agent-ready-laptop.png) |
| Recording | [Laptop](screenshots/agent-recording-laptop.png) |
| Final text and countdown | [Laptop](screenshots/agent-countdown-laptop.png) |
| Saved editable draft | [420 px native window](screenshots/agent-draft-narrow.png) |
| Replaced recipient | [420 px native window](screenshots/agent-unavailable-narrow.png) |
| Unconfirmed delivery | [Laptop](screenshots/agent-unconfirmed-laptop.png) |
| Settings | [Native popover](screenshots/agent-settings-laptop.png) |

Physical Wayland keys/portal approval, live Codex hook review, actual microphone feedback prevention and natural English/Czech listening are still unverified. These cannot be inferred from Xvfb, private portal fixtures, a reaped synthetic player or failed cloud authentication. Remote servers and browser routing are outside the implemented local Linux boundary. The installed Herdr must receive the separately reviewed dependency before automatic sending can be enabled. No merge, release, installation or hosted deployment is part of this verification.
