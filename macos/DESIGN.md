# Mluva for macOS: design language

Soft-futurist Liquid Glass. Mluva follows macOS 26's own materials and controls, and spends its personality in one place: a living red jelly light that breathes with your voice.

## Authority

1. Apple's Human Interface Guidelines for macOS and the Liquid Glass adoption guide (materials, menu bar, panels, colour, onboarding).
2. The Mluva brand set in `docs/brand`: the glossy red jelly mark and the soft M.
3. The [product contract](../docs/product-contract.md) for what each surface must say.

## Layers

| Layer | Material | Where |
| --- | --- | --- |
| Functional, floating | Liquid Glass (`glassEffect`, regular) in a `GlassEffectContainer` so shapes merge and morph | Recording pill, live transcript card, session wheel ring and centre |
| Functional, system | Whatever macOS gives standard components | Menu bar panel, toolbars, sidebars, Settings, sheets, buttons |
| Content | Opaque system backgrounds and text | History editor, transcripts in History, settings forms |

Glass is never used in the content layer, never stacked on glass, and never applied as decoration. The wheel sits on a 35% black dimming layer because the desktop behind it can be bright (HIG guidance for clear surfaces).

## Colour

- One tint: Mluva red. Light `#E91B27`; dark `#FF4048` for contrast on dark glass. It marks recording, the primary action and the selected session, nothing else.
- Everything else uses semantic colours (`primary`, `secondary`, `separator`), so Dark Mode, Increase Contrast and Reduce Transparency work without special cases.
- Status colours come from the system: green ready, orange attention, secondary unknown.

## Type

- San Francisco through text styles, so Dynamic Type and accessibility sizes apply.
- SF Pro Rounded for the few display moments that should feel soft: the wheel's centre name, the pill's timer and the onboarding title.
- Monospaced digits for elapsed time. No monospace elsewhere: transcripts are prose.

## The jelly light

The recording light is the brand mark brought to life: a red, glossy, slightly irregular blob with a specular highlight, drawn natively. It idles still, swells with the microphone level while recording on a 3.4-second breathing cycle, slows and dims while finalizing, and becomes a check when delivered. Reduce Motion keeps it steady and changes only its opacity. This is the single authored motion moment; everything else uses system transitions.

## Surfaces

- **Menu bar panel.** A compact window: status, the primary Dictate button (glass prominent, red), the agent session control, the latest dictation with Copy, and quiet links to History and Settings. Setup problems appear first, as one sentence with one action.
- **Recording pill.** Bottom centre of the active screen, never taking focus. A capsule with the jelly light, what is happening and where the text will go, the timer, Stop and Cancel. Live words appear in a glass card that grows out of the capsule; at most five lines, newest at the bottom. The agent cancel window shows a draining ring around the light with Keep draft and Cancel.
- **Session wheel.** Full-screen, keyboard-first. Sessions sit on a ring, the first at one o'clock, each a glass capsule with its letter key, status and full name. The remembered session is tinted. The centre names it, says whether it can be sent to or is draft only, and shows the next key to press. Escape or a click outside dismisses it.
- **History.** A standard split view: searchable sidebar by day, an editor that autosaves, the original recognition in a disclosure, and Copy in the toolbar.
- **Settings.** Standard grouped forms in toolbar tabs: General, Speech, Shortcuts, Agents, Personalization, Annotation, Permissions.
- **Welcome.** One window, one permission per step, each with what it enables and a live status; skippable steps are labelled optional.

## Accessibility

Every icon-only control has an accessibility label. The pill and wheel expose their state as text. Reduce Motion and Reduce Transparency are honoured; the wheel is fully operable from the keyboard.
