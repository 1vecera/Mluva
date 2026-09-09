# Independent usability and accessibility review

No confirmed blocker remains in the reviewed changes after the accessible-name fixes.

The original private AT-SPI tree exposed an unnamed command-search entry, an unnamed read-only Dictation view, and a Live draft named only by its generic editing tooltip. Root added explicit labels. A fresh private run confirms the final names are Search actions, Dictation, and Live draft; the source is read-only and the draft editable. The recording status exposes Recording, and its button exposes Recording Stop.

Private X11 keyboard input verifies Ctrl-P from the native Markdown editor, search selection with Down, Tab into the selected command row, native Down moving focus and selection together, and native Enter dispatching Polish exactly once. Dialog dismissal restores focus to the previous DocumentEditor. Tab moves directly from the Dictation pane to the editable Live draft.

Actual minimum-window pixels were inspected at an outer 420 by 520 surface (410 by 510 app content). The command panel fits, search and the keyboard hint remain visible, and the Live panes have independent scroll gutters with a visible draft focus border. Existing minimum Live/Markdown screenshots, the narrow command screenshot, the minimum scrolling receipt, and native copy/save test sources were also inspected; the reviewed fields remain reachable and show restrained Markdown emphasis.

The suspected header Close/Enter conflict was not established as a user-reachable failure. Native Tab and F6 did not focus Close in this libadwaita fixture, and its grab_focus call returned false. Consequently the command-final receipt's attempted header test kept search focus; its fake recording call is ordinary selected-command dispatch, not evidence of a Close-button bug. Root nevertheless bounded custom Enter and arrow handling to the search subtree; the focused-row path was then verified independently.

Evidence: commands-final/commands-a11y.json, commands-final/receipt.json, commands-final/commands-minimum.png; live-a11y-final/live-a11y.json, live-a11y-final/receipt.json, live-a11y-final/live-minimum.png, and live-a11y-final/live-draft-focused.png. Earlier command-minimum and command-header exploratory runs are not final acceptance evidence.

All application runtime checks used the supported isolated X11 runner, explicit display206, private session and AT-SPI buses, separate accessibility client process, unique XDG state, synthetic local text, and disabled capture/provider/global-target startup. Initial accessibility traversal exceeded the fixture's six-second timeout; the bounded final traversal completed with a private registry. Private portal startup reported missing Wayland compositor services; no host compositor, microphone, input target, provider, secret, or clipboard was used. Live Wayland focus, physical shortcuts, screen-reader speech, portals, and real target-app delivery remain untested.
