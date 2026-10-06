# Contributing to Mluva

Mluva uses Rust, GTK4/Libadwaita and PipeWire. Preserve privacy, exact-target delivery, immutable raw recognition and recoverable failures. Omarchy is the primary platform; Fedora GNOME compatibility remains without recent desktop acceptance. Contributions use the [Apache License 2.0](LICENSE).

## Local setup

Install the repository-pinned Rust 1.95 toolchain, a C compiler, pkg-config and development libraries for GTK 4.14+, Libadwaita 1.5+, libatspi, Fontconfig, SQLite and OpenSSL. Bash, coreutils and util-linux coordinate builds. WebKitGTK 6.0 loads only when diagrams need it. See the [Linux runtime guide](linux/README.md) and [native verification](rust/README.md) for additional test dependencies. Python and uv are not required.

`make` builds and validates an optimized native bundle. Compiler artifacts stay in `tmp/native-build` unless `CARGO_TARGET_DIR` is set. `make run` prepares a temporary bundle and launches the app; Quit removes it, while closing the window keeps the resident app running. Automated launch checks must use the private desktop runner.

```sh
bash linux/build-native.sh "$PWD/tmp/native-bundle"
```

The output must not exist. Locked Cargo dependencies, runtime resources and verified dependency notices form the bundle. `CARGO_TARGET_DIR`, `CARGO_HOME` and `RUSTUP_HOME` are respected, including relative locations. A populated dependency cache supports offline builds.

With runtime dependencies present, `make linux-install` builds and installs through the same native transaction as a prepared package. Use `MLUVA_INSTALL_HOME="$PWD/tmp/staged-home" bash linux/install.sh` for disposable-prefix verification. `make linux-uninstall` builds the native removal command; installed `mluva-uninstall` needs no compiler. Existing settings, History, drafts, models and credentials are preserved during upgrades.

Combined `bash install.sh` provisions desktop dependencies, installs the app and updates the Omarchy widget; `--app-only` skips the widget. Source setup requires Rust and a C compiler first. Prepared bundles require neither compiler nor Python. Widget ownership is checked before package or application changes. `bash linux/install-widget.sh --check` runs native widget preflight from source.

## Code map

GTK callbacks stay on the main thread; provider/audio work runs outside it. The [native overview](rust/README.md) links independent released comparisons and their exact scopes.

| Change | Owner |
| --- | --- |
| Configuration, prompts, personalization, History, conversations, retention | `rust/mluva-core/src` |
| PCM/WAV, private audio, PipeWire and crash cleanup | `rust/mluva-audio/src` |
| Speech/rewrite connections, credentials, local model downloads and previews | `rust/mluva-providers/src` |
| Native model execution and recognition worker | `rust/mluva-asr/src` |
| Recording, delivery, Live/review, titles, meetings, screenshots and narration | `rust/mluva-workflows/src` |
| GTK application, pages, settings, rendering and text-target tracking | `rust/mluva-gtk/src` |
| D-Bus action/status bridge | `rust/mluva-shell/src` |
| Installation, identity migration, package/archive, widget and editor launch | `rust/mluva-install/src` |
| Omarchy widget / GNOME recording display | `linux/quickshell/mluva.dictation` / `linux/gnome-extension` |

## Verification

Tests use synthetic content and local protocol peers. Never open a real microphone, inspect the live accessibility tree, change the host clipboard, inject host input or open host permission dialogs. GUI checks use `dev/run-isolated-browser.sh` with private display, HOME/XDG, D-Bus, accessibility, network and device isolation. See [dev/README.md](dev/README.md) for prerequisites. Image comparisons also require ImageMagick; Mermaid checks require WebKitGTK 6.0.

```sh
make linux-test
make linux-shortcut-test
shellcheck install.sh linux/*.sh linux/tests/*.sh dev/*.sh linux/mluva-shell
```

The on-demand CI job runs the native gate; local success does not establish a hosted or clean-distribution result. Do not spend to unblock CI.

| Changed boundary | Private integration check |
| --- | --- |
| Full recording/application and desktop actions | `make linux-application-test` |
| Continued recording and recorder review transition | `make linux-continuation-test` |
| Conversation editing, navigation and scrolling | `make linux-conversation-test` |
| Live workspace and controllers | `make linux-live-workspace-test linux-live-rewrite-test` |
| Provider preferences and held discovery | `make linux-provider-settings-test` |
| Conversation rename, merge, privacy and deletion | `make linux-conversation-management-test` |
| Compact layouts and finalization | `make linux-compact-workspace-test` |
| Prompt editor, persistence and process restart | `make linux-prompt-test` |
| First-run model download/readiness and opt-ins | `MLUVA_TEST_ONBOARDING_ASSETS=/absolute/public-fixtures make linux-onboarding-test` |
| Screenshot context and real editor narration | `MLUVA_TEST_EDITOR=/absolute/verified/tensaku make linux-screenshot-test` |
| Exact-target paste / widget | `make linux-text-target-test` / `make linux-omarchy-test` |
| Installed Codex isolation | `make linux-codex-isolation-test` |
| GNOME display extension | `make linux-overlay-test` in GNOME's headless test environment |

Remaining physical-device, desktop permission and platform limits are recorded in the [2.0.0 cutover evidence](docs/verification/rust-cutover/README.md). Historical Python comparison and artwork recipes are available from v1.6.0 and are not maintained or shipped in 2.0.0.

## Pull requests

Describe the user-visible outcome and failure behavior, include focused verification and update changed product/platform contracts. Do not include credentials, real transcripts, recordings, private window names or user screenshots. Claims about compatibility, privacy, speed and accuracy require reproducible evidence.
