# Changelog

This file records user-visible Mluva releases. Linux is the first supported distribution target; macOS entries describe a source preview until a signed, notarized, updateable binary is independently verified.

## Unreleased

No changes queued.

## 0.2.0 — 2026-09-08

- Added persistent Codex model and optional Fast mode choices for rewrites in the main workspace and Omarchy widget, using the installed app-server's model catalog. Capture cleanup and automatic titles keep their existing model settings.
- Rewrites request low reasoning when the selected model supports it and display elapsed time to first text. Fast mode discloses increased credit usage; no general latency improvement is claimed.
- Preserved the picker model when its choices are unchanged, avoiding an invalid-object notification during selection changes. Failed settings saves restore the previous choice, unavailable tiers fail explicitly, and failed catalog loads can be retried.
- Replaced outlined scrollbar tracks with transparent tracks and narrow solid thumbs in light and dark appearances.

## 0.1.1 — 2026-09-08

- Expanded the centered Omarchy preview to five lines, with eased scrolling, advance room near a line ending, and stable wrapping across the bounded text limit.
- Unified translucent main-window surfaces, flat controls, symbolic icons and aligned history/conversation layouts. Live dictation updates preserve the text buffer and respect manual scrolling.

## 0.1.0 — 2026-09-07

- Added a compact conversation workspace with proportional typography, lightly translucent surfaces, searchable history and streaming rewrites. Omarchy colors update with the desktop theme.
- Added automatic titles for new dictations and pasted conversations, with local fallback labels, protection for manual renames and an Incognito cancellation boundary.
- Added an eight-second countdown to Omarchy's completed-note review controls, with pauses for pointer, keyboard, menus and rewriting.
- Replaced the app icon and repository presentation with a shared voice-signal identity, current screenshots and a shorter installation and feature guide.

### Initial Linux implementation — 2026-08-21

### Fedora Linux

- Added a native GTK 4 and Libadwaita client supported on Fedora 44, GNOME Shell 50, and Wayland.
- Added portal-approved global recording with F9 by default, configurable F1–F24 alternatives, and a separately approved cancellation shortcut without reserving Right Alt or AltGr.
- Added Dictation, reviewed Command editing, persistent Notes, explicit microphone-plus-system-audio Meeting capture, History, Incognito, retention controls, dictionary replacements, snippets, vocabulary suggestions, and saved styles.
- Added ElevenLabs Scribe v2 realtime recognition with controlled batch fallback and optional faithful cleanup through a locally authenticated Codex app-server.
- Added exact-target AT-SPI restoration, native Unicode insertion, clipboard-first recovery, and an optional narrowly scoped keyboard helper for targets without an editable accessibility interface.
- Added an in-window live recording surface and an optional display-only GNOME Shell bottom bar that disappears at every terminal capture state.
- Added transactional installation with rollback, guarded uninstallation, compatibility aliases for earlier command names, owner-only local state, privacy-safe diagnostics, and recovery paths that do not repeat recognition or delivery implicitly.
- Added one generated feature-maturity registry shared by the Linux UI and public description. Recording and transcription, recording setup controls, History, and custom saved styles are manually verified; every other capability is labeled Experimental, including the known unreliable automatic-paste path.

### macOS source preview

- Renamed the displayed application and bundle artifact to Mluva while preserving the executable, bundle identifier, Application Support directory, signing identity, and notarization profile needed for upgrade continuity.
- Retained the existing Apple Speech and Google Cloud recognition implementations, deterministic transcript pipeline, accessibility-native delivery, Meeting capture, personalization, recovery, and test suite as source-preview functionality.

### Verification boundary

- Verified 285 Linux tests, lint, formatting, a production-only staged environment, launch without Codex, a real private-bus GlobalShortcuts peer, cross-process Czech Unicode insertion on a private AT-SPI bus, responsive GTK renders, and preparing, recording, and quiet overlay states in a headless GNOME Shell virtual monitor.
- Did not claim a macOS binary release because Swift compilation, signing, notarization, clean-machine installation, update delivery, and supported-architecture execution were not run on this Fedora host.
