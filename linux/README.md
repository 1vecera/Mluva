# Mluva for Linux

Released Mluva 1.6.0 uses Python, GTK 4, Libadwaita and PipeWire. This development branch contains its [native Rust replacement](../rust/README.md); app-only source installation now builds Rust, while the remaining development commands and root widget setup still use Python. Full Rust application acceptance remains pending. **Omarchy is the primary platform** for the released daily workflow. Fedora GNOME compatibility is retained but has not been tested for several releases.

For the shortest path, use the [combined installer or agent prompt](../README.md#install). The [Omarchy guide](../docs/omarchy-integration.md) explains the widget; the [platform profile](../docs/linux-platform-profile.md) describes desktop differences.

## Daily workflow

Press **F9** to start and stop dictation. Completed text copies automatically and becomes a conversation. Use **Polish**, **Structure**, a saved prompt or a custom instruction to rewrite it. Originals and completed replies are editable; **Ctrl+S** saves and **Ctrl+Enter** sends a rewrite. Raw recognition remains available separately in History. A bare recording light sits above the text at left, with elapsed time at right. The app and widget use the desktop monospace font. Drag the Omarchy recorder by its status row or choose a lower-left, bottom or lower-right preset in Settings. Focus it and press **Super+T** to switch between floating and tiling.

**Ctrl+P** searches commands, every settings control, and each prompt by name. Use **Settings → Prompts** or a prompt’s hover/focus settings button to edit its local Markdown override. See the [prompt editor guide](../docs/prompt-editor.md). The history sidebar starts hidden; reveal it to switch between saved and live conversations without interrupting recording. Dates use short weekday/month names and a configurable 12/24-hour clock. **Shift+F9** requests the latest conversation through the desktop shortcut portal. Closing the window keeps Mluva available; use Quit to exit.

Settings → Workspace controls automatic copy, action icons, scrolling and the Omarchy widget's four-second dismissal. **Live rewrite** is opt-in and Experimental and can be toggled during dictation. Its default **Grilling** template keeps evolving questions above architecture notes and local Mermaid sketches. Task spec, Structured note, Polish and Custom remain available. Active drafts reconcile at Stop; paused drafts are saved with a review label. See [workspace settings](../docs/providers-and-live-rewrite.md).

After a completed dictation, **Continue** in the widget or **Continue recording** in the app adds speech to the same conversation. The Live rewrite button cycles Off, Once (1×) and Continuous (∞). The rewrite model picker and Providers settings offer model/provider-specific **Thinking level** choices.

## Supported desktop contract

| Requirement | Omarchy | Fedora GNOME compatibility |
| --- | --- | --- |
| Desktop | Omarchy Quattro with Hyprland 0.55+ and Quickshell 0.3+ | Last accepted on Fedora 44 / GNOME 50; recent releases unverified |
| Native UI | GTK 4, Libadwaita; released Python development also needs distribution PyGObject and Cairo bindings | Same |
| Audio | PipeWire, `pw-record`, `pw-dump` | Same |
| Shortcuts | Hyprland compositor binding or XDG Global Shortcuts portal | XDG Global Shortcuts portal and the desktop backend |
| Clipboard | `wl-copy` on Wayland | Same |
| Remaining Python development/root setup | Distribution Python 3.12+ and `uv`; neither is used by the native app-only bundle | Same |

The released `bash install.sh` at the repository root installs runtime packages, the app and, on Omarchy, the plugin. `--app-only` skips plugin changes. On this Rust development branch, that root setup still needs conversion of widget provisioning and native build prerequisites. Prepare the [native source build dependencies](../CONTRIBUTING.md#local-setup) separately before using source app-only installation. The remaining Python development environment uses these runtime packages:

```sh
# Omarchy
omarchy pkg add git uv python python-gobject python-cairo gtk4 libadwaita \
  at-spi2-core gobject-introspection dbus pipewire pipewire-audio wl-clipboard wtype procps-ng webkitgtk-6.0 bubblewrap

# Fedora GNOME compatibility
sudo dnf install git uv python3-gobject gtk4 libadwaita at-spi2-core \
  gobject-introspection dbus-daemon pipewire-utils wl-clipboard procps-ng webkitgtk6.0 bubblewrap
```

X11 clipboard delivery needs `xclip`; its optional keyboard fallback needs `xdotool`. The Omarchy widget requires the existing Omarchy shell and plugin manager; setup does not install an operating system or replace desktop configuration.

Omarchy setup includes `wtype` for optional keyboard paste. Existing installations can add it with `omarchy pkg add wtype`. This uses symbolic keys, so the paste shortcut does not assume a QWERTY letter position.

## Install for the current user

With the [native build prerequisites](../CONTRIBUTING.md#local-setup) and runtime commands (`pw-record`, `pw-dump`, `wl-copy`) already present, build and install only the native app:

```sh
make linux-install
```

Application files go under `~/.local/share/mluva/app`, commands under `~/.local/bin`, and the desktop entry under the XDG applications directory. Source installation builds an optimized Rust bundle with locked dependencies before changing the user prefix, then invokes its native installer. Prepared native bundles invoke that installer directly and need no compiler or Python. It refuses an active app or unrecognized installation, preserves the previous app until setup succeeds, and rolls back on failure. [Upgrades from 0.x](../docs/identity-migration.md) preserve settings and saved work.

The app requests approval for recording, cancellation and opening the latest conversation on first launch. Settings shows the keys actually approved by the desktop. The default recording key is F9; alternatives range from F1 to F24. Changing a key replaces the portal session. Right Alt/AltGr remains available to the keyboard layout.

On Hyprland, a native keyboard binding can use `mluva-shell global-record` with portal shortcuts disabled. Bindings to `mluva-shell record` start copy-only capture, even when automatic paste is enabled. See the [Omarchy binding instructions](../docs/omarchy-integration.md).

To verify the package in a disposable prefix without changing the active installation:

```sh
MLUVA_INSTALL_HOME="$PWD/tmp/staged-home" bash linux/install.sh
```

Staged mode skips live desktop, systemd and credential integration. Use the [isolated runner](../dev/README.md) for any GUI launch from that prefix.

Run installed `mluva-uninstall` to remove the native app and owned launch integrations without building anything. Source `make linux-uninstall` builds the native remover and requires the development prerequisites. Settings, history, recordings and migration backups remain. On Omarchy, remove the bundled widget with `omarchy plugin remove mluva.dictation` before uninstalling the app.

## Provider setup

Speech recognition and rewriting are selected independently in **Settings → Providers**. The defaults are ElevenLabs Scribe and an authenticated Codex CLI. Alternatives include app-managed local models and compatible speech or rewrite APIs. First-run setup downloads local models and can skip rewriting entirely. [Onboarding](../docs/onboarding.md) explains appearance previews and keyring setup. [Choose providers](../docs/provider-selection.md).

Supply credentials to the app process through your secret manager or desktop launch environment. Settings stores key-variable names, never key values. Restart Mluva after changing that environment. A cloud route may send audio or text off the device; local recognition alone does not make rewrites or generated titles local.

The launcher also supports an existing managed credential profile. When the managed local snapshot is enabled, the launcher requests only `ELEVEN_LABS_STT_TOKEN` from it before considering legacy network profiles. `MLUVA_AGENT_SECRET_NAME` can select another supported credential name in that snapshot. This avoids retired profile references blocking startup after credential rotation. The [native launcher](../rust/mluva-gtk/tests/fixtures/launcher-evidence.md) owns this optional route; `configure-secret-profile.sh` remains available for manual profile setup. Ordinary installations do not require it.

## Optional desktop integrations

Clipboard delivery remains the standard workflow. The optional GNOME overlay below has not been tested in recent releases.

`mluva-overlay install` enables the optional GNOME Shell menu and display-only recording bar. A newly installed extension may need one logout/login before GNOME discovers it. The app's in-window recording display is available without the extension. Remove it with `mluva-overlay remove`.

Automatic insertion is Experimental and disabled by default. Native AT-SPI editing needs accessibility enabled before the app starts. On GNOME, check `gsettings get org.gnome.desktop.interface toolkit-accessibility`; enable it through desktop settings or `gsettings set org.gnome.desktop.interface toolkit-accessibility true`, then restart applications that do not expose text targets.

When a captured AT-SPI target lacks native editing, symbolic keyboard paste uses `Ctrl+Shift+V` for recognized terminal executables (Foot, Alacritty, Ghostty, Kitty and WezTerm) and `Ctrl+V` for other applications. The terminal chord reads the clipboard rather than the primary selection used by default Foot's `Shift+Insert`. On Hyprland, terminals can also be captured without AT-SPI: Mluva verifies the focused window's owning executable and requires that exact window and process to remain focused immediately before paste. It does not activate a different window or read terminal contents. Other applications still require an accessible text target; unsupported terminals on other desktops remain copy-only.

Terminal capture identifies a window and process, not a tab, pane or shell input mode. Keep the intended input active until delivery. Terminal dispatch remains unconfirmed because Mluva cannot inspect its caret or contents.

The optional `ydotool` helper supports only the terminal `Shift+Insert` fallback, which requires that target's `Shift+Insert` to be bound to clipboard paste, as in Omarchy's terminal configuration. It sends physical keycodes, so Mluva does not guess a letter-key code from the physical keyboard's layout: ydotool's virtual device can use a different layout. Symbolic `wtype` on supported Wayland compositors or `xdotool` on X11 is preferred. To enable the terminal helper, install the distribution's `ydotool` package and explicitly run `mluva-input-helper install`. This uses sudo to install a system service and gives processes running as your user access to its private synthetic-keyboard socket. Remove it with `mluva-input-helper remove`. Ordinary installation does not enable it.

## Data and troubleshooting

Settings normally live in `~/.config/mluva`, persistent data in `~/.local/share/mluva`, and temporary state in the XDG runtime directory. The [product contract](../docs/product-contract.md) covers privacy, recovery and retention.

On Omarchy, `mluva-shell screenshot` adds a selected region to the current narration or saved conversation without changing the clipboard. The camera button exposes the same action; [bind an unused F10](../docs/omarchy-integration.md#screenshot-context) for capture from another application. Saved images can be edited in Tensaku and are sent with subsequent AI processing when the selected model supports images. Incognito disables capture. The optional [narrated editor extension](integrations/tensaku/README.md) uses the configured speech provider for a fresh text box phrase and saves it in the chosen image area.

If capture fails, check the input and provider status in Settings. If the widget reports that Mluva is unavailable, start the app and check that its `mluva-shell` command is reachable; see the [Omarchy guide](../docs/omarchy-integration.md#installation). A failed or uncertain insertion leaves the text available for Copy rather than retrying into an unknown target.

## Development

During the port, `make linux-setup` creates the remaining Python development environment with system GTK bindings, and `make linux-run` still starts that implementation; use it only when you intend to open the UI. `make linux-test` runs its deterministic suite, lint, formatting and the native feature-matrix check. The [contributor guide](../CONTRIBUTING.md#verification) lists focused and isolated integration checks; the [Rust guide](../rust/README.md) documents native builds and acceptance evidence.
