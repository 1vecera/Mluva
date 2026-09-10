# Changelog

This file records user-visible Mluva releases.

## Unreleased

- Made Omarchy the primary platform, reflecting daily end-to-end use; Fedora GNOME compatibility remains available but has not been tested in recent releases.
- Added one setup command for desktop dependencies, the native app and the Omarchy plugin, plus a copyable installation prompt for agents.
- Focused the maintained source tree on Linux and simplified user and contributor documentation.

## 1.0.0 — 2026-09-10

- Established Mluva as the sole product identity across source, packages, commands, desktop integrations and storage. Existing installations migrate with ownership checks, private backups and rollback. Desktop permissions may need approval again; see the [upgrade guide](docs/identity-migration.md).
- Made speech and rewrite selection easier to configure, with provider availability, model discovery, connection guidance and progressive endpoint settings.
- Made Live rewrite respond to useful committed speech sooner, retained one compact template control, and kept recording motion and the Live panels steady. Actual response time still depends on the provider and model.
- Added a searchable Ctrl+P command panel and subtle Markdown presentation while preserving editable source, copy, save and follow-up behavior.
- Published the finished 55-second launch film, browser playback with captions, and a clearer repository homepage and installation path.
- Simplified command ownership and contributor navigation, shared local HTTP test fixtures, and added focused checks without removing test coverage.

Published as a Linux source release with a per-user installer. See the current [platform and feature status](docs/feature-maturity.md).

## 0.3.0 — 2026-09-09

- Made original documents and completed rewrites editable, with compact Copy and Save icons, Ctrl+S, and a separate raw recognition record for recovery. Saved edits participate in follow-ups, search and export.
- Enabled automatic copying after completed dictation and rewriting by default. Copy behavior, action visibility, scrolling and the four-second Omarchy review timeout are configurable in Settings and the JSON dotfile.
- Added smooth following with advance room for new text and preservation of manual reading position, plus full-height editable documents and responsive live draft columns.
- Added independent provider choices: native Codex app-server or LiteLLM-compatible rewriting; ElevenLabs Scribe, local Voxtype/Whisper, or LiteLLM-compatible speech recognition. Model and endpoint settings use environment references for secrets.
- Added opt-in Live rewrite with task-spec, structured-note, polish and custom templates. Drafts show missing information while speaking, preserve concurrent manual edits, and save one final version at Stop. Batch speech providers offer provisional chunks before full-audio finalization.
- Verified local Voxtype against public sample audio and transports against controlled HTTP providers. New provider choices and live rewrite remain Experimental pending model-quality and live desktop acceptance; cloud accounts were not each exercised.

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

## Initial Linux implementation — 2026-08-21

- Added a native GTK 4 and Libadwaita client initially accepted on Fedora 44, GNOME Shell 50, and Wayland.
- Added portal-approved global recording with F9 by default, configurable F1–F24 alternatives, and a separately approved cancellation shortcut without reserving Right Alt or AltGr.
- Added Dictation, reviewed Command editing, persistent Notes, explicit microphone-plus-system-audio Meeting capture, History, Incognito, retention controls, dictionary replacements, snippets, vocabulary suggestions, and saved styles.
- Added ElevenLabs Scribe v2 realtime recognition with controlled batch fallback and optional faithful cleanup through a locally authenticated Codex app-server.
- Added exact-target AT-SPI restoration, native Unicode insertion, clipboard-first recovery, and an optional narrowly scoped keyboard helper for targets without an editable accessibility interface.
- Added an in-window live recording surface and an optional display-only GNOME Shell bottom bar that disappears at every terminal capture state.
- Added transactional installation with rollback, guarded uninstallation, compatibility aliases for earlier command names, owner-only local state, privacy-safe diagnostics, and recovery paths that do not repeat recognition or delivery implicitly.
- Added one generated feature-maturity registry shared by the Linux UI and public description. The initial verified set covered recording, setup, History and custom styles; automatic paste remained Experimental.
