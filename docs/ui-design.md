# A quieter, denser Mluva

The aim is to give a desktop writing task more room for text: smaller proportional type, compact controls, fewer borders and lightly translucent surrounding surfaces. These are product choices for Mluva, not a claim that one desktop style is universally preferred.

## What the research supports

GNOME users repeatedly raise header height and padding as problems on constrained displays. The [headerbar-height discussion](https://discourse.gnome.org/t/how-to-reduce-height-of-gtk-headerbars/3034) also shows why blanket CSS shrinking is brittle: different apps and widgets have different minimum geometry. This is qualitative feedback, not a representative preference survey.

GNOME's own [typography guidance](https://developer.gnome.org/hig/guidelines/typography.html) recommends a consistent type scale and reserves monospace for appropriate content. Its [styling guidance](https://developer.gnome.org/hig/guidelines/ui-styling.html) recommends existing style classes and semantic color variables. The [accessibility guidance](https://developer.gnome.org/hig/guidelines/accessibility.html) calls for keyboard access, sufficient contrast and support for larger text. These principles are compatible with a compact interface; oversized controls and forced monospace prose are not necessary consequences of using GTK.

Community reactions are mixed: some prefer compact, configurable controls, while others value consistent spacing and discoverable actions. Mluva follows the explicit preference for density and restrained styling while retaining visible keyboard focus, named buttons and system scaling. There is no measured claim that this redesign improves reading speed or task completion.

## Applied choices

| Area | Decision |
| --- | --- |
| Typography | Proportional Inter, Adwaita Sans or Noto Sans; a 0.92em application base, 1.04em transcript and 1.12em pane headings. Relative units preserve the system text scale. |
| Layout | A 200–232 logical-pixel sidebar, aligned fixed pane headings, and one reading-column width for transcripts, live dictation, the composer and recording controls. History extends to the bottom of the window. |
| Surfaces | An 82% opaque application canvas and floating widget, with transparent content and a slight sidebar tint. The canvas alpha is applied once, so stacked panels do not cancel the translucency. Menus use a 94% opaque surface; dialogs remain opaque. Desktop compositing determines visible translucency; no blur configuration is installed. |
| Controls | Flat controls with two-pixel corners, quiet icons, a transparent symbolic brand mark and explicit hover, selection and keyboard-focus states. Primary actions use a light semantic tint and border. |
| Theme | Omarchy's active semantic colors update while the app is open. Elsewhere the app follows system light/dark preference with green accents. Prose stays proportional in both cases. |
| Review widget | Starts near the bottom center with five lines using the shell body scale and native controls. Drag the status row, resize or use Super+T to tile; floating mode stays above other windows. Opening preserves typing focus while clicking allows interaction. Scrolling eases toward the newest line. |
| Live workspace | Equal-width text panes with a 24-pixel gap at wide sizes, stacked at the existing compact breakpoint. Width no longer follows content. Separate gutters reserve scrollbar space even when a track is hidden. |
| Recording | The main app keeps the light and timer in its existing title bar above the text panes, with Stop below them. A red light grows and shrinks around its six-pixel midpoint on a 2.6-second cycle. Preparation and finalization do not blink; reduced motion keeps a static recording light. Accessible names retain recording state. |
| Markdown | Native text tags provide restrained heading, emphasis and code styling. Focus reveals the unchanged source; Copy and Save use that source. No HTML, fetching or clickable-link execution is introduced. |
| Commands | Ctrl+P and the app menu open one native searchable list of existing actions. Selection remains visible during keyboard navigation; dismissal preserves the recording state. Unavailable actions remain discoverable. |
| Countdown | Four idle seconds by default, shown by a shrinking ring on Dismiss. Hover, keyboard focus and menus pause the countdown; rewriting suspends it and completion starts a fresh interval. Settings can change the delay; the full conversation remains accessible from history. |
| Identity | A flat flowing-m mark refined from the selected generated source, with Mluva set in outlined Adwaita Sans SemiBold. The app tile, symbolic icon and repository banner share geometry; light/dark media variants and provenance are in the [brand contract](brand-and-compatibility.md). |

The countdown is implemented by one clock that drives both the ring and dismissal. Pointer and keyboard pauses follow Qt's [HoverHandler](https://doc.qt.io/qt-6/qml-qtquick-hoverhandler.html) and [Window active state](https://doc.qt.io/qt-6/qml-qtquick-window.html). The four-second interval is a configurable product default, not a research-derived threshold.

The preview uses Qt's [SmoothedAnimation](https://doc.qt.io/qt-6/qml-qtquick-smoothedanimation.html) to follow changing targets without restarting from a hard step. A bounded text-position counter preserves wrapping when older words leave the preview. The GTK conversation uses the native animation setting, applies recognition updates only to the changed text suffix, and suspends following when the reader scrolls away from the bottom. Its configurable lookahead adds proportional space once the last line is 72% full, moving toward the next line before wrapping occurs. The pulse keeps one stable center and releases animation callbacks when hidden.

## Verification boundary

Production GTK and QML are rendered in private X11 sessions with separate D-Bus, accessibility and XDG state. Checks cover light/dark palettes, minimum-width layout, alignment, full text, intermediate scrolling frames, manual reading positions, bounded Unicode previews, theme changes, countdown expiry and actual pointer/keyboard pauses. GTK render textures verify canvas transparency directly. A synthetic JSONL model verifies title generation without sending a transcript to a real provider. These isolated checks complement daily Omarchy use; they do not establish physical microphone quality or every target application’s behavior.
