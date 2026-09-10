# Linux desktop support

**Omarchy is Mluva's primary platform.** The native app, dictation, rewriting and Quickshell widget are tested end to end and used daily by the maintainer. Fedora GNOME compatibility remains available, but has not been tested for several releases; its last accepted desktop was Fedora 44 with GNOME 50.

## Runtime interfaces

Both desktops use GTK 4, Libadwaita, distribution Python/PyGObject, PipeWire and the XDG Global Shortcuts portal. The [setup guide](../linux/README.md#supported-desktop-contract) lists packages. The [combined installer](../README.md#install) also installs the Omarchy widget when the plugin manager is available.

| Capability | Omarchy | Fedora GNOME compatibility |
| --- | --- | --- |
| Native workspace | Themed GTK app follows the active Omarchy palette | GTK app follows the system scheme |
| Recording display | Movable Quickshell window starts with five preview lines, tiles with Super+T and offers completed-note actions | In-window display; optional GNOME extension adds a display-only bottom bar |
| Recording shortcuts | Portal-approved F9 by default, cancellation and latest-conversation actions | Same portal client; actual keys depend on desktop approval |
| Clipboard | Completed dictation and rewrites copy by default | Same; recent desktop behavior unverified |
| Automatic insertion | Experimental and disabled by default | Experimental; historically unreliable in the acceptance setup |

Feature status is tracked separately in the [capability matrix](feature-maturity.md). A working platform does not establish every provider, model or target application's behavior.

## Shortcuts and focus

The [Global Shortcuts portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.GlobalShortcuts.html) owns approval and assignment. Mluva binds recording, cancellation and opening the latest conversation in one session, displays the returned keys, and replaces that session when preferences change. It receives action activations, not arbitrary keystrokes. Escape cancels when Mluva has focus; the approved global cancellation shortcut works elsewhere.

Omarchy's recorder opens without taking typing focus. Clicking it allows interaction; drag its status row or focus it and press Super+T to tile. Floating mode stays above other windows and across workspaces. This requires Quickshell 0.3+ and Hyprland 0.55+. The optional GNOME bottom bar remains display-only and is installed separately.

## Text delivery

Clipboard delivery is the standard workflow. Optional insertion captures an exact AT-SPI target and caret/selection state; password fields, ambiguous targets and stale objects are excluded. A failed restoration falls back to Copy. Native editable-text insertion is preferred; keyboard paste is attempted only when native editing is unsupported. Uncertain delivery never triggers a second insertion attempt.

Target compatibility depends on applications publishing a usable accessibility interface. Browser, terminal and rich-text behavior should be checked in the actual target application before relying on insertion.

Mluva supports explicitly spoken snippets. It stores portable typed-trigger definitions but runs no desktop-wide typed-trigger listener and does not read raw keyboard devices for text expansion.

## Verification

Automated checks run on private displays and buses with synthetic audio, model and input boundaries. They test GTK/QML rendering, portal messages, persistence, cancellation and target restoration without changing the active desktop. [Contributor checks](../CONTRIBUTING.md#verification) describe the commands.

The maintainer's daily Omarchy use supplies the current end-to-end desktop acceptance. Isolated X11 tests complement that use; they do not establish physical microphone quality, real Wayland permissions or compatibility in every application. Fedora requires renewed desktop verification before it can regain the same support claim.
