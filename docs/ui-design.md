# A quieter, denser Mluva

The aim is to give a desktop writing task more room for text: smaller proportional type, compact controls, fewer borders and lightly translucent surrounding surfaces. These are product choices for Mluva, not a claim that one desktop style is universally preferred.

## What the research supports

GNOME users repeatedly raise header height and padding as problems on constrained displays. The [headerbar-height discussion](https://discourse.gnome.org/t/how-to-reduce-height-of-gtk-headerbars/3034) also shows why blanket CSS shrinking is brittle: different apps and widgets have different minimum geometry. This is qualitative feedback, not a representative preference survey.

GNOME's own [typography guidance](https://developer.gnome.org/hig/guidelines/typography.html) recommends a consistent type scale and reserves monospace for appropriate content. Its [styling guidance](https://developer.gnome.org/hig/guidelines/ui-styling.html) recommends existing style classes and semantic color variables. The [accessibility guidance](https://developer.gnome.org/hig/guidelines/accessibility.html) calls for keyboard access, sufficient contrast and support for larger text. These principles are compatible with a compact interface; oversized controls and forced monospace prose are not necessary consequences of using GTK.

Community reactions are mixed: some prefer compact, configurable controls, while others value consistent spacing and discoverable actions. Mluva follows the explicit preference for density and restrained styling while retaining visible keyboard focus, named buttons and system scaling. There is no measured claim that this redesign improves reading speed or task completion.

## Applied choices

| Area | Decision |
| --- | --- |
| Typography | Proportional Inter, Adwaita Sans or Noto Sans; a 0.92em application base, 1.04em transcript and 1.45em title. Relative units preserve the system text scale. |
| Layout | A 200–232 logical-pixel sidebar, a wider reading column, compact action chips and a shorter prompt field that grows with input. |
| Surfaces | A 90% opaque application canvas; lightly tinted sidebar and reading surfaces; fewer dividers and no nested prompt fill. Dialogs remain opaque. Desktop compositing determines visible translucency; no blur configuration is installed. |
| Theme | Omarchy's active semantic colors update while the app is open. Elsewhere the app follows system light/dark preference with green accents. Prose stays proportional in both cases. |
| Review widget | Three fixed-height lines using the shell body scale, a lightly translucent surface and native shell controls. |
| Countdown | Eight idle seconds, shown by a shrinking ring on Dismiss. Hover, keyboard focus and menus pause the countdown; rewriting suspends it and completion starts a fresh interval. The full conversation remains accessible from history. |
| Identity | A three-bar voice signal inside a speech mark. The app tile, symbolic icon and repository banner are generated from shared geometry and color tokens. |

The countdown is implemented by one clock that drives both the ring and dismissal. Pointer and keyboard pauses follow Qt's [HoverHandler](https://doc.qt.io/qt-6/qml-qtquick-hoverhandler.html) and [Window active state](https://doc.qt.io/qt-6/qml-qtquick-window.html). The eight-second interval is a product default, not a research-derived threshold.

## Verification boundary

Production GTK and QML are rendered in private X11 sessions with separate D-Bus, accessibility and XDG state. Checks cover light/dark palettes, minimum-width layout, full text, in-progress rewrites, theme changes, countdown expiry and actual pointer/keyboard pauses. A synthetic JSONL model verifies title generation without sending a transcript to a real provider. Physical microphone quality and live Hyprland focus behavior remain separate manual acceptance items.
