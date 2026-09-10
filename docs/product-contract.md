# Product behavior and privacy

Mluva turns speech or existing text into editable drafts while retaining the original recognition. Omarchy is the primary, daily-used platform; [desktop support](linux-platform-profile.md) and [feature status](feature-maturity.md) describe the supported workflow and experimental capabilities.

## Recording and rewriting

The recording shortcut toggles capture. A second press during preparation cancels before recording starts. Mluva checks the selected input and required provider readiness before opening the microphone. PipeWire audio is captured as private 16 kHz, 16-bit mono PCM.

Scribe displays live recognition but only committed text becomes raw history or final dictation delivery. If realtime recognition cannot start or finalize, the app reports the batch fallback explicitly. Batch providers normally recognize at Stop; Live rewrite can request provisional chunk previews before final recognition.

Raw recognition, edited source, completed rewrites and delivered text remain separate. Saving an editor changes its working version, not the raw record. Rewrites use the current saved version and conversation context. Partial replies remain in memory and cannot be copied as completed results. Failure, cancellation, deletion and Incognito invalidate late responses.

Live rewrite is opt-in. It sends provisional recognition to the rewrite provider, preserves manual edits and reconciles against the complete committed transcript at Stop. Templates mark missing information, but model output still needs review. See [Live rewrite](providers-and-live-rewrite.md#live-rewrite).

Command and Notes modes retain an explicit preview/accept step. Meeting mode explicitly records the selected microphone and output sink's monitor; system audio means everything playing through that sink. Partial-source failures are reported. Meeting remains Experimental.

## Providers and credentials

Speech and rewriting are selected independently. Cloud speech sends audio to its provider; rewriting and automatic titles send text to the chosen rewrite provider. Local Voxtype/Whisper keeps recognition local, but does not determine where other processing happens.

Codex uses an authenticated app-server with a concrete model, an ephemeral read-only thread, disabled approvals and instructions forbidding tools. Compatible text endpoints receive chat messages without tools; tool requests and incomplete responses are rejected. Account access, service retention and usage charges belong to the selected provider.

Credentials enter through the application process environment or an existing managed launcher. Settings stores variable names, not values. Credentials are excluded from history, diagnostics and shell previews. Compatible endpoints reject embedded credentials and redirects; HTTPS is required except for loopback servers. [Provider setup](provider-selection.md) explains connection settings.

## Clipboard and insertion

Completed dictation and successful rewrites copy automatically by default. Settings can disable either action. Volatile speech and partial rewrites never auto-copy. Explicit Copy uses the current editor text; Save changes local storage without contacting a model or changing the clipboard.

Automatic insertion is Experimental and off by default. It requires a restorable, nonsecure target. Unavailable or stale targets become copy-only. Uncertain insertion does not retry or fall through to a second delivery route. A failure preserves recognized text so copying it again does not repeat transcription.

## Local history and recovery

Settings and saved work use the XDG config/data directories with owner-only state. History retains raw recognition and completed working versions, instructions, provider/model metadata and timestamps. Search covers saved conversations. Markdown and JSON exports contain raw recognition, saved source and completed replies.

Audio retention follows the selected policy. Recognition failures can retain recovery audio when allowed; retry validates that it belongs to managed storage and produces a preview without automatic copying or insertion. Deletion and retention pruning also remove associated conversations and replies. Path ownership checks prevent arbitrary files from being adopted as recovery audio.

Automatic titles are enabled by default. They send up to 6,000 characters from a new conversation to the rewrite provider. A local label remains if generation fails, and manual renames take precedence. Existing history is not submitted for titles on startup. Titles can be disabled independently.

Incognito saves no new history or recovery audio and disables conversation rewriting and generated titles. Recognition still uses the chosen speech provider; cancellation cannot recall audio already sent to a cloud service.

## Desktop integration and diagnostics

Shortcuts are approved by the desktop portal. The Omarchy widget receives a bounded volatile preview and conversation/style identifiers through the local application bridge. It does not persist that stream, hold credentials or make provider requests itself. Its completed-note actions operate on the identified conversation, even while another note is open.

Diagnostics exports contain bounded event categories, timings and nonsecret configuration flags. They exclude audio, transcript text, selected text, clipboard contents, device names, application/window identities, credentials and arbitrary provider error messages. Incognito writes no diagnostic events.

Native installation checks ownership before replacing files and preserves the previous app until its installation succeeds. The combined setup checks plugin customizations first; a later widget failure leaves the successfully installed native app available and reports how to retry. Upgrades keep settings and conversations; uninstall retains user-created data. See the [installation guide](../linux/README.md#install-for-the-current-user) and [migration behavior](identity-migration.md).
