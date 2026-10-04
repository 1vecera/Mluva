# Mluva 2.0.0 native cutover

Daniel authorized merging the Rust port, removing Python, publishing a release and installing it on this machine on 4 October 2026, accepting the remaining desktop acceptance risk. This authorizes delivery; it does not establish complete one-to-one acceptance.

The application, recognition worker, audio cleanup, desktop bridge, screenshot narration, editor launcher, installer, uninstaller, widget installer, package builder and feature-maturity generator are Rust. GTK4/Libadwaita, QML, the GNOME JavaScript extension and the existing Bash setup commands remain. The default local model is Qwen3-ASR 1.7B; the other choices are Parakeet v3 and Whisper Tiny.

## Retired implementation and verification

The immutable reference is v1.6.0, commit `5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f`. Its complete original 631-test suite passed in a private network, PID namespace, HOME and XDG state before this cutover. Reference sources and comparison scripts remain outside the maintained tree, in ignored scratch storage and the previous release. The final tree contains no Python modules, Python development environment or Python CI gate.

This is the requested implementation cutover, not a completed test-pruning audit. The former standalone GUI helpers have not all been individually reverified. No fixture is regenerated from the implementation under test and no native scenario is removed. The remaining native proof is owned at these boundaries:

| Retired Python layer | Native keeper |
| --- | --- |
| Configuration, normalization, prompts, personalization, drafts and text diffs | `mluva-core`: `reference_contracts`, `prompt_and_draft_contracts`, `personalization_contracts` |
| SQLite History, conversation search/titles/continuations, screenshots and retention | `mluva-core`: `persistence_contracts`, `storage_lifecycle`, `meeting` |
| Recording, private audio, cleanup and PCM/WAV handling | `mluva-audio` integration tests; `mluva-providers/native_capture`; `mluva-workflows` recording/meeting/cleanup contracts |
| Credentials, Codex, speech/rewrite providers, local downloads and inference | `mluva-providers` integration tests and `mluva-asr` real inference comparisons |
| Live modes, session/revision guards, rewriting, annotations and application lifecycle | `mluva-workflows` contracts and `mluva-gtk` application/controller integration tests |
| GTK pages, preferences, editing, scroll stability and diagrams | `mluva-gtk` private desktop comparisons, including `conversation_page`, `document_surfaces`, History and Meeting pages |
| Exact-target delivery, clipboard, terminal targets and global shortcuts | `mluva-core/delivery_contracts`, `mluva-gtk/text_target`, recording-target and portal checks |
| D-Bus actions/status and Omarchy widget | `mluva-shell` process comparisons, native application-shell and widget checks |
| Setup, identity migration, installation, upgrade, editor activation and distribution | `mluva-install` process, migration, package/archive and source-entry tests |

`make linux-conversation-test` now runs the existing native conversation workspace and application-shell owners in the private desktop runner. The obsolete Python identity scanner and preview benchmark are retired with their implementation; migration/package tests and native local-preview/inference owners remain. Historical Python branding, Blender and video preparation recipes are removed; their committed assets and project files remain, and the recipes can be recovered from v1.6.0. They are not runtime features.

## Acceptance limits

Physical F9/F10, the actual microphone, simultaneous real-microphone recording and the live desktop permission/paste flow remain unverified. Fedora GNOME compatibility has not received recent desktop acceptance. The measured native startup/idle improvement is limited to the documented isolated warm-cache benchmark; complete inference performance and every user-experience matrix row are not certified. The release retains Experimental paste and screenshot context status. Hosted CI is on demand and is not triggered or funded for this delivery.

The previous installed app, settings and History are backed up privately before replacement. The native installer validates the prepared bundle and retains settings, SQLite data, saved drafts, credentials and model caches. The widget and application share version 2.0.0.

## Native cutover gate

The actual `make linux-test` gate passes 140 native tests with zero failures and 68 environment-specific checks ignored across 98 suites. Both strict Clippy configurations, formatting, generated feature consistency and ShellCheck pass. The older-History search regression is retained at the native SQLite boundary: it searches an old source/reply behind ninety newer conversations, treats percent/underscore literally and handles case folding. Removing percent escaping caused the intended assertion failure; production bytes were restored and rebuilt, and all fourteen storage lifecycle checks pass.

Release metadata intentionally changes to 2.0.0. The frozen Codex initialization and ElevenLabs User-Agent comparisons retain every other byte and now independently require the current Cargo release version. Existing reference JSON files are unchanged. Widget compatibility cases continue to use explicit v1.6.0 inputs; actual native package/setup checks require the current release version.
