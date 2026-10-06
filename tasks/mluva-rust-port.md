# Rust port — delivered under functional acceptance

Daniel authorized the Rust cutover on 4 October 2026 and clarified on 6 October that acceptance means preserved main functionality and diagnostics, with small UX differences allowed. Pixel-perfect comparisons and exhaustive Python equivalence are not required. The active guide is the [main functionality checklist](../docs/functional-checklist.md); the original plan below is historical and must not drive further automatic continuation.

## Current completion

- [x] Ship native Rust application, workers, build/test/install tooling and runtime packages without Python. Retain three managed models with Qwen3-ASR 1.7B as the default.
- [x] Verify core recording/delivery, edits/rewrite/Command, saved data/diagnostics, screenshots/narration and privacy through existing native checks and bounded isolated evidence. Reuse unchanged passing evidence rather than repeat full GUI comparisons.
- [x] Release and reinstall matching app and widget 2.2.2, preserving settings, History, retained audio and models. Installed startup and public Quit pass; the current installation is resident and idle.
- [x] Review the long session against the user's intent and run focused functionality checks afterward: 69 synthetic recording/delivery/recovery/privacy workflows and 16 settings/persistence/diagnostic tests pass. The release's 142-test native gate, both strict Clippy checks, formatting and generated checks also pass.

Physical F9/F10, real microphone and optional Meeting system-audio checks remain deliberate device checks. Chromium Command capture of mixed-Unicode selections remains a known limitation, and Fedora GNOME has no recent desktop acceptance. These bounded limits and existing Experimental labels remain explicit; they do not reopen the original exhaustive parity goal.

## Original plan — historical

The remaining text records the original September plan, including its stricter unchecked requirements and former installation state. Later delivery authorizations and the current functional scope above supersede those requirements.

## Intent

Replace the Python application with a native Rust application that preserves the complete Mluva 1.6.0 user experience. Better startup, memory use and responsiveness are goals to measure; UX equivalence is required. This is a full replacement rather than a Rust wrapper around Python or a smaller application with missing features.

Scope change authorized by Daniel on 1 October 2026: support three managed local models, Whisper Tiny, Parakeet v3 and Qwen3-ASR 1.7B. Keep Qwen 1.7B as the default. Retire Whisper Base/Small/Turbo choices through a settings migration without deleting cached files or other user data. This is the explicit exception to model-choice parity; the frozen 1.6.0 reference remains unchanged.

## Definition of Done

- [ ] Every row in [the parity matrix](../docs/rust-port-parity.md) has independent evidence against the immutable 1.6.0 reference, including the native UI and failure/cancellation paths.
- [ ] All existing speech/rewrite routes, the three retained app-managed local models and their GPU choices work through native implementations without Python workers or a paid fallback.
- [ ] The native GTK/Libadwaita workspace, recording display, editor interactions, shortcuts, theme, window sizes and accessibility match the reference. Existing QML and other non-Python assets can remain.
- [ ] Existing configuration, personalization, prompts, SQLite history, recordings, screenshots and recovery state open without losing data or changing user choices. Installation preserves managed credentials and desktop integration.
- [ ] The maintained source, tests, build/install tooling and distributed package contain no Python implementation, helper, worker, compatibility fallback or Python runtime requirement. Development comparison material remains outside the final tree; Git history is preserved.
- [ ] Independent target applications confirm insertion behavior, including the Firefox cold-start/UTF-16 case and terminal/X11 cases. Stale, secure, ambiguous and uncertain targets retain the same safe delivery behavior.
- [ ] Incognito, retention, volatile audio, endpoint restrictions, secret redaction, Codex capability isolation and asynchronous session/revision guards pass meaningful regression checks.
- [ ] Startup, resident memory, idle CPU, interaction latency and representative capture/processing workloads are compared on the same host with the same providers/models and inputs. Report measured improvements and limitations rather than assuming a speedup from the language change.
- [ ] Rust checks, clean package installation, applicable native desktop checks and a substantial final review pass. Deliver the complete result through a draft PR with evidence and clear remaining platform/device limits.

## Hard Constraints

- Preserve the installed 1.6.0 app until the candidate reaches parity. Keep active recordings, saved work, credentials, focus, pointer, workspace and window layout safe.
- Use `/home/vecera/.claude/worktrees/mluva-rust-port`, branch `feat/rust-port`, created from remote `main`. Preserve the unrelated dirty `design/logo` checkout.
- The comparison reference is `v1.6.0`, commit `5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f`. Freeze behavior instead of changing the reference to accommodate the port.
- Preserve all existing experimental features and failure behavior. Keep Omarchy primary and retain the documented Fedora GNOME compatibility scope; do not invent additional verified platform support.
- Raw recognition remains immutable. Drafts, processed text and delivered text stay separate. Provisional or late model output cannot become final delivery after cancellation, privacy changes or manual revision.
- Capture remains private 16 kHz, signed little-endian 16-bit mono PCM. Annotation EOF/dismissal cancels without an upload. App-managed local speech never silently falls back to a paid service.
- No PyO3, embedded interpreter, Python subprocess or renamed Python worker in the final application. Native operating-system libraries and non-Python assets are allowed.
- Use disposable GUI sessions with private display/session/accessibility state and synthetic content. Do not operate the visible Linux desktop or expose host input devices. A Wayland region-picker test also needs a private PID namespace because the Omarchy script can stop other selectors.
- No secrets in output, real user recordings/screenshots in public evidence, real-content cloud uploads or spending to unblock hosted CI. No attribution or co-author additions.

## Soft Constraints

Prefer the existing GTK/Libadwaita toolkit and resource files to preserve native rendering and accessibility. Favor small explicit Rust components and meaningful external-boundary tests. Avoid speculative redesign, duplicated frameworks and tests that only mirror implementation constants.

## Human Checkpoints

Daniel authorized the full Rust rewrite and creation of this goal after merging PRs #66/#67 and publishing 1.6.0. Repository research, isolated implementation/testing, task-branch pushes and draft PR preparation can proceed. No additional kickoff approval is needed.

The future Rust merge and public release require separate authorization after the complete diff and acceptance evidence are reviewable. Local replacement also waits for the parity gate. Physical F9/F10 and real microphone checks remain documented acceptance limits until actually performed; elapsed time is not acceptance.

## Context

- [Product contract](../docs/product-contract.md), [platform profile](../docs/linux-platform-profile.md), [feature maturity](../docs/feature-maturity.md) and [contributor code map](../CONTRIBUTING.md).
- [Released baseline](https://github.com/1vecera/Mluva/releases/tag/v1.6.0), [automatic-paste evidence](../docs/verification/omarchy-shortcut-paste/README.md) and [screenshot evidence](../docs/verification/narrated-screenshots/README.md).
- The initial runtime inventory contains 80 files and 25,362 Python lines. It includes local ONNX/Qwen workers, installation/migration tools and private-session test tooling in addition to the GTK entry point; none can be overlooked at the final Python-removal gate.
- The native app and Omarchy widget were upgraded from checksum-verified release downloads to 1.6.0. Settings, saved conversations, editor preferences, F9/F10 bindings, focus, pointer and workspace were verified preserved. The installed narrated editor is the reviewed Tensaku 0.29.0 source patch.
- Release validation passed 631 Linux tests, Ruff, formatting, generated feature checks, ShellCheck, clean archives and plugin validation. The editor archive passed the real private-Wayland annotation/OCR/cancellation check with synthetic audio and a loopback provider. These are baseline results, not Rust-port results.

## Out of Scope

New features, UX redesign, new provider defaults, new operating-system support, replacing third-party desktop frameworks, rewriting repository history and changing unrelated source checkouts.

## State Persistence

Record decisions, parity evidence and remaining work in `docs/rust-port-parity.md`; keep private captures, logs, benchmarks and synthetic fixtures under this worktree's `tmp/`. The active conversation goal owns continuation. Keep all comparisons tied to the released reference and update current documentation when the complete Rust implementation replaces Python.
