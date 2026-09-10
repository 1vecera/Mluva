# Contributing to Mluva

Mluva accepts focused changes that preserve its privacy, exact-target delivery, immutable raw-recognition, and recoverable-failure contracts. Fedora 44 with GNOME Shell 50 on Wayland is the initial supported release target; macOS remains a source preview until its distribution path is independently verified.

Contributions submitted to this repository are accepted under the repository's [Apache License 2.0](LICENSE).

## Local setup

Follow the [Linux guide](linux/README.md) for Fedora dependencies and the credential boundary. Tests use local protocol servers, fake audio and desktop boundaries, and generated content; they must not open a microphone, inspect the live accessibility tree, alter the clipboard, inject input, or open a shortcut approval dialog.

Run `make linux-setup` to prepare the locked Python environment with distribution PyGObject access. On macOS, the existing [development container](dev/legacy-container.md) can run Linux checks: use a checkout-specific `MLUVA_DEV_NAME` with `dev/box.sh up` and `dev/box.sh exec <command>`. The helper checks container ownership; keep the same name for subsequent commands. It mounts only this checkout, without the host desktop or credentials. Linux GUI smoke tests still need the private-display runners below.

## Code map

Bare Python filenames refer to [linux/mluva_linux](linux/mluva_linux); macOS service/model/view paths and app entrypoints are under [Sources](Sources). Tooling paths start at the repository root. Start at the feature's logic, then follow its calls into `app.py` for lifecycle wiring. GTK callbacks stay on the main thread; provider and audio work runs outside it.

| Change | Linux owner | macOS owner |
| --- | --- | --- |
| Recording and final delivery | `app.py`: `_start_capture`, `_prepare_capture`, `_finish_recording`; `audio.py` captures PCM, `workflow.py` prepares/persists/delivers the final result | `Services/RecordingController.swift`, `Services/AudioCaptureService.swift`, `Services/DeliveryCoordinator.swift` |
| Speech/rewrite provider selection | `config.py` stores choices; `provider_catalog.py` describes/discovers routes; `providers.py` creates speech clients; `app.py` wires rewrite clients and streaming; `provider_settings.py` renders choices | `Models/TranscriptionProviderKind.swift`, `Services/TranscriptionProviderFactory.swift`, `Services/TranscriptionProviderRouter.swift` |
| Live rewrite and manual edits | `live_rewrite.py` schedules/prompts; `app.py`: `_maybe_live_rewrite`, `_live_rewrite_finished`, `_save_live_draft` guard sessions/revisions; `conversation_view.py` owns editors and `conversation.py` saves working copies | Separate preview implementation: `Services/TranscriptCleanupSession.swift`, `Services/TranscriptProcessor.swift`; do not assume Linux Live feature parity |
| Commands | `command_palette.py`: `application_commands` owns Ctrl+P actions and availability. Spoken Command mode uses `workflow.py` and `app.py`: `_accept_command_preview` | `Services/RecordingController.swift` and `Services/TranscriptProcessor.swift` for spoken commands; `AppDelegate.swift` for app actions |
| History, recovery and privacy | `history.py`, `conversation.py`, `scratchpad.py`, `meeting.py` own persistence; `history_view.py` renders history; `workflow.py` enforces retention/Incognito | `Services/TranscriptionStore.swift`, `Services/TranscriptionRecoveryService.swift`, `Services/AudioRetentionStore.swift`, `Services/MeetingStore.swift` |
| Native UI and theme | `conversation_view.py`, `markdown_view.py`, `ui.py`, `theme.py`; settings/page views are siblings; `app.py` composes them | `Views/`, `AppDelegate.swift` |
| Desktop integration | `global_shortcuts.py` owns portal sessions; `text_target.py` captures/restores exact targets; `delivery.py` inserts/copies; `shell_bridge.py` and `overlay_state.py` connect the GNOME/QML surfaces | `Services/GlobalHotkeyManager.swift`, `Services/HotkeyGestureInterpreter.swift`, `Services/TextTarget.swift`, `Services/KeyboardSimulator.swift`, `Services/ClipboardTransaction.swift` |
| Installation and capture tooling | `linux/install.sh` and `linux/resources/mluva.in` own installed entrypoints; `dev/run-isolated.sh` owns the private desktop; [dev/README.md](dev/README.md) owns capture recipes/provenance | `scripts/build.sh` packages; [macOS guide](docs/macos-development.md) covers signing |

Raw recognition is immutable. Editor drafts, processed text and delivery results are separate. Live provisional text must never become final delivery; late provider output must pass session, privacy and manual-edit revision gates. A command opened on one document must not act on a newly selected document or replacement editor. Keep these guards beside the affected callbacks instead of introducing a shared state framework.

## Verification

For a quick text/editing change, run `make linux-test-fast` from the root. It covers transcript preservation, conversation/history persistence, scratchpads, Live scheduling and Markdown round trips. It is a subset, not handoff acceptance. For other changes, choose the tests that exercise their boundary:

| Change | Focused check after `make linux-setup` |
| --- | --- |
| Provider route or catalog | `(cd linux && uv run --locked pytest -q tests/test_provider_catalog.py tests/test_provider_workspace.py)` |
| Recording, cleanup or delivery | `(cd linux && uv run --locked pytest -q tests/test_app_capture.py tests/test_workflow.py tests/test_segment_cleanup.py tests/test_delivery.py)` |
| Ctrl+P action or availability | `make linux-command-test` (real native editor, stale actions, lossless Copy/Save, dismissal and keyboard navigation) |
| Live editor or finalization | `make linux-live-rewrite-test` (manual edits, late results, final transcript and clipboard gates) |
| Installed launch or shortcut registration | `(cd linux && uv run --locked pytest -q tests/test_launcher.py tests/test_global_shortcuts.py)` followed by `make linux-shortcut-test` |

Run the complete Linux gate at handoff; it retains every deterministic case plus lint, formatting and generated-feature consistency:

```bash
make linux-test
make linux-shortcut-test
make linux-overlay-test
make linux-text-target-test
shellcheck linux/*.sh linux/tests/*.sh scripts/*.sh dev/*.sh linux/mluva-shell
```

On macOS, `swift test` reuses the local build. For the complete CI-equivalent test command:

```bash
swift test --disable-dependency-cache
```

For macOS packaging changes, use `make ci-package` or the signing path in the [macOS guide](docs/macos-development.md). `scripts/smoke-test.sh` also launches and shuts down the app; run it only in a dedicated desktop session.

Changes to visible Linux UI or the GNOME extension also require isolated virtual-display screenshots and pixel inspection. Changes to focus capture or delivery require the private cross-process native text-target smoke test. Never use a contributor's active desktop as an automated test surface.

Use `PYTEST_ADDOPTS=--durations=20 make linux-test` to identify slow cases before changing coverage. The shared `linux/tests/http_fixture.py` owns loopback HTTP cleanup; provider handlers and assertions stay in their test modules. Measured timings, worked change examples and remaining recommendations are in the [focused development review](docs/development-review.md). Hosted CI is currently dispatch-only; report actual local/hosted results instead of assuming a PR ran it.

## Pull requests

Keep each pull request narrow, explain the user-visible outcome and failure behavior, include focused tests, and update the product or platform contract when behavior changes. Never include credentials, real transcripts, real recordings, private application or window names, or screenshots containing user data. Public claims about compatibility, privacy, speed, or accuracy require reproducible evidence.
