# Mluva for Linux

Mluva is a native Python, GTK 4 and Libadwaita application with PipeWire audio. **Omarchy is the primary platform**, with the dictation, rewrite and widget workflow tested end to end and used daily. Fedora GNOME compatibility is retained but has not been tested for several releases.

For the shortest path, use the [combined installer or agent prompt](../README.md#install). The [Omarchy guide](../docs/omarchy-integration.md) explains the widget; the [platform profile](../docs/linux-platform-profile.md) describes desktop differences.

## Daily workflow

Press **F9** to start and stop dictation. Completed text copies automatically and becomes a conversation. Use **Polish**, **Structure**, a saved prompt or a custom instruction to rewrite it. Originals and completed replies are editable; **Ctrl+S** saves and **Ctrl+Enter** sends a rewrite. Raw recognition remains available separately in History. A bare recording light sits above the text at left, with elapsed time at right. The app and widget use the desktop monospace font. Drag the Omarchy recorder by its status row or choose a lower-left, bottom or lower-right preset in Settings. Focus it and press **Super+T** to switch between floating and tiling.

**Ctrl+P** searches commands, every settings control, and each prompt by name. Use **Settings → Prompts** or a prompt’s hover/focus settings button to edit its local Markdown override. See the [prompt editor guide](../docs/prompt-editor.md). The history sidebar starts hidden; reveal it to switch between saved and live conversations without interrupting recording. Dates use short weekday/month names and a configurable 12/24-hour clock. **Shift+F9** requests the latest conversation through the desktop shortcut portal. Closing the window keeps Mluva available; use Quit to exit.

Settings → Workspace controls automatic copy, action icons, scrolling and the Omarchy widget's four-second dismissal. **Live rewrite** is opt-in and Experimental and can be toggled during dictation. Its default **Grilling** template keeps evolving questions above architecture notes and local Mermaid sketches. Task spec, Structured note, Polish and Custom remain available. Active drafts reconcile at Stop; paused drafts are saved with a review label. See [workspace settings](../docs/providers-and-live-rewrite.md).

## Supported desktop contract

| Requirement | Omarchy | Fedora GNOME compatibility |
| --- | --- | --- |
| Desktop | Omarchy Quattro with Hyprland 0.55+ and Quickshell 0.3+ | Last accepted on Fedora 44 / GNOME 50; recent releases unverified |
| Native UI | GTK 4, Libadwaita, distribution PyGObject and Cairo bindings | Same |
| Audio | PipeWire, `pw-record`, `pw-dump` | Same |
| Shortcuts | XDG Global Shortcuts portal and the desktop backend | Same |
| Clipboard | `wl-copy` on Wayland | Same |
| Python | Distribution Python 3.12+ and `uv` | Same |

`bash install.sh` at the repository root installs the required packages, native app and, on Omarchy, the plugin. `--app-only` skips plugin changes. For manual package management:

```sh
# Omarchy
omarchy pkg add git uv python python-gobject python-cairo gtk4 libadwaita \
  at-spi2-core gobject-introspection dbus pipewire pipewire-audio wl-clipboard procps-ng webkitgtk-6.0

# Fedora GNOME compatibility
sudo dnf install git uv python3-gobject gtk4 libadwaita at-spi2-core \
  gobject-introspection dbus-daemon pipewire-utils wl-clipboard procps-ng webkitgtk6.0
```

X11 clipboard delivery needs `xclip`; its optional keyboard fallback needs `xdotool`. The Omarchy widget requires the existing Omarchy shell and plugin manager; setup does not install an operating system or replace desktop configuration.

## Install for the current user

With dependencies already present, install only the native app:

```sh
make linux-install
```

Application files go under `~/.local/share/mluva/app`, commands under `~/.local/bin`, and the desktop entry under the XDG applications directory. The installer uses distribution GTK bindings and the locked Python dependencies. It refuses an active app or unrecognized installation, preserves the previous app until setup succeeds, and rolls back on failure. [Upgrades from 0.x](../docs/identity-migration.md) preserve settings and saved work.

The app requests approval for recording, cancellation and opening the latest conversation on first launch. Settings shows the keys actually approved by the desktop. The default recording key is F9; alternatives range from F1 to F24. Changing a key replaces the portal session. Right Alt/AltGr remains available to the keyboard layout.

To verify the package in a disposable prefix without changing the active installation:

```sh
MLUVA_INSTALL_HOME="$PWD/tmp/staged-home" bash linux/install.sh
```

Staged mode skips live desktop, systemd and credential integration. Use the [isolated runner](../dev/README.md) for any GUI launch from that prefix.

Run `mluva-uninstall` to remove the native app and owned launch integrations. Settings, history, recordings and migration backups remain. On Omarchy, remove the separately managed widget with `omarchy plugin remove mluva.dictation` before uninstalling the app.

## Provider setup

Speech recognition and rewriting are selected independently in **Settings → Providers**. The defaults are ElevenLabs Scribe and an authenticated Codex CLI. Alternatives include local Voxtype/Whisper and compatible speech or rewrite APIs. Install any required provider client or model, and connect the selected account before recording. [Choose providers](../docs/provider-selection.md).

Supply credentials to the app process through your secret manager or desktop launch environment. Settings stores key-variable names, never key values. Restart Mluva after changing that environment. A cloud route may send audio or text off the device; local recognition alone does not make rewrites or generated titles local.

The launcher also supports an existing managed credential profile. That optional integration is implemented in `resources/mluva.in` and `configure-secret-profile.sh`; ordinary installations do not require it.

## Optional GNOME integrations

These Fedora compatibility paths have not been tested in recent releases. Clipboard delivery remains the standard workflow.

`mluva-overlay install` enables the optional GNOME Shell menu and display-only recording bar. A newly installed extension may need one logout/login before GNOME discovers it. The app's in-window recording display is available without the extension. Remove it with `mluva-overlay remove`.

Automatic insertion is Experimental and disabled by default. Native AT-SPI editing needs accessibility enabled before the app starts. On GNOME, check `gsettings get org.gnome.desktop.interface toolkit-accessibility`; enable it through desktop settings or `gsettings set org.gnome.desktop.interface toolkit-accessibility true`, then restart applications that do not expose text targets.

For targets without native accessibility editing, the optional `ydotool` helper can supply a keyboard-only fallback. Install the distribution's `ydotool` package, then explicitly run `mluva-input-helper install`. This uses sudo to install a system service and gives processes running as your user access to its private synthetic-keyboard socket. Remove it with `mluva-input-helper remove`. Ordinary installation does not enable it.

## Data and troubleshooting

Settings normally live in `~/.config/mluva`, persistent data in `~/.local/share/mluva`, and temporary state in the XDG runtime directory. The [product contract](../docs/product-contract.md) covers privacy, recovery and retention.

If capture fails, check the input and provider status in Settings. If the widget reports that Mluva is unavailable, start the app and check that its `mluva-shell` command is reachable; see the [Omarchy guide](../docs/omarchy-integration.md#installation). A failed or uncertain insertion leaves the text available for Copy rather than retrying into an unknown target.

## Development

`make linux-setup` creates the locked environment with system GTK bindings. `make linux-run` starts the app from source; use it only when you intend to open the UI. `make linux-test` runs the deterministic suite, lint, formatting and feature-matrix check. The [contributor guide](../CONTRIBUTING.md#verification) lists focused and isolated integration checks.
