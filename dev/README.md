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

The native app and widget ship from this repository in one source archive and use the same release version. `linux/quickshell/mluva.dictation` is the widget source; `linux/install_widget.py` installs those exact QML, JavaScript and font files through a versioned entry point, validates them with Omarchy and enables the stable `mluva.dictation` ID. No plugin mirror or separate export is needed.

`bash install.sh` installs both parts. Installer tests cover fresh installs, upgrades, clean legacy Git migration without network access, protected local edits and rollback after shell failures. Run `make linux-test` and the isolated widget check before releasing. To build the package from a reviewed tag:

```sh
git archive --format=tar.gz --prefix=mluva-1.5.2/ \
  --output=tmp/mluva-1.5.2-source.tar.gz v1.5.2
```

Publish that archive and its SHA-256 checksum together on the main Mluva release. Re-running setup from the new archive updates app and widget together.
