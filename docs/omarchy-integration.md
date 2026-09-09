# Omarchy integration

Mluva’s optional `mluva.dictation` plugin uses Omarchy’s popup colors, border, corner radius, font scale, and button controls. The GTK app follows the active Omarchy `colors.toml` palette, including light/dark mode and theme changes while it is open. Outside Omarchy it follows the system scheme.

During recording, a centered 500 × 127 logical-pixel widget shows five wrapped lines at the default shell font size on an 82% opaque surface. It shrinks to fit narrow displays and grows with the shell’s font scale. The status and timer use a compact row; text stays the same size. Scrolling starts gently as the fifth line fills and follows new lines with eased movement. The recording surface passes mouse and keyboard input through to the current application and reserves no desktop space.

After dictation, the widget offers **Polish**, **Structure**, and **More** with the built-in styles and saved prompts. Choosing a rewrite runs it in the background against that exact note, even while another conversation is open. The completed review closes after four idle seconds by default, shown by a shrinking ring around Dismiss. Hovering the widget, keyboard focus or an open menu pauses the countdown. Rewriting suspends it; completion starts a fresh configured interval. A new recording replaces the review. The full note remains in history and can be reopened with the configured Shift+F9 shortcut. **Copy** copies the displayed version; **Open** opens that conversation and its instruction box. **Cancel** stops a pending rewrite. Rewrites preserve raw recognition, append to the workspace, and copy successful results automatically by default. The compact Copy icon remains available when enabled. Settings → Workspace controls automatic copying, icon visibility, the dismissal delay and scroll behavior; the same keys live in the [JSON configuration](providers-and-live-rewrite.md). Errors retain the note and its controls. Incognito does not expose a completed conversation for rewriting.

Rewrites stream into the widget and the matching open conversation as text arrives, with display updates limited to twenty per second. Partial text stays in memory and has no Copy action. Only a successfully completed, bounded reply enters history. Cancellation, failure, deletion, and Incognito erase the partial preview; queued updates cannot revive a cancelled request. Browsing another note does not move the stream onto that note.

The review surface accepts pointer input and requests keyboard focus only on demand. Recording never requests focus. Losing the application’s bus owner clears the preview, conversation ID, options, and menu.

## Installation

Install and start [Mluva v0.1.1](https://github.com/1vecera/Mluva/releases/tag/v0.1.1), including its [Linux dependencies](../linux/README.md). The installer supplies `mluva-shell`. On Omarchy Quattro, use the public plugin repository:

```sh
omarchy plugin add https://github.com/1vecera/omarchy-mluva.git --enable
```

The repository contains one root manifest, the released QML, an Apache license and a source/hash record. `omarchy plugin update mluva.dictation` updates that Git-managed copy. This is a community integration; [marketplace submission #5533](https://github.com/omacom/omarchy-plugin-marketplace/issues/5533) awaits a maintainer's listing decision.

If a manually copied `mluva.dictation` directory already exists, back it up or remove it with `omarchy plugin remove mluva.dictation` before adding the Git-managed copy. Omarchy refuses duplicate plugin IDs. The application and saved conversations are separate from plugin removal.

For development or a manual install, the application still packages the same plugin under `~/.local/share/voice-scribe/app/quickshell/mluva.dictation`:

```sh
mkdir -p ~/.config/omarchy/plugins/mluva.dictation
cp ~/.local/share/voice-scribe/app/quickshell/mluva.dictation/* ~/.config/omarchy/plugins/mluva.dictation/
omarchy-shell shell rescanPlugins
omarchy plugin enable mluva.dictation
```

Use the installed `~/.local/bin/mluva-shell` as the plugin’s command when it is absent from the shell’s PATH. When upgrading an already loaded plugin, use `omarchy restart shell` if the shell retains the previous QML components.

The bar’s left click starts or stops clipboard-only dictation. Right click cancels capture; middle click opens the latest conversation. These commands address the existing application and never start it implicitly. F9 remains the configured dictation shortcut.

`mluva-shell watch` emits only capture phase and elapsed seconds. The plugin opts into `watch --overlay`, which carries audio level, up to 4,096 characters of volatile text, its character offset, and bounded conversation/style identifiers and labels. The offset lets the widget keep the same wrapping as older words leave the bounded preview, including text containing emoji. Saved instructions, credentials, device names, and target application names are excluded. The production plugin does not log or persist this stream; do not redirect it into persistent logs. Review commands pass only action, conversation ID, and style ID through the existing GApplication action group.

Disable with `omarchy plugin disable mluva.dictation`. Uninstalling Mluva removes its packaged bridge and plugin, while preserving the user-copied plugin and shell configuration.

## Recording display rationale

Google’s CHI 2023 study found that unstable live captions distract readers and developed ways to model and reduce that instability. That supports avoiding unnecessary visual motion, although it does not establish an optimal widget size for dictation. [Modeling and Improving Text Stability in Live Captions](https://research.google/pubs/modeling-and-improving-text-stability-in-live-captions/).

Research on simultaneous subtitles compares word-at-a-time, block, and rolling-line presentation. It motivates retaining visible context, but the study concerns translated subtitles rather than this application. [Simultaneous Speech Translation for Live Subtitling: from Delay to Display](https://arxiv.org/abs/2107.08807).

The resulting design choice is five fixed-height wrapped lines, with the latest line visible and no ticker or font shrinking. This is a product judgment informed by that research, not a measured comprehension improvement. Recognition corrections remain visible immediately; the UI does not freeze or alter the recognizer’s words. Qt’s [Text.Wrap and plain-text rendering](https://doc.qt.io/qt-6/qml-qtquick-text.html) handle long tokens and keep recognized markup literal. A discarded-prefix origin and first-line indentation retain the visible wrapping as the bounded text window advances. New conversations and changes in width or font establish a fresh layout.

## Verification

Run `make linux-test linux-shortcut-test` and repository shell checks. The Omarchy runtime fixtures require an installed shell under `/usr/share/omarchy/shell` and a private X11 display, session bus, and XDG state. `linux/tests/shell_overlay_smoke.py` copies the installed controls into its private fixture and redirects desktop configuration reads. It runs the production widget and bridge against a separate synthetic publisher, checks focus retention while recording, five-line geometry, intermediate scrolling frames, long-preview wrapping, controls, light/dark colors, errors, preview erasure, owner loss, monitor fit, timed dismissal and actual pointer/keyboard countdown pauses, and retains screenshots.

`linux/tests/conversation_ui_smoke.py` with `MLUVA_UI_SCENARIO=lifecycle` tests the real GTK callbacks and a separate fake model subprocess, including note identity during browsing, deliberate Copy, saved prompts, duplicate clicks, cancellation, Incognito, deletion, late completion after dismissal, automatic title generation, a queued title, manual renames, unsaved title edits and provider failure. `linux/tests/theme_ui_smoke.py` replaces only a private theme symlink and checks that an already open GTK window follows both schemes and recovers from malformed theme data.

The private X11 checks establish production QML rendering and application/bridge behavior. Physical F9 capture, Hyprland layer-shell focus handoff, popup outside-click dismissal, and insertion into real applications still require live desktop acceptance. This integration remains Experimental until that acceptance is recorded.

The implementation follows Quickshell’s documented [PanelWindow](https://quickshell.org/docs/v0.2.1/types/Quickshell/PanelWindow/), [PopupAnchor](https://quickshell.org/docs/v0.2.1/types/Quickshell/PopupAnchor/), and [on-demand keyboard focus](https://quickshell.org/docs/v0.2.1/types/Quickshell.Wayland/WlrKeyboardFocus/) contracts. Omarchy’s native widget and style interfaces are documented in the installed shell’s `README.md` and `Commons`/`Ui` components.
