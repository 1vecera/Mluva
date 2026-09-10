# Omarchy integration

Mluva’s optional `mluva.dictation` plugin uses Omarchy’s popup colors, border, corner radius, font scale, and button controls. The GTK app follows the active Omarchy `colors.toml` palette, including light/dark mode and theme changes while it is open. Outside Omarchy it follows the system scheme.

During recording, a 500 × 127 logical-pixel window starts near the bottom center of the display and shows five wrapped lines at the default shell font size on an 82% opaque surface. Drag its status row to move it. Click the window and use Omarchy’s **Super+T** to switch between floating and tiling; resizing gives the preview more room without shrinking the text. Floating recordings stay above other windows and across workspaces, including after returning from tiling. Opening the recorder leaves keyboard focus in the current application; clicking it deliberately gives it focus. Scrolling starts gently as the last visible lines fill and follows new lines with eased movement.

In the main application, the recording light and elapsed time occupy the existing title bar above the text panes. The Stop button remains below the text. Preparation and finalization use the same header without a blinking recording light.

After dictation, the widget offers **Polish**, **Structure**, and **More** with the built-in styles and saved prompts. Choosing a rewrite runs it in the background against that exact note, even while another conversation is open. The completed review closes after four idle seconds by default, shown by a shrinking ring around Dismiss. Hovering the widget, keyboard focus or an open menu pauses the countdown. Rewriting suspends it; completion starts a fresh configured interval. A new recording replaces the review. The full note remains in history and can be reopened with the configured Shift+F9 shortcut. **Copy** copies the displayed version; **Open** opens that conversation and its instruction box. **Cancel** stops a pending rewrite. Rewrites preserve raw recognition, append to the workspace, and copy successful results automatically by default. The compact Copy icon remains available when enabled. Settings → Workspace controls automatic copying, icon visibility, the dismissal delay and scroll behavior; the same keys live in the [JSON configuration](providers-and-live-rewrite.md). Errors retain the note and its controls. Incognito does not expose a completed conversation for rewriting.

Rewrites stream into the widget and the matching open conversation as text arrives, with display updates limited to twenty per second. Partial text stays in memory and has no Copy action. Only a successfully completed, bounded reply enters history. Cancellation, failure, deletion, and Incognito erase the partial preview; queued updates cannot revive a cancelled request. Browsing another note does not move the stream onto that note.

The recording and review surfaces accept pointer input. Neither requests focus when opening. Losing the application’s bus owner clears the preview, conversation ID, options, and menu.

## Installation

Use the [combined installation command or agent prompt](../README.md#install). From a complete Mluva checkout, `bash install.sh` installs system dependencies, the native app and the [community Omarchy plugin](https://github.com/1vecera/omarchy-mluva) together. Start Mluva from the application menu after setup. This integration needs Omarchy Quattro's existing shell and plugin manager, Quickshell 0.3+ and Hyprland 0.55+.

The plugin registers a Lua rule for its own window before showing it and reapplies that rule after a compositor configuration reload. It does not edit desktop configuration files.

If the native app is already installed and you only need the widget:

```sh
omarchy plugin add https://github.com/1vecera/omarchy-mluva.git --enable
```

The plugin invokes the native app's `mluva-shell` bridge. If the shell cannot find it, set the widget's **Mluva shell executable** setting to the full path of `~/.local/bin/mluva-shell`, expanded to your actual home directory.

Rerun the combined setup to update both parts after updating your Mluva checkout. It uses Omarchy's plugin manager and refuses to overwrite an unmanaged plugin directory or local plugin edits. Back up customizations before resolving those conflicts. `bash install.sh --app-only` leaves plugins alone. If widget setup fails after native installation, the app remains available; resolve the reported plugin error and rerun setup.

For plugin-only maintenance:

```sh
omarchy plugin update mluva.dictation
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

The implementation follows Quickshell’s documented [FloatingWindow](https://quickshell.org/docs/v0.3.0/types/Quickshell/FloatingWindow/) and [PopupAnchor](https://quickshell.org/docs/v0.3.0/types/Quickshell/PopupAnchor/) contracts and Hyprland’s [window rules](https://wiki.hypr.land/Configuring/Basics/Window-Rules/). Omarchy’s native widget and style interfaces are documented in the installed shell’s `README.md` and `Commons`/`Ui` components.
