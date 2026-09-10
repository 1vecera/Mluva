# Focused development review — S27-468

Keep the working Python/GTK + Swift implementation. The useful changes are a feature map, one small command boundary and faster HTTP fixture cleanup. This review starts from merged PR #20, `6022b05e4258768e7aa70305febf7dad0835b060`, on claw-mini on 2026-09-10. The naming migration and launch-film resources are outside this change.

## Findings and changes

| Evidence before | Result |
| --- | --- |
| The 34-line `CLAUDE.md` described mostly Swift services despite Linux being the supported release. README sent developers first to campaign-oriented capture documentation. | README and an 11-line `CLAUDE.md` now lead to the [contribution code map](../CONTRIBUTING.md#code-map), covering both clients and eight feature areas. Capture provenance keeps its existing owner. |
| Eight Ctrl+P definitions and their guards lived inside the 4,979-line `app.py`, separate from the 146-line panel. | `command_palette.py` now owns `application_commands` and the panel in 242 lines. `app.py` is 4,891 lines; `_show_commands` only manages dialog lifecycle. No action, callback, document identity or availability condition changed. |
| Eight repeated HTTP server startup/shutdown blocks across four test modules paid the default 0.5-second shutdown poll on each use. Slow-duration output repeatedly attributed about 0.51 seconds to catalog/transport teardown. | One 20-line `http_fixture.py` owns the loopback socket and thread, including cleanup on failure. A 0.01-second idle poll removes teardown delay. Handlers, real HTTP/SSE/multipart exchanges, request timeouts and every assertion remain. |
| No named fast path or command-panel gate; an agent had to reconstruct the isolated environment for `command_palette_smoke.py`. | `make linux-test-fast` selects 64 existing text/editing cases. `make linux-command-test` runs the existing native lifecycle on a private display. `make linux-test` retains all 381 cases, lint, format and generated-feature checks. |
| `docs/test-review.md` still read like current evidence for 282 Linux tests and an unavailable macOS host. | Marked explicitly historical and linked to this review; its original evidence is preserved. |

The app coordinator still owns too much: window composition, capture, Meeting, rewrite/title jobs and persistence callbacks. The extraction is intentionally small; an 88-line reduction is a navigation improvement, not a claim that the coordinator is now modular. Recording already has useful boundaries in `audio.py`, `realtime.py`, `segment_cleanup.py` and `workflow.py`; persistence and target delivery also have distinct modules. A broad move would couple unrelated session state and create more review risk than this task warrants.

## Worked feature-change paths

**Add a speech route.** Follow `AppConfig.transcription_provider` → `provider_catalog.SPEECH_PROVIDERS`/`CatalogRequest` → `providers.transcription_client` → `app._configure_realtime_provider` and capture preparation. The first pair owns selection/discovery, the latter pair owns execution. Scribe streams natively; other routes get batch previews only with Live enabled. Meeting deliberately uses its separate diarized ElevenLabs path. Run provider catalog/workspace and capture/workflow checks; adding a picker row alone does not implement a route.

**Change a Live draft after the user types.** Start at `LiveRewriteSchedule.take` for timing, `live_prompt` for template input, and `_live_rewrite_finished` for the session/client/revision gate. `ConversationWorkspace` owns editable buffers; `ConversationStore` saves working versions while the history entry’s `raw_text` retains recognition. `_save_live_draft` must only auto-copy a successful final reconciliation. The native Live smoke checks manual edits during requests and finalization, cancellation, Incognito, failed-final copy suppression and one final saved draft. Keep this lifecycle together until a concrete change needs a separate controller.

**Add a Ctrl+P action.** Start at `application_commands` beside the searchable panel; route the action to the existing application/workspace callback. `enabled` is a callable evaluated at navigation, activation and after dismissal. Document commands capture both the conversation identifier and editor object, so switching conversations or replacing an editor cannot redirect an already-open action. `make linux-command-test` checks native editor focus, lossless Copy/Save, stale and hidden actions, provisional text restrictions, dismissal and narrow-window scrolling.

These are traced paths through current features, not speculative extension APIs. The `_new_rewrite_client` override is used by both the native Live smoke and capture subclasses; removing it as apparent indirection would break useful injection boundaries. No dead-code deletion or test-case removal was supported by this review.

## Measurements and retained checks

Linux ran in the existing Fedora 44 arm64 development image on claw-mini, in a container owned by this checkout with two CPUs and 3 GiB RAM. Python was 3.14.7; dependencies came from `linux/uv.lock`. Timings are single observed wall-clock runs after `linux-setup`, using Bash `time -p` and `PYTEST_ADDOPTS=--durations=20`; they are not statistical benchmarks. Swift builds ran concurrently, so host load varies. No external providers, user credentials, microphones or active desktop state were used. Native fixtures use only their private clipboard and target windows.

| Check | Before | After / result |
| --- | --- | --- |
| Complete Linux gate, including setup validation, feature generation check and Ruff | 27.37 s; 381 passed | 9.24 s; 381 passed; lint/format/generated checks pass |
| Pytest portion of that gate | 26.74 s | 7.42 s; same 381 cases |
| Text/editing fast path | No named target | 0.86 s including setup; 64 passed in pytest's 0.62 s |
| Ctrl+P private native lifecycle | Existing script, no Make target | 5.01 s; all 14 recorded acceptance assertions pass; wide/narrow screenshots inspected |
| Shortcut portal | Retained | Private D-Bus registration, binding and lifecycle pass |
| GNOME overlay | Retained | 10.02 s; all six headless scenarios pass; recording/copied screenshots inspected |
| Native target + Live workspace | Retained | 16.30 s combined; exact focus/Unicode/target checks and all 14 Live assertions pass; finalization screenshot inspected |
| ShellCheck | Retained | Passes `linux/*.sh`, `linux/tests/*.sh`, `scripts/*.sh`, `dev/*.sh`, `linux/mluva-shell` |
| Swift baseline `swift test` | Cold checkout: 230.53 s, including dependency fetch/build | 292 tests; two failing test cases / three assertions, described below |
| Swift handoff `swift test --disable-dependency-cache` | Existing CI command | 57.69 s, including a dependency/tool rebuild; 292 tests, one existing failing assertion |

The GNOME gate used a disposable image derived from `mluva-dev:0.1.1` with Fedora’s `gnome-shell` 50.4 package added. It needed a private system-bus substitute: inside that image, run `dbus-run-session -- bash -c 'export DBUS_SYSTEM_BUS_ADDRESS="$DBUS_SESSION_BUS_ADDRESS"; make linux-overlay-test'`. No host bus or desktop is mounted. The base development image lacks this package; the unchanged runner cannot start GNOME without these prerequisites.

The Linux suite retains lossless transcript and Markdown checks, editable-source persistence, Incognito/retention, stale document actions, actual staged installed-entrypoint/rollback behavior and shortcut lifecycle coverage. The shared fixture changes teardown mechanics only. The remaining slowest cases exercise real cancellation/audio draining (about one second each) and a two-second realtime provider-error path; shortening those deadlines without separate evidence would weaken their purpose. No coverage was deleted to improve the timing.

The Swift failure is pre-existing at [TranscriptCleanupSessionTests.swift:142](https://github.com/1vecera/Mluva/blob/6022b05e4258768e7aa70305febf7dad0835b060/Tests/TranscriptCleanupSessionTests.swift#L142): input vocabulary is `Mluva`, a three-character bound produces `Mlu`, but the assertion still expects `Voi`. Leave that naming correction to S27-467. The initial run also failed the ordered-cleanup test at lines 171–172; that test passed on the full handoff rerun. It remains an intermittent scheduling concern, not a proven fix. Swift sources and tests are unchanged here.

Known Linux warnings remain: the GLib signal deprecation, software-rendering DRI3 warnings and the existing private AT-SPI cache warning, and GNOME warnings about unavailable desktop services on its private bus. They did not fail their assertions. Physical F9, live Wayland placement and real microphone/provider behavior remain manual acceptance; isolated tests do not establish them. CI is `workflow_dispatch` only and was not dispatched or billed for this review.

## Stack assessment

This recommendation is an engineering judgment from the code paths and checks above, not a migration benchmark.

| Option | Native UI/integration and provider interoperability | Cost, maintainability and agent effectiveness |
| --- | --- | --- |
| Keep Python/GTK + Swift | Retains Libadwaita, PipeWire, AT-SPI and portals on Linux, and AppKit/SwiftUI plus Apple capture/accessibility on macOS. Existing HTTP, WebSocket, subprocess and gRPC boundaries already isolate providers. | Lowest cost; preserves the working behavior and regression fixtures. The measured feedback loop and feature map address concrete agent friction. Two clients remain a maintenance cost, but there is no measured language bottleneck here. |
| Rust + GTK on Linux, retain Swift on macOS | [gtk-rs supports GTK 4 and Libadwaita](https://gtk-rs.org/gtk4-rs/git/book/libadwaita.html). This keeps the Linux widget approach; it does not remove the separate macOS integration. Provider transports must be ported and reverified. | High migration cost for capture, persistence, cancellation and provider code. Static types could help refactoring, but ownership/FFI and recompilation become part of agent work. Consider a bounded component only after profiling or a specific safety problem justifies it. |
| Tauri with Rust and a web frontend | Tauri uses a core process and OS webviews: WKWebView on macOS and WebKitGTK on Linux. UI actions cross an IPC boundary. Native audio, target restoration and shortcuts still need platform integration. [Tauri process model](https://v2.tauri.app/concept/process-model/) | High cost: rewrite the native editor/workspace and revalidate focus, lossless text, clipboard and manual-edit races across IPC. A shared UI might reduce duplication later; it also adds Rust/frontend/webview surfaces for agents to understand. No demonstrated benefit offsets this migration now. |

## Remaining recommendations

- When changing Meeting or title generation next, consider extracting that whole lifecycle with explicit state/callback ownership. Do not split `app.py` mechanically or introduce mixins/event buses.
- Provider selection is already bounded, but workflow fields named `elevenlabs` and `codex` can contain other clients. Document their actual role during provider work; introduce or rename interfaces only with a concrete consumer and compatible capture injection.
- Keep capture helpers and their source/runtime provenance intact. They are active consumers, not unused app code. Use the feature map for development and the capture guide for media work.
- Correct the stale Swift vocabulary expectation in the naming task, and investigate the ordered-cleanup timing with a deterministic synchronization fixture if it recurs. Retain the failure checks.
- Keep local dependency/build caches for iteration; the first Swift build dominated test execution. Continue reporting the full required handoff command separately from focused runs and from optional packaging/manual acceptance.

## Final review

Reviewed the production extraction, shared HTTP lifecycle, Make targets and every changed document against the issue scope. An AST comparison against the base confirms the moved catalog is identical after the receiver rename and returning the command tuple; changed test modules retain every test signature, parameterization and assertion. Native checks cover the callback behavior beyond that structural comparison. The full Linux gate and targeted GUI checks pass; the existing Swift failure is disclosed above. No film resources, other worktrees, installed app, provider transports or storage schema were changed.
