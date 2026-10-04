# Contributing to Mluva

Mluva accepts focused changes that preserve its privacy, exact-target delivery, immutable raw-recognition, and recoverable-failure contracts. Omarchy is the primary platform and the core workflow is used daily. Fedora GNOME compatibility is retained but has not been tested in recent releases.

Contributions submitted to this repository are accepted under the repository's [Apache License 2.0](LICENSE).

## Local setup

Follow the [Linux guide](linux/README.md) for runtime dependencies and provider setup. Tests use local protocol servers, fake audio and desktop boundaries, and generated content; they must not open a microphone, inspect the live accessibility tree, alter the clipboard, inject input, or open a shortcut approval dialog.

This branch contains the in-progress [native Rust implementation](rust/README.md). Default Make build/run/test commands, shortcut checks and installation now use Rust. Remaining Python checks are available through `make linux-python-test`; the unconverted GUI targets prepare that environment through `linux-python-setup`. GUI checks use the isolated runners below. Complete application acceptance and final Python removal remain pending.

Native source builds require the repository-pinned Rust 1.95 toolchain, a C compiler, pkg-config, and development libraries for GTK 4.14+, Libadwaita 1.5+, libatspi, Fontconfig, SQLite and OpenSSL. Bash, coreutils and util-linux provide the build coordinator's filesystem/process commands. The optional WebKitGTK 6.0 renderer is loaded at runtime; its headers are unnecessary. See [native verification](rust/README.md) for additional test dependencies.

`make` or `make linux-setup` compiles and validates an optimized native bundle without installing it; compiler artifacts stay under `tmp/native-build` unless `CARGO_TARGET_DIR` is supplied. `make run` or `make linux-run` prepares a complete private runtime and launches its native app, retaining the bundle until that process exits. Quit removes the temporary runtime. Closing the window keeps the resident app running; these launch commands intentionally open the UI and belong inside the isolated runner during automated verification. [Source Make evidence](rust/mluva-gtk/tests/fixtures/source-make-evidence.md) covers real build, launch, forwarding and cleanup with Python and uv blocked.

Build a prepared native bundle without installing or launching it:

```sh
bash linux/build-native.sh "$PWD/tmp/native-bundle"
```

The output must not exist. This builds the production executables with locked dependencies and optimization, then packages their runtime resources and verified dependency notices. Build products default to `tmp/native-build`; `CARGO_TARGET_DIR`, `CARGO_HOME` and `RUSTUP_HOME` are respected, including caller-relative paths. Cargo selects its configured target directory; the package builder uses the executables beside itself. First builds need the pinned toolchain and dependency downloads, while an already populated cache supports offline builds.

With build and [runtime dependencies](linux/README.md#supported-desktop-contract) present, `make linux-install` builds a fresh private bundle and installs it through the same transaction as a prepared package. `MLUVA_INSTALL_HOME="$PWD/tmp/staged-home" bash linux/install.sh` uses a disposable prefix. Source removal, `make linux-uninstall`, also builds the native command; installed `mluva-uninstall` needs no compiler. Failed or interrupted builds preserve the existing installation. The [source-entry comparison](rust/mluva-install/tests/fixtures/source-entry-evidence.md) covers these commands and terminal authentication during legacy service migration.

The app-only source commands do not install system packages. For combined source setup, install the pinned Rust toolchain and a C compiler first, then run `bash install.sh`. Its widget ownership check builds without GTK, GLib or SQLite development libraries and runs before any system-package or app changes. After confirmation, setup requests the remaining runtime/development packages and installs the app and widget. Prepared native bundles use the same root script with runtime packages only and need no compiler or Python. [Combined source/package evidence](rust/mluva-install/tests/fixtures/source-setup-evidence.md) covers confirmation, dependency selection, ownership and failure recovery; Fedora package requests are checked without claiming actual Fedora installation.

`bash linux/install-widget.sh` builds and runs only the widget command; `--check` and `--stage /absolute/new/folder` retain the native CLI. `bash linux/build-native.sh --widget-only /absolute/new/folder` prepares that command and its assets without app development libraries, explicitly selecting the compiler's host target. `make linux-feature-maturity` and `make linux-feature-maturity-check` use the native development-only generator, requiring Cargo, a C compiler, pkg-config and SQLite development files. Remaining GUI/development-script conversion is pending.

## Code map

Use the [Rust overview](rust/README.md) for current native owners and independent comparisons. The table below maps the remaining Python implementation in [linux/mluva_linux](linux/mluva_linux), retained during acceptance. Tooling paths start at the repository root. In that implementation, feature logic connects to `app.py` for lifecycle wiring. GTK callbacks stay on the main thread; provider and audio work runs outside it.

| Change | Linux owner |
| --- | --- |
| Recording and final delivery | `app.py`: `_start_capture`, `_prepare_capture`, `_finish_recording`; `audio.py` captures PCM, `workflow.py` prepares/persists/delivers the final result |
| Speech/rewrite provider selection | `config.py` stores choices; `provider_catalog.py` describes/discovers routes; `providers.py` creates speech clients; `rewriting.py` owns rewrite client construction, model/speed policy and execution; `app.py` owns worker dispatch and UI completion gates; `provider_settings.py` renders choices |
| Live rewrite and manual edits | `live_rewrite.py` schedules/prompts; `app.py`: `_maybe_live_rewrite`, `_live_rewrite_finished`, `_save_live_draft` guard sessions/revisions; `conversation_view.py` owns editors and `conversation.py` saves working copies |
| Prompt instructions and templates | `prompt_defaults.py`: `LIVE_TEMPLATES` defines each Live mode once for validation, menus, prompts and initial structure; `prompts.py` resolves local overrides; `prompt_editor.py` is the shared native editor; `app.py` freezes recording snapshots |
| Automatic conversation titles | `title_jobs.py` owns the bounded queue, worker and main-thread commit checks; `conversation_titles.py` owns bounded prompt/title rules and SQL compare-and-set; `app.py` supplies settings and label refresh callbacks |
| Commands | `command_palette.py`: `application_commands` owns Ctrl+P actions and availability. Spoken Command mode uses `workflow.py` and `app.py`: `_accept_command_preview` |
| History, recovery and privacy | `history.py`, `conversation.py`, `scratchpad.py`, `meeting.py` own persistence; `history_view.py` renders history; `workflow.py` enforces retention/Incognito |
| Sidebar rename, merge and delete | `conversation_view.py` owns menus and inline titles; `conversation.py` joins saved chats atomically; `app.py` saves pending edits and checks active operations; `make linux-conversation-management-test` exercises the native controls at three window sizes |
| Native UI and theme | `conversation_view.py`, `markdown_view.py`, `ui.py`, `theme.py`; settings/page views are siblings; `app.py` composes them |
| Desktop integration | `global_shortcuts.py` owns portal sessions; `text_target.py` captures/restores accessible text; `terminal_target.py` revalidates exact Hyprland terminal identities; `delivery.py` inserts/copies; `shell_bridge.py` and `overlay_state.py` connect the GNOME/QML surfaces |
| Installation and development tooling | Root `install.sh` coordinates native app/widget setup; `linux/build-native.sh` prepares native bundles; `linux/install.sh`, `linux/uninstall.sh` and `linux/install-widget.sh` invoke `rust/mluva-install`; `rust/mluva-workflows/src/launch.rs` owns managed startup; `dev/run-isolated.sh` owns isolated GUI checks |

Raw recognition is immutable. Editor drafts, processed text and delivery results are separate. Live provisional text must never become final delivery; late provider output must pass session, privacy and manual-edit revision gates. A command opened on one document must not act on a newly selected document or replacement editor. Keep these guards beside the affected callbacks instead of introducing a shared state framework.

## Verification

Keep a test when it would catch an observable regression: lost text, an incorrect request, a stale result, unwanted delivery, leaked content or failed recovery. Use controlled external boundaries while running the production logic. Expected results should be independent of the implementation; avoid copying constants, searching source code for particular statements, checking unchanged input fixtures, or duplicating an existing stronger scenario. When a test claims an operation is skipped, make that operation available and observable. Generated-file drift belongs in its existing check command. Keep explicit configuration and packaging checks where they protect a privacy, security or distribution contract.

`make test` or `make linux-test` builds the native bundle, runs the Rust workspace suite, checks strict Clippy for default and widget-only configurations, checks formatting and verifies generated feature documentation. Environment-dependent checks remain explicitly ignored until invoked in their required private runners; consult the [native evidence index](rust/README.md) for the affected feature. `make linux-test-fast` selects native configuration/transcript, persistence, prompt/draft and Live-policy contracts; it is a subset of the full gate.

`make linux-shortcut-test` builds its native peer and runs the portal comparison on a private D-Bus with network/PID/device isolation; it needs bubblewrap and dbus-run-session, and opens no display. Native installation and source launch changes use the [combined setup](rust/mluva-install/tests/fixtures/source-setup-evidence.md), [source transactions](rust/mluva-install/tests/fixtures/source-entry-evidence.md), [source Make](rust/mluva-gtk/tests/fixtures/source-make-evidence.md) and [managed startup](rust/mluva-gtk/tests/fixtures/launcher-evidence.md) comparisons.

The following checks still exercise Python during the port. Their Make targets prepare that environment automatically; run `make linux-python-setup` before a direct `uv` command:

| Remaining Python owner | Focused check |
| --- | --- |
| Provider route or catalog | `(cd linux && uv run --locked pytest -q tests/test_provider_catalog.py tests/test_provider_workspace.py)` |
| Rewrite policy or title lifecycle | `(cd linux && uv run --locked pytest -q tests/test_rewriting.py tests/test_title_jobs.py tests/test_conversation_titles.py)`; `make linux-conversation-test` exercises the native completion gates |
| Codex capability restrictions | `make linux-codex-isolation-test` exercises the installed CLI against a loopback model fixture, including an unsolicited command, inherited MCP and global instructions; it does not use a real account or provider |
| Recording, cleanup or delivery | `(cd linux && uv run --locked pytest -q tests/test_app_capture.py tests/test_workflow.py tests/test_segment_cleanup.py tests/test_delivery.py)` |
| Prompt configuration or editor | `make linux-prompt-test` (hover/focus, Ctrl+P deep links, local files, Save/Cancel/reset, restart and recording snapshots) |
| Ctrl+P action or availability | `make linux-command-test` (real native editor, stale actions, lossless Copy/Save, dismissal and keyboard navigation) |
| Live editor or finalization | `make linux-live-rewrite-test` (manual edits, late results, final transcript and clipboard gates) |
| Continue recording, Live activation or thinking controls | `make linux-continuation-test` (real capture lifecycle with fake device/provider boundaries, edits, cancellation, persistence and narrow layout) |
| Conversation layout | `make linux-compact-workspace-test` (minimum, narrow, wide and 360-pixel tiles at 2× scaling; checks visible control bounds in saved, empty, rewrite and recording states) |
| Grilling, live navigation or Mermaid | `make linux-fluid-workspace-test` (mid-recording controls, paused draft recovery, questions, offline diagrams and responsive layouts; requires WebKitGTK 6.0) |
| Omarchy widget | `make linux-omarchy-test` (production QML and bridge on a private display/bus) |
| Shortcut or manual credential helper | `(cd linux && uv run --locked pytest -q tests/test_launcher.py tests/test_global_shortcuts.py)`; the native portal owner additionally uses `make linux-shortcut-test` |

The remaining Python rewrite-policy and title-job checks also run without GTK on a non-Linux development host:

```bash
uv run --no-project --with pytest==9.1.1 pytest -q \
  linux/tests/test_rewriting.py linux/tests/test_title_jobs.py linux/tests/test_conversation_titles.py
```

Run both native and remaining Python gates before submitting a change, plus affected private integration checks. The on-demand CI job uses these same Make targets; changing its dependencies does not establish a successful hosted or clean-distribution run.

```bash
make linux-test
make linux-shortcut-test
make linux-python-test
shellcheck install.sh linux/*.sh linux/tests/*.sh dev/*.sh linux/mluva-shell
```

For desktop delivery changes, also run `make linux-text-target-test`. Widget changes use `make linux-omarchy-test` on an Omarchy development machine. GNOME-extension changes use `make linux-overlay-test` in an environment with GNOME Shell’s headless test tools; that compatibility check is not required for unrelated Omarchy work.

The [architecture review](docs/architecture-review.md) records the scope, measured baseline, three implemented improvements and 20 options for faster feature development. The remaining recommendations are proposals; the code map above describes the current implementation.

## Pull requests

Keep each pull request narrow, explain the user-visible outcome and failure behavior, include focused tests, and update the product or platform contract when behavior changes. Never include credentials, real transcripts, real recordings, private application or window names, or screenshots containing user data. Public claims about compatibility, privacy, speed, or accuracy require reproducible evidence.
