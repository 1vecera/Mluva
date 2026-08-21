# Mluva launch kit

This kit keeps public copy inside the evidence available on 2026-08-21. Linux is the initial supported release; macOS remains a source preview until a signed, notarized, updateable binary is independently verified.

## Product copy

**One line:** Mluva is open-source AI dictation for Fedora GNOME that shows what it is hearing, preserves raw recognition, and keeps every completed result recoverable on the clipboard.

**Short description:** Press F9 once to start and again to stop. Mluva streams microphone audio to ElevenLabs Scribe v2, shows a transient waveform and volatile transcript, optionally cleans committed text through a locally authenticated Codex app-server, and keeps the completed result recoverable on the clipboard. Recording and transcription, recording setup controls, History, and custom saved styles are verified on Linux. Command, Notes, Meeting, automatic paste, and every other capability are visibly labeled Experimental until manually accepted.

**Support boundary:** Fedora 44, GNOME Shell 50, and Wayland are the first supported public target. Automatic paste is an experimental known limitation, is disabled by default, and is not reliable in the current Fedora acceptance setup; inaccessible, ambiguous, and unsuccessful targets remain copy-only. The optional GNOME Shell recording bar is display-only, Experimental, and requires an explicit install. The complete status lives in the generated [feature-maturity matrix](feature-maturity.md).

## Screenshot assets

| Asset | Recommended use | Alt text |
| --- | --- | --- |
| [`assets/mluva-capture.png`](assets/mluva-capture.png) | README hero, product overview, or the first LinkedIn image | Mluva Capture page showing Dictate, Command, and Notes modes, F9 global delivery, private recovery, recent captures, and the primary record action |
| [`assets/mluva-recording.png`](assets/mluva-recording.png) | Second LinkedIn image or recording-state detail | Mluva recording with a bottom bar showing recording phase, elapsed time, waveform, Dictate mode, copy-only delivery, live transcript preview, and Stop action |

Both images use synthetic content and production GTK widgets rendered at 1280×900 inside fresh X11, D-Bus, XDG, and AT-SPI namespaces. No live desktop, real transcript, user application, clipboard, credential, microphone, or input event appears in them.

## LinkedIn draft

I've built my own desktop dictation app - this time it escaped VS Code :)

It's called Mluva.

Press F9 anywhere on Fedora GNOME, speak, press F9 again. Mluva keeps the full result on the clipboard. It has an experimental automatic-insertion path for accessible text fields, but that does not work reliably for me yet, so I am not calling it done. It fails back to a recoverable copy instead of typing into some random window.

Under the hood: native GTK 4 + Libadwaita, ElevenLabs Scribe v2 Realtime, GNOME Global Shortcuts, AT-SPI, PipeWire, and an optional local Codex app-server for cleanup and spoken commands.

It also has this little status bar at the bottom - recording phase, elapsed time, waveform, mode, recognition route, delivery state and volatile words. The same state can appear over other applications through an explicitly installed display-only GNOME Shell extension. That Shell bar takes no input, keeps focus where it was, and disappears on Stop.

The set I have manually accepted is deliberately smaller: recording and transcription, recording controls, History, and my own saved writing styles. Command, Notes, Meeting, automatic paste, the desktop overlay, dictionary, snippets, Incognito, recovery tools, and everything else are labeled Experimental in the app until they earn their way out.

Linux is the proper first target: Fedora 44, GNOME 50, Wayland. macOS is included as a source preview - no pretending I have a signed, notarized, auto-updating Mac release yet.

Current gate: 285 tests, the exact staged installer and application launcher, a real private-bus shortcut peer, cross-process Unicode insertion plus production GTK pixels in private X11 and AT-SPI sessions, and preparing, recording and quiet states inside a headless GNOME Shell virtual monitor. I could keep using my actual desktop while agents tested the GUI - which is way cooler than watching them steal the mouse.

Code and Fedora install instructions: https://github.com/1vecera/Mluva

Can someone on Fedora GNOME test it properly in GTK, Chrome, Electron, terminals and rich-text editors please? :)

## Claim guardrails

- Say **supported on Fedora 44 with GNOME Shell 50 and Wayland**, not “works on Linux” or “works everywhere.”
- Say **conditional exact-target insertion with copy-only recovery**, not “automatic paste always works.”
- Say **automatic paste is Experimental and is not reliable in the current Fedora acceptance setup**, not “insertion is verified.”
- Keep **Verified on Linux** limited to recording and transcription, recording setup controls, History, and custom saved styles; describe every other capability as **Experimental**.
- Say **ElevenLabs cloud recognition** and **optional locally authenticated Codex processing**; do not imply Linux transcription is offline or fully local.
- Say **macOS source preview**, not “macOS release,” until a current binary passes signing, notarization, packaging, update, and clean-machine checks.
- Do not publish speed, latency, accuracy, privacy, or compatibility percentages until a reproducible corpus or target matrix exists.
