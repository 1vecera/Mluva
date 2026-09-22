# Contributing to Mluva

Mluva accepts focused changes that preserve its privacy, exact-target delivery, immutable raw-recognition, and recoverable-failure contracts. Omarchy is the primary platform and the core workflow is used daily. Fedora GNOME compatibility is retained but has not been tested in recent releases.

Contributions submitted to this repository are accepted under the repository's [Apache License 2.0](LICENSE).

## Local setup

Follow the [Linux guide](linux/README.md) for runtime dependencies and provider setup. Tests use local protocol servers, fake audio and desktop boundaries, and generated content; they must not open a microphone, inspect the live accessibility tree, alter the clipboard, inject input, or open a shortcut approval dialog.

Run `make linux-setup` to prepare the locked Python environment with distribution PyGObject access. GUI checks use the isolated runners below.

## Code map

Bare Python filenames refer to [linux/mluva_linux](linux/mluva_linux). Tooling paths start at the repository root. Start at the feature's logic, then follow its calls into `app.py` for lifecycle wiring. GTK callbacks stay on the main thread; provider and audio work runs outside it.

| Change | Linux owner |
| --- | --- |
| Recording and final delivery | `app.py`: `_start_capture`, `_prepare_capture`, `_finish_recording`; `audio.py` captures PCM, `workflow.py` prepares/persists/delivers the final result |
| Speech/rewrite provider selection | `config.py` stores choices; `provider_catalog.py` describes/discovers routes; `providers.py` creates speech clients; `rewriting.py` owns rewrite client construction, model/speed policy and execution; `app.py` owns worker dispatch and UI completion gates; `provider_settings.py` renders choices |
| Live rewrite and manual edits | `live_rewrite.py` schedules/prompts; `app.py`: `_maybe_live_rewrite`, `_live_rewrite_finished`, `_save_live_draft` guard sessions/revisions; `conversation_view.py` owns editors and `conversation.py` saves working copies |
| Prompt instructions and templates | `prompt_defaults.py`: `LIVE_TEMPLATES` defines each Live mode once for validation, menus, prompts and initial structure; `prompts.py` resolves local overrides; `prompt_editor.py` is the shared native editor; `app.py` freezes recording snapshots |
| Automatic conversation titles | `title_jobs.py` owns the bounded queue, worker and main-thread commit checks; `conversation_titles.py` owns bounded prompt/title rules and SQL compare-and-set; `app.py` supplies settings and label refresh callbacks |
| Commands | `command_palette.py`: `application_commands` owns Ctrl+P actions and availability. Spoken Command mode uses `workflow.py` and `app.py`: `_accept_command_preview` |
| History, recovery and privacy | `history.py`, `conversation.py`, `scratchpad.py`, `meeting.py` own persistence; `history_view.py` renders history; `workflow.py` enforces retention/Incognito |
| Native UI and theme | `conversation_view.py`, `markdown_view.py`, `ui.py`, `theme.py`; settings/page views are siblings; `app.py` composes them |
| Desktop integration | `global_shortcuts.py` owns portal sessions; `text_target.py` captures/restores exact targets; `delivery.py` inserts/copies; `shell_bridge.py` and `overlay_state.py` connect the GNOME/QML surfaces |
| Installation and development tooling | `install.sh` coordinates dependencies, the native app and the Omarchy plugin; `linux/install.sh` and `linux/resources/mluva.in` own the app installation; `dev/run-isolated.sh` owns isolated GUI checks |

Raw recognition is immutable. Editor drafts, processed text and delivery results are separate. Live provisional text must never become final delivery; late provider output must pass session, privacy and manual-edit revision gates. A command opened on one document must not act on a newly selected document or replacement editor. Keep these guards beside the affected callbacks instead of introducing a shared state framework.

## Verification

Keep a test when it would catch an observable regression: lost text, an incorrect request, a stale result, unwanted delivery, leaked content or failed recovery. Use controlled external boundaries while running the production logic. Expected results should be independent of the implementation; avoid copying constants, searching source code for particular statements, checking unchanged input fixtures, or duplicating an existing stronger scenario. When a test claims an operation is skipped, make that operation available and observable. Generated-file drift belongs in its existing check command. Keep explicit configuration and packaging checks where they protect a privacy, security or distribution contract.

For a quick text/editing change, run `make linux-test-fast` from the root. It covers transcript preservation, conversation/history persistence, scratchpads, Live scheduling and Markdown round trips. It is a subset, not the complete check set. For other changes, choose the tests that exercise their boundary:

| Change | Focused check after `make linux-setup` |
| --- | --- |
| Provider route or catalog | `(cd linux && uv run --locked pytest -q tests/test_provider_catalog.py tests/test_provider_workspace.py)` |
| Rewrite policy or title lifecycle | `(cd linux && uv run --locked pytest -q tests/test_rewriting.py tests/test_title_jobs.py tests/test_conversation_titles.py)`; `make linux-conversation-test` exercises the native completion gates |
| Codex capability restrictions | `make linux-codex-isolation-test` exercises the installed CLI against a loopback model fixture, including an unsolicited command, inherited MCP and global instructions; it does not use a real account or provider |
| Recording, cleanup or delivery | `(cd linux && uv run --locked pytest -q tests/test_app_capture.py tests/test_workflow.py tests/test_segment_cleanup.py tests/test_delivery.py)` |
| Prompt configuration or editor | `make linux-prompt-test` (hover/focus, Ctrl+P deep links, local files, Save/Cancel/reset, restart and recording snapshots) |
| Ctrl+P action or availability | `make linux-command-test` (real native editor, stale actions, lossless Copy/Save, dismissal and keyboard navigation) |
| Live editor or finalization | `make linux-live-rewrite-test` (manual edits, late results, final transcript and clipboard gates) |
| Continue recording, Live activation or thinking controls | `make linux-continuation-test` (real capture lifecycle with fake device/provider boundaries, edits, cancellation, persistence and narrow layout) |
| Grilling, live navigation or Mermaid | `make linux-fluid-workspace-test` (mid-recording controls, paused draft recovery, questions, offline diagrams and responsive layouts; requires WebKitGTK 6.0) |
| Omarchy widget | `make linux-omarchy-test` (production QML and bridge on a private display/bus) |
| Installed launch or shortcut registration | `(cd linux && uv run --locked pytest -q tests/test_launcher.py tests/test_global_shortcuts.py)` followed by `make linux-shortcut-test` |

Rewrite policy and title jobs also run without GTK on a non-Linux development host:

```bash
uv run --no-project --with pytest==9.1.1 pytest -q \
  linux/tests/test_rewriting.py linux/tests/test_title_jobs.py linux/tests/test_conversation_titles.py
```

Run the complete Linux gate before submitting a change; it retains every deterministic case plus lint, formatting and generated-feature consistency:

```bash
make linux-test
make linux-shortcut-test
shellcheck install.sh linux/*.sh linux/tests/*.sh dev/*.sh linux/mluva-shell
```

For desktop delivery changes, also run `make linux-text-target-test`. Widget changes use `make linux-omarchy-test` on an Omarchy development machine. GNOME-extension changes use `make linux-overlay-test` in an environment with GNOME Shell’s headless test tools; that compatibility check is not required for unrelated Omarchy work.

The [architecture review](docs/architecture-review.md) records the scope, measured baseline, three implemented improvements and 20 options for faster feature development. The remaining recommendations are proposals; the code map above describes the current implementation.

## Pull requests

Keep each pull request narrow, explain the user-visible outcome and failure behavior, include focused tests, and update the product or platform contract when behavior changes. Never include credentials, real transcripts, real recordings, private application or window names, or screenshots containing user data. Public claims about compatibility, privacy, speed, or accuracy require reproducible evidence.
