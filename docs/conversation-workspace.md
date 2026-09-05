# Dictation and rewrite conversations

Mluva's primary Linux workflow is dictation, rewriting and history. F9 starts and stops recording without opening the main window. Completed dictation copies automatically for manual paste. The conversation shows the complete original transcript and every completed rewrite; no rewriting action silently replaces the clipboard.

Shift+F9 requests the latest conversation through the desktop's Global Shortcuts portal. The desktop approves and may reassign shortcuts. The main window offers Quick Polish, Structured Note, saved prompts, and a multiline prompt for custom rewrites and follow-ups. Pasting text into a new conversation starts the same editing workflow without recording or copying it again.

Quick Polish removes filler words, false starts and accidental repetitions, fixes grammar, and lightly improves phrasing while preserving language, voice and facts. Structured Note leads with a concise summary and organizes the remaining details into bullets. Custom instructions and follow-ups operate on the latest completed version with the original and earlier turns available as context. AI rewrites require review; the app preserves the source and provides an explicit Copy action for every version.

## History and privacy

Existing dictation history supplies the conversation identity. A new SQLite reply table stores complete instructions, replies, model identity and timestamps. Reopening a conversation after restart reconstructs its complete local history. Search covers titles, source text, copied text and rewrite instructions/results before limiting the visible sidebar page. Show more extends the result page.

Deleting or pruning a source also deletes its replies. A completed background rewrite cannot recreate a deleted source. Incognito keeps new text out of the history database, disables rewriting and prompt persistence, and cancels an active rewrite when enabled. Existing audio and history retention settings continue to apply.

Follow-ups replay local context through an isolated Codex app-server transformation instead of relying on a remote durable thread. Each request uses a concrete model resolved from the local app-server, read-only sandboxing, disabled approvals, and instructions prohibiting tools. Rewrites accept up to 120,000 characters of serialized conversation context and 40,000 output characters; exceeding a bound produces an explicit error rather than silently shortening text. The display does not truncate source text. Automatic capture cleanup retains its existing smaller output bound.

## Shell integration

The bundled GNOME Shell extension adds a top-panel Mluva menu with recording, latest conversation, history, settings, open and quit actions. Actions delegate to the application's existing GApplication action group. The bottom bar remains noninteractive and does not change keyboard focus. It shows preparing, recording, processing, copied and error states. Copied feedback remains for five seconds and errors for ten seconds; a new recording cancels the older dismissal timer. The bar clears when the app's bus owner disappears.

Closing the main window hides it while the app and approved shortcuts remain available. Quit Mluva explicitly exits. Meeting, vocabulary, snippets and recovery tools remain accessible through secondary menus and settings.

Install the shell integration with `make linux-recording-overlay-install`, or `mluva-overlay install` from an installed application. A GNOME session that has not discovered a newly installed extension may require one logout/login before it can enable the extension. Automated verification uses a private nested shell and never enables the extension in the live desktop.

## Visual direction

The interface uses neutral reading surfaces, a quiet sidebar, restrained violet actions, a consistent sans-serif type scale and visible keyboard focus. The rounded lowercase m has two open arches and a trailing speech stroke. Its colored app tile and monochrome panel mark share one path, with no font or image dependencies. Keep at least one stroke width of clear space around the mark and use the monochrome variant on the panel. The symbol is intended to remain readable at 16 pixels; the app tile is used from 24 pixels upward.

Visual guidance was fetched from the [logo-design skill](https://github.com/atypica-ai/marketing-skills/blob/main/skills/logo-design/SKILL.md) and [Anthropic frontend-design skill](https://github.com/anthropics/skills/blob/main/skills/frontend-design/SKILL.md), together with [Apple's app-icon guidance](https://developer.apple.com/design/human-interface-guidelines/app-icons) and [sidebar guidance](https://developer.apple.com/design/human-interface-guidelines/sidebars). These inform visual craft; GNOME remains the runtime. Codex transport follows the [official app-server documentation](https://learn.chatgpt.com/docs/app-server) and is tested against an independent synthetic server process.
