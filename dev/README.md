# Development tools

Prepare the native Python environment with `make linux-setup`. The [contributor guide](../CONTRIBUTING.md) maps features to code and focused checks.

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

For a custom fixture:

```sh
bash dev/run-isolated.sh tmp/my-scenario -- \
  env PYTHONPATH=linux GTK_A11Y=none GSK_RENDERER=cairo \
  uv run --project linux --locked python path/to/fixture.py
```

Use `OFFSCREEN_ENABLE_ATSPI=1` for a fixture that exercises accessibility. Run virtual-display tests sequentially, or assign distinct unused `OFFSCREEN_DISPLAY_NUMBER` values and separate output directories. Keep external providers, microphones, input devices and credentials disabled unless the fixture explicitly needs them.

## Unified release package

The native app and widget ship from this repository in one source archive and use the same release version. The root `manifest.json` is the sole plugin manifest and points directly into `linux/quickshell/mluva.dictation`. Omarchy can clone this repository as a plugin; the native app still needs explicit setup from that checkout. No plugin mirror, separate export or second website is needed.

Omarchy's current plugin manager uses a full Git clone, including this project's older design and video history. On 29 September 2026 the checked-out source occupied about 29 MiB and local Git packs about 361 MiB; actual download size varies. The documented shallow clone or release archive with combined setup remains the smaller download. Keeping one repository avoids a publishing mirror; reducing historical clone size would require a separate, explicitly planned history migration.

`linux/install_widget.py --stage <new-folder>` copies the QML, JavaScript and fonts and derives a folder-relative manifest from the root manifest. The native app installer uses that same staging operation. The combined installer then loads the staged widget through a content-specific entry point, validates it with Omarchy and enables the stable `mluva.dictation` ID. This preserves the existing bundled upgrade and rollback behavior while keeping manifest metadata in one place.

`bash install.sh` installs both parts. Installer tests cover fresh installs, upgrades, clean legacy Git migration without network access, protected local edits and rollback after shell failures. Validate a clean checkout or source archive with `omarchy plugin validate <repository>` and the staged widget with the same command. Use a clean tree because Omarchy rejects symlinks, including those in a development virtual environment. Run `make linux-test` and the isolated widget check before releasing. To build the package from a reviewed tag:

```sh
git archive --format=tar.gz --prefix=mluva-1.5.3/ \
  --output=tmp/mluva-1.5.3-source.tar.gz v1.5.3
```

Publish that archive and its SHA-256 checksum together on the main Mluva release. Re-running setup from the new archive updates app and widget together.
