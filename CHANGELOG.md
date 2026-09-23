# Changelog

This file records user-visible Mluva releases.

## 1.5.0 — 2026-09-23

- Added **Rename**, **Merge with…** and **Delete…** directly to each conversation's sidebar menu.
- Made conversation titles editable with a click. Enter or the checkmark saves; Escape or Cancel discards the rename. Manual titles take precedence over automatic titles without disturbing document edits or unsent prompts.
- Added searchable conversation merging. The destination keeps its title, working text and prompt drafts are combined, and original recordings plus saved rewrites remain recoverable in History and exports.
- Added confirmation before merging or deleting. Deletion covers the whole combined conversation and retained recordings; active recording, rewriting, recovery and Incognito prevent sidebar merge/delete operations.

The compatible Omarchy widget remains version 0.5.0. This release changes the native app; existing settings and conversations are preserved.

## 1.4.0 — 2026-09-22

- Added **Continue recording** in the app and **Continue** in the finished Omarchy widget. New speech appends to the same conversation, with the previous text visible throughout. History shows one combined conversation and keeps individual raw recordings available for recovery.
- Added three Live rewrite modes: **Off**, **Once** for one recording, and **Continuous** across recordings. Existing enabled installations retain Continuous behavior.
- Added rewrite thinking levels. Codex choices follow the selected model's catalog; compatible providers use advertised choices or explicit overrides, with a provider-default option.
- Fixed the completed widget's dismissal countdown being held by inherited keyboard focus. Deliberate interaction, hovering, menus and active rewrites still pause it.

The matching Omarchy widget is version 0.5.0. Update the app and widget together to use Continue.

## 1.3.0 — 2026-09-21

- Fixed desktop startup when a legacy managed credential profile references a retired 1Password item. Enabled local credential snapshots now take precedence and load only the supported speech token; existing process credentials and installations without snapshots keep their existing paths.
- Updated the app introduction with Daniel's narration and inline GitHub playback.

The matching Omarchy widget remains version 0.4.0. This release retains the security fixes from 1.2.1.

## 1.2.1 — 2026-09-21

- Restricted native Codex rewrites to text-only sessions without tools, inherited instructions or integrations. Unsupported isolation capabilities fail closed; existing Codex sign-in is preserved. Setup now installs Bubblewrap for masking global instruction files.
- Moved Incognito dictation, meeting and batch-preview audio into private memory-backed storage, with cleanup after app exit or a crash and no disk fallback. Operating-system swap and crash dumps remain outside Mluva's control.
- Prevented ElevenLabs uploads from following redirects that could disclose credentials or recordings.
- Made scratchpad, settings and personalization files private before content is written, with atomic replacement and owner-only application data directories.
- Rebuilt the local Mermaid renderer with patched DOMPurify and lodash-es dependencies, a locked build, checksum, component inventory and license notices.

This source release also includes the changes prepared for 1.2.0 below. The matching Omarchy widget remains version 0.4.0. Native Codex isolation was verified with Codex 0.155.1; older unsupported protocols fail closed.

## 1.2.0 — 2026-09-18

- Introduced the final Mluva identity: a glossy red mark beside the Mluva wordmark with its soft M. The launcher tile, GNOME panel icon, README, site and documentation use it. The complete asset set, with three lockup ratios, marks, icon PNGs and a favicon `.ico`, lives in [docs/brand](docs/brand/README.md) and regenerates from committed sources.
- Made every prompt editable through Settings → Prompts and readable override files in `~/.config/mluva/prompts/`, with Ctrl+P deep links, staged restore, invalid-file fallback and external-edit conflict detection. Custom and saved-style text remains a lossless recovery baseline. A recording keeps its prompt set; saved changes apply to the next recording.
- Stabilized Live dictation: new words appear immediately at full opacity, a fresh recording starts at the top, the Live draft pane is present from startup, and deliberate edits survive restarts within the same capture. The recording light moves between fixed circular endpoints with fluid deformation between them.
- Refined the workspace: one JetBrains Mono typeface, full-window provider setup and settings, wider collapsible Live panes, a translucent command list with visible shortcuts, and recorder updates that fade only changed words and keep large corrections visible without reversing automatic scrolling.
- Unified Live templates into single declarations and centralized rewrite provider and model policy; automatic titles run in a separate bounded queue with the same privacy, manual-naming and deletion gates. Prompt identities, defaults and generated prompts are preserved.
- Trimmed the test suite to 451 focused cases and documented when a test earns its place.

The matching Omarchy widget remains version 0.4.0.

## 1.1.0 — 2026-09-11

- Enable, pause and change Live rewrite templates while dictating. Grilling is the default for new preferences, with unanswered questions pinned above notes that grow from supplied intent, constraints, preferences and technologies. Existing template choices are preserved and Live remains opt-in.
- Render Mermaid sketches locally in live and saved drafts, retaining exact Markdown for editing, copying and export. Incomplete, invalid or unsupported diagrams remain readable source. The installer now includes WebKitGTK 6.0.
- Give the floating recorder bottom-left, bottom-center and bottom-right presets, a bare light/timer header and a smoother inner/outer recording pulse. Match the app's monospace font to the widget, hide history by default and reduce visual chrome.
- Browse Live and saved conversations without incoming speech stealing selection. Add compact weekday/day/month dates, a 12/24-hour preference and Ctrl+P access to every settings row.
- Preserve manual edits across Live changes, cancel paused requests and save a paused draft as not reconciled without automatic copying. Batch speech previews retain audio captured before Live was enabled and make no preview requests while Live is off.
- Made Omarchy the primary platform, reflecting daily end-to-end use; Fedora GNOME compatibility remains available but has not been tested in recent releases.
- Added one setup command for desktop dependencies, the native app and the Omarchy plugin, plus a copyable installation prompt for agents.
- Focused the maintained source tree on Linux and simplified user and contributor documentation.

The matching Omarchy widget is version 0.4.0. See the [verification report](docs/verification/fluid-workspace.md) for reproducible checks and remaining live-desktop/provider limits.

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
