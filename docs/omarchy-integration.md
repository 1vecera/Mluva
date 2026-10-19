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

When using compositor bindings, launch Mluva with `MLUVA_DISABLE_GLOBAL_SHORTCUT=1` to avoid a second portal binding for the same keys. The keyboard action addresses the running application without opening its window; keep Mluva running in the background. The installer preserves custom Hyprland bindings, so an older `mluva-shell record` binding needs this one-time change.

`mluva-shell watch` emits only capture phase and elapsed seconds. The plugin opts into `watch --overlay`, which carries audio level, up to 4,096 characters of volatile text, its character offset, and recording appearance settings. The offset lets the widget keep the same wrapping as older words leave the bounded preview, including text containing emoji. Saved instructions, credentials, device names, and target application names are excluded. The production plugin does not log or persist this stream; do not redirect it into persistent logs.

## Screenshot context

Bind an unused function key to `mluva-shell screenshot`; F10 is the suggested choice. In the existing Omarchy Lua bindings file, use the same native binding API as F9:

```lua
hl.unbind("F10")
o.bind("F10", "Mluva screenshot context", os.getenv("HOME") .. "/.local/bin/mluva-shell screenshot")
```

During a narration this invokes Omarchy's existing region selector and attaches its PNG to that recording, with its elapsed time. In a saved conversation it attaches to the selected conversation. Switching conversations while selecting does not change the frozen destination. Escape cancels; the capture does not replace the clipboard. Incognito does not accept screenshots.

Mluva opens the attached image in the normal `tensaku-edit` editor. Its thumbnail refreshes after a save. Images remain local attachments; built-in AI rewriting has been removed. Removing an image, deleting its conversation and history pruning remove its managed file. Screenshot files live under the Mluva data directory and inherit the conversation's retention.

Install the optional [Tensaku source extension](../linux/integrations/tensaku/README.md) to enable narrated text boxes in the current default editor. **Add narration** → choose an area → speak a fresh phrase → **Stop narration** inserts the complete text and saves the PNG. It uses Mluva's saved microphone, language and speech provider, with its deterministic text rules. A main narration can continue recording the same speech. Escape or closing the editor cancels the annotation; its temporary raw audio is erased. This action does not copy, paste or create an extra Mluva conversation. Incognito is checked again before uploading annotation audio.

## Verification

Run `make linux-test linux-shortcut-test linux-application-test linux-omarchy-test`. The native widget check connects the production status publisher, shell bridge and QML in a private session. It exercises recording focus/geometry, scrolling and contraction, Unicode preview origins, light/dark colors, reduced motion and owner loss. It requires the installed Omarchy shell, Quickshell, Xvfb, xdotool, ImageMagick and FFmpeg. `MLUVA_PANEL_REPLAY=1 make linux-omarchy-test` selects the retained preview replay.

[Current evidence](verification/dictation-only/README.md) distinguishes isolated acceptance from physical Wayland checks. Older [Wayland widget](verification/current-wayland-widget/README.md), [recorder header](verification/recorder-header/README.md) and [theme](../rust/mluva-gtk/tests/fixtures/theme-evidence.md) receipts remain historical references; their removed review controls are no longer product behavior.
