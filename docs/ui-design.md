# A quieter Mluva workspace

Mluva gives speaking and editing more room: JetBrains Mono throughout, compact controls, full-width text panes and lightly translucent surrounding surfaces. The [original Figma kit](design/video-kit/README.md) retains shared variables, editable masters and motion studies; the current [app intro](../launch-video/README.md) uses native desktop footage.

| Area | Applied decision |
| --- | --- |
| Typography | Bundled JetBrains Mono Regular/Medium/Bold/Italic. GTK uses relative sizing for system text scale; the recorder uses 14-pixel text with 22-pixel leading. Mermaid receives the same bundled font. |
| Layout | A 224-pixel optional sidebar. One editor uses the available width and adapts to compact windows. |
| Surfaces | The application and recorder use an 82% opaque canvas. Menus and Commands use 94% surface opacity with full-opacity text. No desktop blur configuration is installed. |
| Controls | Flat controls, quiet icons and visible hover, selection and keyboard-focus states. Commands uses a simple result list without nested card boxes. |
| Theme | Active Omarchy semantic colors update while the app is open. Other desktops use the system light/dark preference. The three palettes shown in Figma are examples; the app reads any installed Omarchy palette. |
| Recorder | A 500×150 baseline with a 20-pixel status header, zero gap to the pane, 10-pixel pane padding and five visible lines. The elapsed time ends at the right stroke; the peak filled breathing contour touches the left stroke. The window closes after delivery. |
| Window behavior | Drag the status row or transcript, resize, or use Super+T to tile. Floating mode remains above windows. Opening preserves typing focus; deliberate interaction can take focus. |
| Recording light | A stable-center harmonic contour expands and contracts on a 3.4-second cycle. Preparation/finalization do not pulse; reduced motion keeps a steady light. Accessible state names remain available. |
| Markdown | Native text tags preserve exact source text while styling headings, emphasis and code. Mermaid uses a local renderer and embedded font. Copy and Save use the source rather than visual animation remnants. |
| Commands | Ctrl+P searches actions and Settings-prefixed controls. Ctrl+Shift+C copies; Ctrl+S saves; Ctrl+H opens History; Ctrl+B toggles the sidebar; Ctrl+, opens Settings. |
| Welcome and Settings | Full-window routes. Welcome reuses real provider/model controls and persists explicit completion. Settings has a page selector; Escape returns to the workspace. Automatic pasting requires the same verified capture target and has no preference switch. |
| Identity | The glossy red mark and soft-M wordmark from [docs/brand](brand/README.md). App tile, panel symbol and repository assets are composed from that set; see the [brand contract](brand-and-compatibility.md). |

## Text and scrolling

Recognition compare previous/current text locally. The exact new text is available immediately; changed old runs fade for 120 ms and inserted runs for 220 ms. Unchanged middle text remains stable. Bounded diff work limits pathological replacement cost. Editing, focus changes and unmapping remove visual remnants.

Recent character growth estimates speech rate over four seconds and reserves room for a likely next line. Automatic scrolling moves only forward. Large corrections retain the new tail at the existing reading origin with temporary space above it; manual navigation releases that space and preserves readable content. Bounded recorder previews carry indentation and discarded-line offsets so removing a prefix does not change wrapping. Reduced motion keeps text and geometry while disabling animation.

## Verification boundary

Native GTK and QML checks run in private X11 sessions with separate D-Bus, accessibility and XDG state. They cover minimum layout, exact Unicode text, intermediate scroll frames, continuous wrapping, large corrections, manual reading, theme changes and reduced motion. A disposable Omarchy ARM VM verifies actual compositor window identity, floating/tiling, palettes/borders and the WebKit Markdown/workspace flow. Examples are synthetic and do not establish microphone quality, provider latency or every target editor’s insertion behavior.
