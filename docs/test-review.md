# Test review

This is a historical review. Its counts and host limitations describe the earlier change, not the current suite. See the [focused development review](development-review.md) for the S27-468 measurements and current check results.

The suite does not support a dramatic count reduction without losing distinct failure checks. This review removes 20 Linux cases and four Swift tests, reducing Linux from 302 to 282 collected cases and Swift from 296 to 292 declared tests. The test files lose 344 net lines. Parameterized input cases remain separate; regrouping them into loops would only disguise their count.

## Changes

| Area | Change and retained evidence |
|---|---|
| Installation | Delete seven source-string checks. Exercise the installed uninstaller through its symlink, verify actual installed aliases and desktop metadata, and require production-only dependency setup in the existing staged upgrade test. Keep rollback, unrelated-path protection, and credential isolation tests. |
| Shortcuts | Replace three overlapping session tests with one running-service lifecycle that checks the application ID, all three bindings, F9 → F24 replacement, actual approvals, and closure of both sessions. Keep the independent private D-Bus smoke. |
| History and diagnostics | Combine persistence/export/delete assertions into one history lifecycle and retain configuration redaction in the full diagnostics export test. Scope SQLite connection spies to their respective modules so they do not replace coverage's own SQLite connection. |
| Recording and UI | Remove copied constants, exact prose snapshots, and a JavaScript source scan. Preserve terminal erasure through the application cleanup path and invalid-state normalization. Keep real rendered UI and shell lifecycle fixtures. |
| Dictation | Remove one duplicate raw/prepared-text workflow; personalization delivery and transcript transformation tests retain the behavior. |
| Swift | Remove three CoreGraphics literal/API checks that never call Mluva and an audio test identical to the existing initial-state test. No Swift production code changes. |

## Review scope

All 30 Python test modules and 38 Swift test files were reviewed for assertion meaning, duplication, and the failures they distinguish. Runtime verification was performed on Linux. Swift findings are from source inspection; macOS execution is unavailable on this host.

| Suites reviewed | Decision |
|---|---|
| Linux audio, PipeWire, ElevenLabs, realtime, segment cleanup, Codex client, managed agent launcher | Retain process and protocol tests: readiness, ordered results, bounded queues, timeouts, cancellation, restart, and sanitized failures differ even when they execute shared paths. |
| Linux delivery, text target, workflow, app capture, conversation, scratchpad, meeting | Retain capture/delivery boundaries, recovery, Incognito, exact-once dispatch, editable review, and concurrency checks. Apply only the duplicate/UI-snapshot reductions above. |
| Linux config, history, diagnostics, personalization, vocabulary, transcript | Retain migration, malformed-input, permissions, protected-token, correction-provenance, and destructive-path checks. Combine only overlapping persistence/export setup. |
| Linux global shortcuts, shell bridge, overlay state, recording bar, launcher, input helper, recording overlay helper | Keep behavioral and subprocess checks. Replace copied implementation text with existing runtime evidence. |
| Linux brand assets, release metadata, feature maturity | Keep generated-asset consistency, cross-platform versioning, dependency notices, license metadata, and conservative feature claims; remove the copied name/descriptor literals. |
| Swift recognition, Google authentication and transport, Apple providers, Gemini rewriting, enhancement, context, transcript, and chunking | Keep protocol ordering, consent, fallback, protected content, readiness, and long-stream checks. |
| Swift recording controllers, delivery, clipboard, keyboard, audio, permissions, hotkeys | Keep independent state transitions and race checks. Remove only the four checks described above. Some permission tests depend on host TCC state and some smoke assertions are weak; replacing them requires deterministic macOS fixtures, not deletion of the permission boundary. |
| Swift settings, personalization, history, audio retention, scratchpad, meeting, diagnostics | Keep persistence, opt-in defaults, traversal protection, corruption preservation, review-before-delivery, and privacy boundaries. Combining default-value tests would mostly change reporting granularity. |

## Verification

`make linux-test linux-shortcut-test` passes with 282 tests, Ruff lint/format, generated-feature consistency, and the private D-Bus registration/binding/lifecycle smoke. ShellCheck passes for the repository shell scripts and bridge launcher. The earlier widget and note-tail checks use real Quickshell and GTK processes at minimum, narrow, and wide sizes; rendered screenshots were inspected.

Coverage is supporting evidence, not a deletion rule. Before/after instrumentation covered 4,377/4,374 of 8,380 Python lines and 1,010/1,007 of 2,252 branches. One removed line is the UI-description wrapper formerly exercised by prose snapshots. The other two lines and three branches are thread-scheduling variation in the unchanged realtime provider-error test. No provider, recovery, or delivery tests were removed on coverage overlap alone.

Three temporary defects were each rejected by retained tests: showing the preview head instead of its tail, writing Incognito history, and dispatching paste twice. Production files were restored before the final full verification.

Hosted CI is waived for billing under repository policy. Swift tests and the GNOME extension runtime could not run here. The private AT-SPI smoke previously confirmed Unicode insertion and caret position but emitted a startup cache warning. Physical recording shortcuts and Wayland layer placement remain manual acceptance checks on Hyprland.
