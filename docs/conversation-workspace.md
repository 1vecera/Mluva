# Dictation workspace

Mluva turns speech into text and delivers it to the selected destination. F9 starts and stops ordinary dictation. When the original target is still valid, delivery pastes automatically; otherwise the complete text remains on the clipboard. App and widget buttons start copy-only capture. F8 holds recording for the selected Herdr agent, then offers the existing two-second cancel window before sending. The target is frozen at capture start; uncertainty never triggers an automatic resend.

The workspace has one editor. Live recognition follows new text smoothly while respecting deliberate scrolling and the system's reduced-motion preference. Copy and Save icons are always present. Copy uses the current editor contents, including unsaved changes; Save stores working edits without changing raw recognition or the clipboard. Ctrl+S saves and Ctrl+Shift+C copies. Continue recording appends speech to the current conversation while preserving its original and earlier recordings.

Polish, manual and live rewriting, prompts/templates, generated titles, Notes and Command modes have been removed. Use the destination agent's context for those transformations. Existing saved rewrites remain readable, editable and exportable. Titles use a local excerpt and can be renamed.

## History and recovery

The sidebar supports search, rename, merge and deletion. Pending document edits survive navigation. Merge preserves originals, saved replies, recording metadata and retained audio under the destination conversation. Deletion asks for confirmation and removes the owned history, replies and retained files. Active capture or recovery prevents conflicting operations.

An unfinished legacy Notes draft recovers once into History, preserving its working text separately from immutable recognition. Existing newer edits win; a completed migration or later deletion cannot replay the draft. Malformed recovery files remain untouched with an explicit error. Private legacy drafts and their managed audio are erased instead of becoming saved history.

Incognito keeps new text out of History and account sync. Save is unavailable for private text, while Copy remains available. Retention settings still govern ordinary recordings and failures. The independent Meeting recorder remains available.

## Settings and shortcuts

Settings contains Workspace, Providers, Capture, Audio, Privacy, Feature maturity, Advanced and Account pages. Speech selection remains independent of optional sign-in; local models work offline. Automatic paste, Copy/Save visibility and smooth following have no preference switches. Ctrl+P opens the action search; Shift+F9 opens the latest conversation; Ctrl+H opens History; Ctrl+B toggles the sidebar; Ctrl+, opens Settings.

The [Omarchy recorder](omarchy-integration.md) shows only active recording/finalization. It closes after delivery. Screenshots and narrated annotations remain local attachments; there is no built-in model request for them.

## Verification

Run `make linux-test linux-application-test linux-conversation-test linux-text-target-test linux-agent-voice-test`. Private GTK checks use synthetic speech, separate buses and private storage. They do not establish physical microphone quality, live compositor behavior or every editor's insertion support. Current evidence and remaining limits belong in [the simplification report](verification/dictation-only/README.md); older frozen pixel comparisons describe their original releases.
