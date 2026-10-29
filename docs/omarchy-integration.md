# Omarchy integration

Mluva’s optional `mluva.dictation` plugin uses Omarchy’s popup colors, border, corner radius and button controls, with bundled JetBrains Mono text. The GTK app follows the active Omarchy `colors.toml` palette, including light/dark mode and theme changes while it is open. Outside Omarchy it follows the system scheme.

With multiple monitors, each bar keeps its dictation control but only the first attached bar widget creates the floating recorder. Removing that bar transfers ownership to a remaining widget. The compositor chooses the recorder’s display when it opens; the bar’s display does not force its placement.

During recording, a 500-pixel-wide window starts near the bottom center of the display. Settings → Workspace or Ctrl+P can select lower left, bottom center or lower right. Drag its status row or transcript to move it; ordinary updates preserve manual placement. The recording light sits at top left and the timer at top right, with no background behind either. A separate 82% opaque surface holds five wrapped transcript lines at 14 pixels with 22-pixel leading. A filled fluid contour breathes on a 3.4-second cycle and honor reduced motion. Click the window and use Omarchy’s **Super+T** to switch between floating and tiling; resizing gives the preview more room without shrinking the text. Floating recordings stay above other windows and across workspaces, including after returning from tiling. Opening the recorder leaves keyboard focus in the current application; clicking it deliberately gives it focus. Scrolling anticipates the next line from recent speech rate and moves forward during automatic updates. Local text revisions fade only changed words. Manual scrolling takes control and releases temporary correction padding.

In the main application, the recording light and elapsed time span the content above the text panes. The Stop button remains below the text, beside a small bottom-aligned status. Preparation and finalization use the same header without a recording pulse.

After dictation, the recorder closes. Open the latest conversation with Shift+F9 to edit, copy, save or continue recording. There is no completed-note review, rewrite menu or dismissal countdown. Smooth following is always enabled and respects the desktop reduced-motion preference; deliberate scrolling preserves the reading position.

## Installation

Use the [combined installation command or agent prompt](../README.md#install). From a complete Mluva checkout, `bash install.sh` installs system dependencies, the native app and the bundled Omarchy widget together from the same release. Start Mluva from the application menu after setup. This integration needs Omarchy Quattro's existing shell and plugin manager, Quickshell 0.3+ and Hyprland 0.55+.

For a Git-managed plugin installation, use the [plugin-manager commands](../README.md#install-through-omarchys-plugin-manager). The root manifest points at `linux/quickshell/mluva.dictation/Widget.qml`; the native app, widget, releases and website remain in this repository. Omarchy clones and enables QML but does not run installation hooks or install build tools, GTK, PipeWire, provider clients or models. On the Rust development branch, prepare the pinned Rust toolchain and a C compiler, then run `bash "$HOME/.config/omarchy/plugins/mluva.dictation/install.sh" --app-only` to install the matching native app and remaining system packages. Optional providers and models remain separate choices in Settings; complete Rust acceptance is still pending.

Git-managed widgets update with `omarchy plugin update mluva.dictation`, followed by the same `--app-only` command while Mluva is closed. Updating only the plugin leaves the native app unchanged. The combined installer protects Git-managed checkouts from replacement; use `--app-only` inside the existing plugin checkout. Conversely, bundled installations keep using the combined setup below. Do not add a second copy with the same plugin ID. To switch routes, back up any custom widget files outside the plugin directory, remove the existing widget with `omarchy plugin remove mluva.dictation`, then follow the chosen installation path.

The plugin registers a Lua rule for its own window before showing it and reapplies that rule after a compositor configuration reload. It does not edit desktop configuration files.

If the matching native app is already installed and you only need the widget, run this from the Rust source checkout, with the [source prerequisites](../CONTRIBUTING.md#local-setup) prepared:

```sh
bash linux/install-widget.sh
```

From a prepared native runtime bundle, use `bin/mluva-install-widget` directly; it needs no compiler or Python. Published 1.6.0 archives retain their own bundled instructions.

The plugin invokes the native app's `mluva-shell` bridge. If the shell cannot find it, set the widget's **Mluva shell executable** setting to the full path of `~/.local/bin/mluva-shell`, expanded to your actual home directory.

For bundled installations, rerun the combined setup from the latest Mluva release to update both parts. The app and widget share one version; the installer never fetches a separate widget repository. Clean legacy Git installations migrate automatically without contacting their retired remote. Previous widget files are retained outside plugin discovery under `~/.config/omarchy/plugin-backups/`. Local edits, symlinks, duplicate widgets and unmanaged manual installations stop setup before package or app changes. Move any custom installation outside the plugins directory after backing it up, then rerun setup; `bash install.sh --app-only` leaves plugins alone.

The installer validates the widget, uses a content-specific QML entry-point path and rescans the existing shell before enabling the stable `mluva.dictation` ID. This loads changed components without restarting the bar or moving its controls. If widget setup fails after native installation, its previous files are restored and the native app remains available. Resolve the reported error and rerun setup. To restore a backup manually, disable the widget, move the current plugin directory aside, restore the saved directory under `~/.config/omarchy/plugins/`, then rescan and enable it.

For bundled plugin-only maintenance:

```sh
# Update from the matching prepared native bundle:
bin/mluva-install-widget
omarchy plugin disable mluva.dictation
omarchy plugin remove mluva.dictation
```

Remove the plugin before uninstalling the native app with `mluva-uninstall`. Settings and saved conversations remain. Plugin removal does not uninstall the app.

The bar’s left click starts or stops clipboard-only dictation. Right click cancels capture; middle click opens the latest conversation. These commands address the existing application and never start it implicitly. F9 remains the configured dictation shortcut.

For a Hyprland keyboard binding, use `mluva-shell global-record`. It captures the focused text field or supported terminal at Start and automatically inserts into that same verified target at Stop. `mluva-shell record` is the bar/button action and always starts copy-only capture. Stopping from a different control preserves the original delivery choice.

In `~/.config/hypr/bindings.lua`, replace an existing Mluva F9 binding with:

```lua
hl.unbind("F9")
o.bind("F9", "Mluva start/stop", os.getenv("HOME") .. "/.local/bin/mluva-shell global-record")
```

The manual binding above uses the fixed key written in your Lua file. Launch with `MLUVA_DISABLE_GLOBAL_SHORTCUT=1` only for that manual setup; it disables all portal actions, including agent keys.

For keys managed from Settings → Capture on Lua-based Hyprland, use the opt-in portal bridge instead. Hyprland registers portal actions but requires compositor bindings to trigger them. Replace existing manual Mluva recording/cancel/latest bindings with this loader in `~/.config/hypr/bindings.lua`, leaving other shortcuts intact:

```lua
local mluva_keys = (os.getenv("XDG_CONFIG_HOME") or (os.getenv("HOME") .. "/.config")) .. "/mluva/hyprland-shortcuts.lua"
local file = io.open(mluva_keys, "r")
if file then file:close(); dofile(mluva_keys) end
```

Create that file initially with the single line `-- Mluva managed portal bindings v1`. Remove `MLUVA_DISABLE_GLOBAL_SHORTCUT` from the app's launch environment, then reload the configuration and restart Mluva. Mluva generates bindings for the normal key, cancellation, latest conversation and enabled agent keys, verifies them against Hyprland and updates them automatically when Settings changes. The generated file survives a configuration reload and login; Incognito changes apply only in memory. Binding handles remove only Mluva's own entries. A key already assigned to another application is rejected; choose a free key in Settings. The bridge does not change unrelated desktop configuration or synthesize input. Other desktops continue using portal approval.

`mluva-shell watch` emits only capture phase and elapsed seconds. The plugin opts into `watch --overlay`, which carries audio level, up to 4,096 characters of volatile text, its character offset, and recording appearance settings. The offset lets the widget keep the same wrapping as older words leave the bounded preview, including text containing emoji. Saved instructions, credentials, device names, and target application names are excluded. The production plugin does not log or persist this stream; do not redirect it into persistent logs.

## Verification

Run `make linux-test linux-shortcut-test linux-application-test linux-omarchy-test`. The native widget check connects the production status publisher, shell bridge and QML in a private session. It exercises recording focus/geometry, scrolling and contraction, Unicode preview origins, light/dark colors, reduced motion and owner loss. It requires the installed Omarchy shell, Quickshell, Xvfb, xdotool, ImageMagick and FFmpeg. `MLUVA_PANEL_REPLAY=1 make linux-omarchy-test` selects the retained preview replay.

[Current evidence](verification/dictation-only/README.md) distinguishes isolated acceptance from physical Wayland checks. Older [Wayland widget](verification/current-wayland-widget/README.md), [recorder header](verification/recorder-header/README.md) and [theme](../rust/mluva-gtk/tests/fixtures/theme-evidence.md) receipts remain historical references; their removed review controls are no longer product behavior.
