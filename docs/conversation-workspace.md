# Dictation and rewrite conversations

Mluva's primary Linux workflow is dictation, rewriting and history. F9 starts and stops recording without opening the main window. Completed dictation copies automatically for manual paste. The workspace shows an editable original and every completed rewrite. Both dictation and successful rewrites copy automatically by default; Settings → Workspace can disable either behavior. Compact Copy and Save icons can be hidden independently. Ctrl+S saves edits, and requesting a rewrite saves pending edits first. Raw recognition stays available in History for recovery.

Live dictation follows the newest line. Completed notes and rewrites open at their end, with the rewrite controls available after recording finishes. The full text remains selectable and reachable by scrolling up; shortened shell previews keep the newest words.

Rewrites stream into the matching conversation and Omarchy widget as text arrives. The incomplete reply stays in memory and cannot be copied through the rewrite controls. Completion saves one full reply and optionally copies it. Cancellation, failure, deletion, and Incognito discard the partial display and reject late updates. Manual reading position survives same-document updates; smooth scrolling, animation duration and advance room are configurable.

The model menu beside the rewrite actions lists models from the chosen provider: the installed Codex app-server or a LiteLLM-compatible endpoint. Settings → Providers selects rewriting independently from speech recognition; see [provider and workspace configuration](providers-and-live-rewrite.md). For Codex, model and Fast mode choices persist for rewrites only, including rewrites requested from the Omarchy widget. Default follows the existing configured Codex model, or the catalog default when none is configured; capture cleanup and automatic titles keep their own existing model setting. Each request freezes its choices before starting. Missing models fail without silently substituting another model.

Fast mode is off by default, discloses increased Codex credit usage, and is available only when the selected model advertises a Fast service tier. Choosing a model without Fast clears that choice. A stale saved Fast choice is rejected before sending text if the refreshed catalog no longer supports it. Turning Fast off explicitly requests standard speed even when the user's general Codex configuration enables Fast. Rewrite turns request low reasoning when supported, otherwise the model's advertised default effort. These overrides do not edit the user's Codex configuration.

Completed rewrites show the elapsed time from worker dispatch to the first nonempty text delta, including process startup and model discovery. This timing is session-only. It is not a guarantee of provider latency; the [synthetic latency measurements](rewrite-latency.md) describe the current evidence and a repeatable opt-in benchmark.

Shift+F9 requests the latest conversation through the desktop's Global Shortcuts portal. The desktop approves and may reassign shortcuts. The main window offers Quick Polish, Structured Note, saved prompts, and a multiline prompt for custom rewrites and follow-ups. Pasting text into a new conversation starts the same editing workflow without recording or copying it again.

Quick Polish removes filler words, false starts and accidental repetitions, fixes grammar, and lightly improves phrasing while preserving language, voice and facts. Structured Note leads with a concise summary and organizes the remaining details into bullets. Custom instructions and follow-ups operate on the latest completed version with the original and earlier turns available as context. AI rewrites require review. You can edit and save either source or result directly; later rewrites use these saved working versions, with raw recognition retained separately. Copy uses the current editor contents, including unsaved changes; Save persists text without invoking a provider or changing the clipboard.

## History and privacy

Existing dictation history supplies the conversation identity. SQLite tables store the editable working source, complete instructions, replies, model identity and timestamps. Raw recognition and its original provenance are not overwritten by document editing. Reopening a conversation after restart reconstructs its complete local history. Search covers titles, source text, copied text and rewrite instructions/results before limiting the visible sidebar page. Show more extends the result page.

Deleting or pruning a source also deletes its replies. A completed background rewrite cannot recreate a deleted source. Incognito keeps new text out of the history database, disables rewriting and prompt persistence, and cancels an active rewrite when enabled. Existing audio and history retention settings continue to apply.

Follow-ups replay saved working context through the chosen provider. Codex uses an isolated app-server transformation with a concrete model, read-only sandboxing, disabled approvals, and instructions prohibiting tools. LiteLLM receives plain chat-completion messages with no tools enabled and rejects incomplete responses or tool requests; the configured server owns its retention policy. Rewrites accept up to 120,000 characters of serialized conversation context and 40,000 output characters; exceeding a bound produces an explicit error rather than silently shortening text. The display does not truncate source text. Automatic capture cleanup retains its existing smaller output bound.

## Automatic conversation titles

Each newly completed dictation or pasted conversation receives a short local label immediately. When automatic titles are enabled (the default), one background request to the chosen rewrite provider at a time generates a title in the source language from up to 6,000 characters of the original. The title is limited to 64 characters. Existing history is not sent to the model on startup; untitled older conversations display a local text label.

Capture and copying never wait for a title. A missing, slow or invalid provider leaves the local label in place; there is no automatic retry. At most twenty titles wait behind the active request; excess completions retain their local labels. Manual renames, including clearing a title or saving the same text, increment a revision that prevents a late automatic title from replacing them. Deletion cannot be undone by a title response. Labels update without moving the selected note, scrolling the conversation, clearing a prompt draft or rebuilding a rename editor.

Settings → Capture → Behavior can disable model-generated titles. Incognito cancels pending requests, clears the queue and discards late results. Turning it off does not replay missed conversations. Retention and deletion apply to titles with their source records.

## Shell integration

Omarchy has an optional [Quickshell plugin](omarchy-integration.md) with explicit recording controls, a centered translucent five-line preview with eased scrolling, and direct rewrite actions after dictation. Its completed-note controls dismiss after four idle seconds by default, with an animated ring and pauses for hover, keyboard focus, menus and rewriting; recording itself stays noninteractive. The main window uses aligned pane headings and one column for live text, saved messages, rewrite controls and the recording footer. Manual scrolling suspends following until the reader returns to the bottom. This integration remains Experimental pending live Hyprland acceptance.

The bundled GNOME Shell extension adds a top-panel Mluva menu with recording, latest conversation, history, settings, open and quit actions. Actions delegate to the application's existing GApplication action group. Its bottom bar remains noninteractive and does not change keyboard focus. It shows preparing, recording, processing and errors; a completed saved dictation hides that bar while Omarchy offers conversation controls. Other capture modes retain five-second copied feedback, and errors remain for ten seconds. A new recording cancels the older dismissal timer. The bar clears when the app's bus owner disappears.

When the extension attaches or is re-enabled, it requests the last bounded display snapshot through the same application action group. Processing status therefore returns immediately without restarting capture or touching the clipboard.

Closing the main window hides it while the app and approved shortcuts remain available. Quit Mluva explicitly exits. Meeting, vocabulary, snippets and recovery tools remain accessible through secondary menus and settings.

Install the shell integration with `make linux-recording-overlay-install`, or `mluva-overlay install` from an installed application. A GNOME session that has not discovered a newly installed extension may require one logout/login before it can enable the extension. Automated verification uses a private nested shell and never enables the extension in the live desktop.

## Visual direction

The workspace uses smaller proportional type, compact action chips, a wider reading area and lightly translucent surfaces. Omarchy supplies the active light/dark palette; other desktops use a neutral green palette. The voice-signal mark, app tile and repository banner share token-generated SVG geometry. The symbolic variant is used on the panel; the app tile starts at 24 pixels. See the [UI design notes](ui-design.md) for research, typography, spacing and transparency choices.

## Verification

Run `make linux-test linux-shortcut-test linux-text-target-test linux-conversation-test linux-live-rewrite-test linux-overlay-test`. The conversation check reuses the private X11, D-Bus and accessibility runner. It exercises the production GTK application against an independent synthetic Codex subprocess, with private settings/history and no microphone, live desktop or provider requests. Screenshots and receipts remain under `tmp/conversation-smoke/` for the full conversation, empty, recording, processing, rewriting, error, Incognito and dark appearances. It also checks the 420 × 520 minimum window for clipping.

The live workspace check covers source/reply editing, raw recovery, final-only automatic copy, icon settings, long editor allocation, narrow live layout, manual-edit races, template gaps, final-tail persistence and cancellation. Lifecycle checks also cover contextual follow-ups, explicit Copy, imports, drafts before presets and during navigation, cancellation, failed Incognito setting persistence, complete JSON/Markdown export, correction/rename synchronization, management beyond the newest 100 entries, and startup/delivery status. Unit tests cover restart persistence, search across older sources and replies, context bounds, deletion and late reply rejection. The nested shell check verifies all five application actions and preparing/recording/quiet/processing/copied/error states without enabling the extension in the live shell.

These checks establish behavior under controlled boundaries. Real microphone recognition quality, physical Wayland shortcut approval and the new shell integration still need live acceptance; the new capabilities remain Experimental. Automatic paste remains optional and retains its existing known limitations.
