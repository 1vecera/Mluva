# Linux desktop support

**Omarchy is Mluva's primary platform.** The native app, dictation, rewriting and Quickshell widget are tested end to end and used daily by the maintainer. Fedora GNOME compatibility remains available, but has not been tested for several releases; its last accepted desktop was Fedora 44 with GNOME 50.

## Runtime interfaces

Both desktops use native Rust executables, GTK 4, Libadwaita and PipeWire, with no Python runtime. Omarchy supports native Hyprland bindings or the XDG Global Shortcuts portal; GNOME uses the portal. The [setup guide](../linux/README.md#supported-desktop-contract) lists packages. The [combined installer](../README.md#install) also installs the Omarchy widget when the plugin manager is available.

| Capability | Omarchy | Fedora GNOME compatibility |
| --- | --- | --- |
| Native workspace | Themed GTK app follows the active Omarchy palette | GTK app follows the system scheme |
| Recording display | Movable Quickshell window starts with five preview lines, tiles with Super+T and offers completed-note actions | In-window display; optional GNOME extension adds a display-only bottom bar |
| Recording shortcuts | Native Hyprland `mluva-shell global-record` binding or portal-approved F9, cancellation and latest-conversation actions | Portal client; actual keys depend on desktop approval |
| Clipboard | Completed dictation and rewrites copy by default | Same; recent desktop behavior unverified |
| Automatic insertion | Experimental and disabled by default | Experimental; historically unreliable in the acceptance setup |

Feature status is tracked separately in the [capability matrix](feature-maturity.md). A working platform does not establish every provider, model or target application's behavior.

## Shortcuts and focus

The [Global Shortcuts portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.GlobalShortcuts.html) owns approval and assignment. Mluva binds recording, cancellation and opening the latest conversation in one session, displays the returned keys, and replaces that session when preferences change. It receives action activations, not arbitrary keystrokes. Escape cancels when Mluva has focus; the approved global cancellation shortcut works elsewhere.

Native Hyprland bindings can invoke the same global recording path through `mluva-shell global-record`, without a portal session or opening the app window. The bar/menu action `mluva-shell record` remains copy-only. See the [Omarchy binding setup](omarchy-integration.md) before replacing a keyboard binding.

Omarchy's recorder opens without taking typing focus. Clicking it allows interaction; drag its status row or focus it and press Super+T to tile. Floating mode stays above other windows and across workspaces. This requires Quickshell 0.3+ and Hyprland 0.55+. The optional GNOME bottom bar remains display-only and is installed separately.

## Text delivery

Clipboard delivery is the standard workflow. Optional insertion captures an exact AT-SPI target and caret/selection state; password fields, ambiguous targets and stale objects are excluded. On Hyprland, known terminal executables also support content-free window/process capture when no AT-SPI text interface is available. The exact terminal must still be focused immediately before keyboard delivery; Mluva does not activate it after focus moves elsewhere. A failed restoration falls back to Copy. Native editable-text insertion is preferred for text fields; keyboard paste is attempted only when native editing is unsupported. Uncertain delivery never triggers a second insertion attempt.

Firefox uses the guarded keyboard path because its advertised accessibility editing interface can silently ignore mutations. The exact accessible target must remain current. Confirmation accounts for Firefox's UTF-16 caret offsets, so emoji do not cause a false failed-paste receipt.

On startup, focus tracking seeds an existing field only inside one active external window. Focus events then remain authoritative; losing focus does not rediscover a stale field. This allows dictation after a background restart without requiring an extra click.

When system accessibility is enabled, focus tracking requests application-root attributes from external applications at startup and when they join the accessibility desktop. This lets Chromium publish its web tree without opening its accessibility settings. Attribute values are discarded; automatic capture still does not request target text. Exact-target, password and stale-focus guards continue to apply. The [browser verification](verification/browser-direct-transport/README.md#chromium-activated-by-the-focus-tracker) records startup ordering, transport and browser-selection limits.

Text-field compatibility depends on applications publishing a usable accessibility interface. Hyprland terminal capture identifies the window and process, not tabs, panes or shell input mode. Browser, terminal and rich-text behavior should be checked in the actual target application before relying on insertion. The [automatic paste verification](verification/automatic-paste/README.md) records the tested targets and limits.

For a captured terminal target, symbolic keyboard fallback uses `Ctrl+Shift+V`, based on the captured executable rather than the active window title. This selects the clipboard, including on default Foot where `Shift+Insert` selects the primary selection. Other targets use symbolic `Ctrl+V` through `wtype` or X11 `xdotool`. Clipboard and input tools follow the session type, so installed Wayland tools cannot intercept X11 delivery. The physical-keycode `ydotool` fallback is limited to terminal `Shift+Insert` with an explicit clipboard binding; Mluva cannot infer its virtual keyboard's letter layout from another keyboard. Terminals without either supported capture path and ordinary targets with only `ydotool` remain copy-only. An unconfirmed key dispatch is never reported as confirmed insertion.

Ghostty's custom bindings must preserve `Ctrl+Shift+V` for clipboard text and literal `Ctrl+V` for TUI images. The [verified workstation repair](verification/ghostty-format-paste/README.md) also makes Omarchy's Super+V select the image chord only for an advertised image format, with a fresh window/process check before dispatch.

Mluva supports explicitly spoken snippets. It stores portable typed-trigger definitions but runs no desktop-wide typed-trigger listener and does not read raw keyboard devices for text expansion.

## Verification

Automated checks run on private displays and buses with synthetic audio, model and input boundaries. They test GTK/QML rendering, portal messages, persistence, cancellation and target restoration without changing the active desktop. [Contributor checks](../CONTRIBUTING.md#verification) describe the commands.

The maintainer's daily Omarchy use supplies the current end-to-end desktop acceptance. Isolated X11 tests complement that use; they do not establish physical microphone quality, real Wayland permissions or compatibility in every application. Fedora requires renewed desktop verification before it can regain the same support claim.
