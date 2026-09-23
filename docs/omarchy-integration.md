# Omarchy integration

Mluva’s optional `mluva.dictation` plugin uses Omarchy’s popup colors, border, corner radius and button controls, with bundled JetBrains Mono text. The GTK app follows the active Omarchy `colors.toml` palette, including light/dark mode and theme changes while it is open. Outside Omarchy it follows the system scheme.

With multiple monitors, each bar keeps its dictation control but only the first attached bar widget creates the floating recorder. Removing that bar transfers ownership to a remaining widget. The compositor chooses the recorder’s display when it opens; the bar’s display does not force its placement.

During recording, a 500-pixel-wide window starts near the bottom center of the display. Settings → Workspace or Ctrl+P can select lower left, bottom center or lower right. Drag its status row or transcript to move it; ordinary updates preserve manual placement. The recording light sits at top left and the timer at top right, with no background behind either. A separate 82% opaque surface holds five wrapped transcript lines at 14 pixels with 22-pixel leading. A filled fluid contour breathes on a 3.4-second cycle and honor reduced motion. Click the window and use Omarchy’s **Super+T** to switch between floating and tiling; resizing gives the preview more room without shrinking the text. Floating recordings stay above other windows and across workspaces, including after returning from tiling. Opening the recorder leaves keyboard focus in the current application; clicking it deliberately gives it focus. Scrolling anticipates the next line from recent speech rate and moves forward during automatic updates. Local text revisions fade only changed words. Manual scrolling takes control and releases temporary correction padding.

In the main application, the recording light and elapsed time span the content above the text panes. The Stop button remains below the text, beside a small bottom-aligned status. Preparation and finalization use the same header without a recording pulse.

After dictation, **Continue** resumes the same conversation, keeping the previous text visible and appending new speech underneath. The widget also offers **Polish**, **Structure**, and **More** with the built-in styles and saved prompts. Choosing a rewrite runs it in the background against that exact note, even while another conversation is open. The completed review closes after four idle seconds by default, shown by a shrinking ring around Dismiss. Hovering the widget, deliberate interaction while focused or an open menu pauses the countdown; inherited focus alone does not. Rewriting suspends it; completion starts a fresh configured interval. A new recording replaces the review. The full note remains in history and can be reopened with the configured Shift+F9 shortcut. **Copy** copies the displayed version; **Open** opens that conversation and its instruction box. **Cancel** stops a pending rewrite. Rewrites preserve raw recognition, append to the workspace, and copy successful results automatically by default. The compact Copy icon remains available when enabled. Settings → Workspace controls automatic copying, icon visibility, the dismissal delay and scroll behavior; the same keys live in the [JSON configuration](providers-and-live-rewrite.md). Errors retain the note and its controls. Incognito does not expose a completed conversation for rewriting.

Rewrites stream into the widget and the matching open conversation as text arrives, with display updates limited to twenty per second. Partial text stays in memory and has no Copy action. Only a successfully completed, bounded reply enters history. Cancellation, failure, deletion, and Incognito erase the partial preview; queued updates cannot revive a cancelled request. Browsing another note does not move the stream onto that note.

The recording and review surfaces accept pointer input. Neither requests focus when opening. Losing the application’s bus owner clears the preview, conversation ID, options, and menu.

## Installation

Use the [combined installation command or agent prompt](../README.md#install). From a complete Mluva checkout, `bash install.sh` installs system dependencies, the native app and the bundled Omarchy widget together from the same release. Start Mluva from the application menu after setup. This integration needs Omarchy Quattro's existing shell and plugin manager, Quickshell 0.3+ and Hyprland 0.55+.

The plugin registers a Lua rule for its own window before showing it and reapplies that rule after a compositor configuration reload. It does not edit desktop configuration files.

If the matching native app is already installed and you only need the widget, run this from the same Mluva checkout or extracted release archive:

```sh
python3 linux/install_widget.py
```

The plugin invokes the native app's `mluva-shell` bridge. If the shell cannot find it, set the widget's **Mluva shell executable** setting to the full path of `~/.local/bin/mluva-shell`, expanded to your actual home directory.

Rerun the combined setup from the latest Mluva release to update both parts. The app and widget share one version; the installer never fetches a separate widget repository. Clean legacy Git installations migrate automatically without contacting their retired remote. Previous widget files are retained outside plugin discovery under `~/.config/omarchy/plugin-backups/`. Local edits, symlinks, duplicate widgets and unmanaged manual installations stop setup before package or app changes. Move any custom installation outside the plugins directory after backing it up, then rerun setup; `bash install.sh --app-only` leaves plugins alone.

The installer validates the widget, uses a content-specific QML entry-point path and rescans the existing shell before enabling the stable `mluva.dictation` ID. This loads changed components without restarting the bar or moving its controls. If widget setup fails after native installation, its previous files are restored and the native app remains available. Resolve the reported error and rerun setup. To restore a backup manually, disable the widget, move the current plugin directory aside, restore the saved directory under `~/.config/omarchy/plugins/`, then rescan and enable it.

For plugin-only maintenance:

```sh
# Update from the matching Mluva release:
python3 linux/install_widget.py
omarchy plugin disable mluva.dictation
omarchy plugin remove mluva.dictation
```

Remove the plugin before uninstalling the native app with `mluva-uninstall`. Settings and saved conversations remain. Plugin removal does not uninstall the app.

The bar’s left click starts or stops clipboard-only dictation. Right click cancels capture; middle click opens the latest conversation. These commands address the existing application and never start it implicitly. F9 remains the configured dictation shortcut.

`mluva-shell watch` emits only capture phase and elapsed seconds. The plugin opts into `watch --overlay`, which carries audio level, up to 4,096 characters of volatile text, its character offset, and bounded conversation/style identifiers and labels. The offset lets the widget keep the same wrapping as older words leave the bounded preview, including text containing emoji. Saved instructions, credentials, device names, and target application names are excluded. The production plugin does not log or persist this stream; do not redirect it into persistent logs. Review commands pass only action, conversation ID, and style ID through the existing GApplication action group.

## Verification

Run `make linux-test linux-shortcut-test`, repository shell checks and `make linux-omarchy-test`. The Omarchy runtime fixture requires an installed shell under `/usr/share/omarchy/shell`; the runner supplies a private X11 display, session bus and XDG state. `linux/tests/shell_overlay_smoke.py` copies the installed controls into its private fixture and redirects desktop configuration reads. It runs the production widget and bridge against a separate synthetic publisher, checks focus retention while recording, five-line geometry, intermediate scrolling frames, long-preview wrapping, controls, light/dark colors, errors, preview erasure, owner loss, monitor fit, timed dismissal and actual pointer/keyboard countdown pauses, and retains screenshots.

`linux/tests/conversation_ui_smoke.py` with `MLUVA_UI_SCENARIO=lifecycle` tests the real GTK callbacks and a separate fake model subprocess, including note identity during browsing, deliberate Copy, saved prompts, duplicate clicks, cancellation, Incognito, deletion, late completion after dismissal, automatic title generation, a queued title, manual renames, unsaved title edits and provider failure. `linux/tests/theme_ui_smoke.py` replaces only a private theme symlink and checks that an already open GTK window follows both schemes and recovers from malformed theme data.

The private X11 checks establish production QML rendering and application/bridge behavior. The core dictation, rewrite and widget workflow is also tested end to end through daily Omarchy use. These isolated checks complement that desktop acceptance; automatic insertion and alternative provider/model combinations retain their separate limits.

The earlier [recorder-header verification](verification/recorder-header/README.md) covers native dragging from either surface, tiling and pinning, and a cached-plugin upgrade inside the installed Omarchy shell. The [fluid workspace verification](verification/fluid-workspace.md) covers the current bare header, placement presets and recording pulse, with its separate isolation limits.

The implementation follows Quickshell’s documented [FloatingWindow](https://quickshell.org/docs/v0.3.0/types/Quickshell/FloatingWindow/) and [PopupAnchor](https://quickshell.org/docs/v0.3.0/types/Quickshell/PopupAnchor/) contracts and Hyprland’s [window rules](https://wiki.hypr.land/Configuring/Basics/Window-Rules/). Omarchy’s native widget and style interfaces are documented in the installed shell’s `README.md` and `Commons`/`Ui` components.
