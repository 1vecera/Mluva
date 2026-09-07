# Mluva on Omarchy

The optional `mluva.dictation` Quickshell plugin shows a floating widget near the bottom of the bar's display while Mluva prepares the microphone, records, and transcribes. It briefly confirms clipboard readiness or an error, then disappears. The widget passes mouse and keyboard input through to the current application and reserves no desktop space. It shows the newest words when a live preview is too long to fit. Completed notes and rewrites open at their end; scroll up to read earlier text. Rewrite controls return when dictation finishes.

Install the Linux application with `make linux-install`. The installer packages `mluva-shell` and the plugin under `~/.local/share/voice-scribe/app/quickshell/mluva.dictation`. Copy the plugin into the user plugin directory and enable it through Omarchy:

```bash
mkdir -p ~/.config/omarchy/plugins/mluva.dictation
cp ~/.local/share/voice-scribe/app/quickshell/mluva.dictation/* ~/.config/omarchy/plugins/mluva.dictation/
omarchy plugin enable mluva.dictation
```

When updating an existing plugin, copy all three files, including `RecordingOverlay.qml`, and reload the Omarchy shell using its normal plugin workflow. Mluva must already be running; the plugin never starts the application or a recording on its own. No additional credential or provider setup is needed for the widget.

The top-bar controls are left-click to start/stop dictation, right-click to cancel, and middle-click to open the latest note. These use Mluva's clipboard-only shell action. Existing keyboard bindings can call `mluva-shell record`, `mluva-shell cancel`, and `mluva-shell latest`. The widget does not grant a global shortcut or enable automatic pasting.

`mluva-shell watch` emits only the capture phase and elapsed seconds. The plugin uses `watch --overlay` to receive an additional audio level and the last 180 characters of the volatile preview. This preview is passed through a pipe to Quickshell, is not logged or stored by the plugin, and is erased on completion, watcher failure, or application exit. The shell does not receive device names, target application names, or credentials. Do not redirect the preview stream into persistent logs.

Disable with `omarchy plugin disable mluva.dictation`. Uninstalling Mluva removes its packaged bridge and plugin, while preserving the user-copied plugin and shell configuration.

## Verification

Run `make linux-test linux-shortcut-test` and the repository shell checks. `linux/tests/shell_overlay_smoke.py` runs the actual Quickshell widget and bridge against a separate synthetic publisher on a private session bus. Run it under a private Xvfb display with isolated XDG state and the private AT-SPI registry. It checks recording, processing, copied/error feedback, hidden idle state, owner loss, preview bounds, keyboard focus, and monitor fit, and retains screenshots. `linux/tests/conversation_ui_smoke.py` accepts `MLUVA_UI_SCENARIO=long-note` or `long-live` and explicit `MLUVA_UI_WIDTH`/`MLUVA_UI_HEIGHT` to verify the final line and composer visibility in the production UI.

Xvfb verifies rendering and the real process boundary. A live Hyprland recording remains the final acceptance check for layer-shell placement and physical shortcuts; no automated test should record from the user's microphone or send input to their desktop.

The panel uses Quickshell's documented [PanelWindow](https://quickshell.org/docs/v0.2.1/types/Quickshell/PanelWindow/), [input mask](https://quickshell.org/docs/types/Quickshell/QsWindow), and [layer-shell focus](https://quickshell.org/docs/v0.2.1/types/Quickshell.Wayland/WlrLayershell/) properties. The Omarchy widget interface is documented in the installed `/usr/share/omarchy/shell/plugins/bar/README.md`.
