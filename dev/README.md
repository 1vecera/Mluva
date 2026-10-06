# Development tools

The [contributor guide](../CONTRIBUTING.md) covers native Rust builds and focused checks. `make` builds a native bundle; `make run` launches it; `make test` checks the native workspace. `make linux-shortcut-test` runs the native portal comparison without a display. Maintained build, test and install tooling uses Rust and shell, with no Python runtime dependency. The [functional checklist](../docs/functional-checklist.md) defines current release acceptance.

## Isolated UI verification

`run-isolated.sh` starts a virtual X11 display with a private session bus and XDG state. It leaves the active desktop alone. Install Xvfb, `xvfb-run`, `xauth`, D-Bus and the normal GTK runtime before using it. Native accessibility checks also need AT-SPI; input scenarios use `xdotool` only inside the virtual display.

```sh
make linux-command-test
make linux-omarchy-test
make linux-conversation-test
make linux-live-rewrite-test
make linux-provider-settings-test
```

The fixtures use the production application with synthetic audio or model boundaries. Screenshots and logs stay under this checkout's `tmp/`. They exercise rendering, editing and process communication; physical microphone quality and Wayland permissions require the actual target desktop.

For a custom native fixture, build it before starting the private desktop and run its executable through the isolated runner:

```sh
bwrap --die-with-parent --bind / / --dev /dev --tmpfs /run/dbus -- \
  bash dev/run-isolated-browser.sh "$PWD/tmp/my-scenario" -- \
  /absolute/path/to/native-fixture
```

Use `OFFSCREEN_ENABLE_ATSPI=1` for a fixture that exercises accessibility. Run virtual-display tests sequentially, or assign distinct unused `OFFSCREEN_DISPLAY_NUMBER` values and separate output directories. Keep external providers, microphones, input devices and credentials disabled unless the fixture explicitly needs them.

## Unified release package

The native app and widget share one release version and package. Build a prepared runtime bundle with `bash linux/build-native.sh /absolute/new/folder`; it contains no Python and needs no compiler to install. Published releases state their verified functional scope and platform limits. The root `manifest.json` is the sole plugin manifest and points directly into `linux/quickshell/mluva.dictation`. Omarchy can clone this repository as a plugin; the app still needs explicit setup from that checkout. No plugin mirror, separate export or second website is needed.

Omarchy's current plugin manager uses a full Git clone, including this project's older design and video history. On 29 September 2026 the checked-out source occupied about 29 MiB and local Git packs about 361 MiB; actual download size varies. The documented shallow clone or release archive with combined setup remains the smaller download. Keeping one repository avoids a publishing mirror; reducing historical clone size would require a separate, explicitly planned history migration.

`bash linux/install-widget.sh --stage /absolute/new/folder` builds the native widget command, copies the QML, JavaScript and fonts, and derives a folder-relative manifest from the root manifest. The native package builder uses the same Rust staging owner. A prepared bundle exposes `bin/mluva-install-widget` directly. The combined installer loads the staged widget through a content-specific entry point, validates it with Omarchy and enables the stable `mluva.dictation` ID, preserving upgrade/rollback behavior and one manifest source.

`bash install.sh` installs both parts. Source installation requires the pinned Rust toolchain and a C compiler before dependency provisioning. [Native setup comparisons](../rust/mluva-install/tests/fixtures/source-setup-evidence.md) cover confirmation, fresh installs, upgrades, protected local edits and rollback after shell failures. Validate a clean checkout and staged widget with `omarchy plugin validate <path>` in the isolated environment; Omarchy rejects symlinks. Run the applicable native and Linux gates before delivery. Build a source archive from the exact reviewed release commit:

```sh
git archive --format=tar.gz --prefix=mluva-2.2.2/ \
  --output=tmp/mluva-2.2.2-source.tar.gz v2.2.2
```

The native [archive builder](../rust/mluva-install/tests/fixtures/archive-evidence.md) creates runtime archives with verified dependency notices. A source archive still requires a compiler. Release approval and a verified package do not establish complete application or physical/platform acceptance.
