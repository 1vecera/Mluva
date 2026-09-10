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

## Plugin distribution

The installable [Omarchy plugin repository](https://github.com/1vecera/omarchy-mluva) mirrors `linux/quickshell/mluva.dictation`. Export the reviewed checkout by its full commit ID:

```sh
uv run --no-project dev/export_plugin.py tmp/plugin-release \
  --commit "$(git rev-parse HEAD)" --preview docs/promotion/assets/widget-review.png
```

Use `--release` with an explicit release tag when exporting a tagged version. The export contains that revision's QML and manifest, license, current README template and `SOURCE.json` with the source commit, optional release tag and file hashes. Review the guide against the selected source before publishing an update. Edit `dev/plugin-README.md` when the installation or usage guide changes.
