# Product behavior and privacy

Mluva turns speech or existing text into editable drafts while retaining the original recognition. Omarchy is the primary, daily-used platform; [desktop support](linux-platform-profile.md) and [feature status](feature-maturity.md) describe the supported workflow and experimental capabilities.

## Recording and rewriting

The recording shortcut toggles capture. A second press during preparation cancels before recording starts. Mluva checks the selected input and required provider readiness before opening the microphone. PipeWire audio is captured as private 16 kHz, 16-bit mono PCM.

A successful recognition with no speech completes quietly. It does not rewrite, copy, paste, create a blank history entry or replace the current conversation. A visible Live draft remains available to edit and copy. Silent audio follows the successful-capture retention policy; Incognito erases it. Malformed responses, network errors and capture failures still use the normal recovery path.

Scribe displays live recognition but only committed text becomes raw history or final dictation delivery. If realtime recognition cannot start or finalize, the app reports the batch fallback explicitly. Batch providers normally recognize at Stop; Live rewrite can request provisional chunk previews before final recognition.

Raw recognition, edited source, completed rewrites and delivered text remain separate. Saving an editor changes its working version, not the raw record. Rewrites use the current saved version and conversation context. Partial replies remain in memory and cannot be copied as completed results. Failure, cancellation, deletion and Incognito invalidate late responses.

Manual Rewrite controls start collapsed. The Rewrite button reveals them, and the app remembers the last expanded or collapsed choice across restarts. Collapsing preserves an unsent request. The empty request field shows its placeholder without a caret; typing restores the caret. An explicit rewrite command opens the controls. Running rewrites keep Cancel visible without changing the remembered choice. Live rewrite keeps its own recording controls.

Live rewrite is opt-in and can start or pause during Dictation. Active Live rewrites reconcile against the complete committed transcript at Stop; a paused draft is saved with an unreconciled label and never auto-copied. The default Grilling template pins unanswered questions and grows architecture notes only from supplied facts. Other templates may mark missing information. Model output still needs review. Local Mermaid previews preserve the exact editable Markdown. See [Live rewrite](providers-and-live-rewrite.md#live-rewrite).

Command and Notes modes retain an explicit preview/accept step. Meeting mode explicitly records the selected microphone and output sink's monitor; system audio means everything playing through that sink. Partial-source failures are reported. Meeting remains Experimental.

## Providers and credentials

Speech and rewriting are selected independently. Cloud speech sends audio to its provider; rewriting and automatic titles send text to the chosen rewrite provider. App-managed local models keep recognition local and never use a paid fallback. They load only while transcribing. Choosing Skip for rewriting disables model-based polishing and automatic titles.

Codex uses its existing authentication and a concrete model in an ephemeral thread with no execution environment. Mluva disables tools, MCP, apps, hooks and skills, filters inherited environment variables, and hides global instruction files in a child mount namespace. Isolation is checked before text is submitted; unsupported servers and unexpected tool activity fail closed. Compatible text endpoints receive chat messages without tools; tool requests and incomplete responses are rejected. Account access, service retention and usage charges belong to the selected provider.

Credentials enter through the application process environment or an existing managed launcher. Settings stores variable names, not values. Credentials are excluded from history, diagnostics and shell previews. ElevenLabs batch uploads and compatible endpoints reject redirects so credentials and recordings stay at the configured endpoint. Compatible endpoints reject embedded credentials; HTTPS is required except for loopback servers. [Provider setup](provider-selection.md) explains connection settings.

## Clipboard and insertion

Completed dictation and successful rewrites copy automatically by default. Settings can disable either action. Volatile speech and partial rewrites never auto-copy. Explicit Copy uses the current editor text; Save changes local storage without contacting a model or changing the clipboard.

Automatic insertion is Experimental and off by default. It requires a restorable, nonsecure target. Unavailable or stale targets become copy-only. Uncertain insertion does not retry or fall through to a second delivery route. A failure preserves recognized text so copying it again does not repeat transcription.

Automatic dictation and Live delivery preserve Chromium's actual highlight rather than restoring its unreliable Unicode offsets. Changing the field or selection makes delivery copy-only. Metadata that cannot confirm the complete insertion keeps an unconfirmed receipt and never triggers a retry. Explicit Command capture of mixed-Unicode Chromium selections remains unsupported by this repair. See the [browser verification and limits](verification/browser-direct-transport/README.md#chromium-selection-preserved-for-automatic-delivery).

## Local history and recovery

Explicit screenshot context belongs to its recording or selected conversation. Captures use Omarchy's normal region picker and leave the clipboard unchanged. Saved PNGs remain editable in Tensaku; each AI request freezes complete saved image bytes and their narration offsets. The selected rewrite provider receives those pixels only during an explicit rewrite or the already enabled recording processing. Incognito rejects screenshot capture and image requests. A request accepts at most eight screenshots, 8 MiB per image and 24 MiB total. Images are retained, merged, removed and pruned with their conversation. Completed selection before an interrupted recording remains reviewable in History; unfinished selections are cancelled.

Settings and saved work use the XDG config/data directories with owner-only state. History retains raw recognition and completed working versions, instructions, provider/model metadata and timestamps. Search covers saved conversations. Markdown and JSON exports contain raw recognition, saved source and completed replies.

Audio retention follows the selected policy. Recognition failures can retain recovery audio when allowed; retry validates that it belongs to managed storage and produces a preview without automatic copying or insertion. Deletion and retention pruning also remove associated conversations and replies. Path ownership checks prevent arbitrary files from being adopted as recovery audio.

Sidebar actions rename, merge or delete saved conversations; clicking a chat title edits it in place. Merging preserves immutable originals and saved replies, combines the current working versions in the selected order, and keeps one sidebar entry under the destination title. Original recordings remain in History and exports. Confirmation is required for merge and deletion; deletion and retention cover the complete combined conversation. These local actions do not invoke a provider or copy text.

Automatic titles are enabled by default. They send up to 6,000 characters from a new conversation to the rewrite provider. A local label remains if generation fails, and manual renames take precedence. Existing history is not submitted for titles on startup. Titles can be disabled independently.

Incognito saves no new history or recovery audio and disables conversation rewriting and generated titles. Dictation, meeting source files and batch preview chunks use a private memory-backed `/dev/shm` directory, with no disk-directory fallback. An independent cleanup process removes it when Mluva exits or is killed; reboot also clears the volatile filesystem. This does not control operating-system swap or crash dumps. Recognition still uses the chosen speech provider; cancellation cannot recall audio already sent to a cloud service.

## Optional account continuity

The main toolbar's Account sync icon opens Account & devices directly; its tooltip reflects the current account status. Settings → Account & devices remains available. Sign in with browser opens Cognito with PKCE and a ten-minute state-bound `mluva://account` callback. The browser return opens the account panel so connection status or a recovery message is visible. Credentials use the existing Secret Service (`secret-tool`); an unavailable keyring permits a session-only login with a visible explanation. Nonsecret host configuration and per-account device choices are cached for reconnect. Login never changes native speech/rewrite settings, local models or audio retention. Native capture still works signed out and offline.

New native recordings are shared only when “Sync new recordings automatically” is enabled. Previously private history requires the explicit “Share the open conversation” confirmation. Once shared, saved edits, titles, completed rewrites and original recording segments sync through one revisioned account note. Native microphone audio, retained files, screenshot pixels, local paths, provider credentials and model settings are excluded from account documents and live requests. An explicitly selected native cloud provider remains a separate user choice.

Receivers are named and selected explicitly. Volatile live words have ordered sequence numbers and acknowledgement retry; they do not trigger native clipboard or insertion. The durable note is the recovery path if a receiver misses live updates. Native previews are speculative until final recognition. Native sync persists a frozen edit/revision/operation ID, and web recovery stores its equivalent in account-scoped IndexedDB. Concurrent edits require a visible choice; original recognition cannot be replaced. Continued recordings and merges preserve original segments, with merged source history retired atomically on the account. Tombstones prevent late saves from resurrecting deleted notes.

Sign-out cancels sync, removes downloaded account notes and clears keyring credentials while retaining original local recordings, models and settings. If offline revocation cannot reach the server, the UI asks the user to remove that device from another signed-in device. Removing a device blocks its existing tickets, live connection and history access even while its access JWT has time remaining. Signing in afresh can register a new device; lost-account control still requires changing account credentials. Incognito pauses native account traffic and imports; the browser disables live sync, history, rewrite and recovery persistence for its new words.

The installable web workspace provides recording/Stop/Cancel, continuation, editing, immutable originals, rewrite preview/apply, editable templates, Dictate/Notes/Command, searchable paged history, rename, merge, deletion, copy/export, device settings, privacy and recovery. Command uses text explicitly supplied in its editor and a spoken or typed rewrite instruction. Browser speech sends microphone PCM directly to Scribe using a short-lived server-minted token; account history and live fan-out receive text only. Browser permissions, screen suspension and clipboard user-gesture requirements apply. Arbitrary-window insertion, global shortcuts, system-audio meeting capture and screenshot narration remain native workflows. The browser does not promise offline microphone transcription or background recording.

Public speech and optional Bedrock rewriting remain disabled in deployment defaults. The canonical-host preparation and safe retirement of the PC-dependent bridge are recorded in the [account continuity cutover plan](verification/account-sync/cutover.md). Do not retire the installed bridge before the approved replacement passes device acceptance.

## Desktop integration and diagnostics

The optional [browser recorder](browser-recorder.md) accepts authenticated device recordings through Cloudflare Access and Tunnel. Live microphone mode streams ordered PCM into the native recording session, automatically showing the desktop widget without opening the PC microphone or focusing the main window. Either device can Stop; the native provider, Live/rewrite, clipboard and retention rules apply. The optional send-at-Stop inbox uses Scribe v2 and saves recognition in local History before it is reused across browser devices. It can be added to a phone’s home screen as a standalone web app, with Record / Stop as the main action. Installation keeps the same Access gate and requires an online PC; it does not add background recording or cache authenticated responses. Its own audio staging, browser-local recovery, clipboard behavior and Incognito limits are described in that guide. Native installation does not enable this service.

Shortcuts are approved by the desktop portal. The Omarchy widget receives a bounded volatile preview and conversation/style identifiers through the local application bridge. It does not persist that stream, hold credentials or make provider requests itself. Its completed-note actions operate on the identified conversation, even while another note is open.

Diagnostics exports contain bounded event categories, timings and nonsecret configuration flags. They exclude audio, transcript text, selected text, clipboard contents, device names, application/window identities, credentials and arbitrary provider error messages. Incognito writes no diagnostic events.

Native installation checks ownership before replacing files and preserves the previous app until its installation succeeds. The combined setup checks plugin customizations first; a later widget failure leaves the successfully installed native app available and reports how to retry. Upgrades keep settings and conversations; uninstall retains user-created data. See the [installation guide](../linux/README.md#install-for-the-current-user) and [migration behavior](identity-migration.md).

## Editable task prompts

The Linux client exposes all Live task instructions and initial structures, on-demand rewrites, built-in and saved styles, automatic titles and optional cleanup through one prompt editor and local Markdown overrides. Settings and Ctrl+P open exact prompt identities; hover/focus settings buttons never execute a prompt. Saves preserve active recording snapshots and manual draft revision gates. Fixed source-integrity and response protocols remain in code. Existing custom text remains a lossless baseline, malformed files stay recoverable, and Incognito is read-only. See [prompt configuration and lifecycle](prompt-editor.md).
