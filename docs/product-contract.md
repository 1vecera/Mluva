# Mluva product contract

## Product outcome

Mluva is a native macOS and Linux voice-input system that turns speech into faithful, finished text. macOS can work privately on-device through Apple Speech or use Google Cloud Speech-to-Text for harder technical and multilingual dictation. The supported Fedora GNOME client uses ElevenLabs Scribe v2 for recognition and the local Codex app-server for optional cleanup, styles, and Command mode. Both keep recognition, enhancement, and delivery independently replaceable.

The initial public support boundary is Fedora 44, GNOME Shell 50, and Wayland. macOS remains a source preview until its current implementation, signing, notarization, packaging, and update path are independently verified; this contract describes intended macOS behavior without implying a supported public binary. Implemented behavior is not automatically accepted behavior: the generated [feature-maturity matrix](feature-maturity.md) and the matching in-app registry limit **Verified on Linux** to the current manual acceptance boundary and label every other capability **Experimental**.

Raw speech recognition is not the product boundary. The product is complete only when it captures reliably, preserves meaning, lands text exactly once in the intended application, and makes every failure recoverable.

The common behavioral contract below applies to both platforms unless a provider or operating-system capability is named. The [Fedora GNOME platform profile](linux-platform-profile.md) is authoritative where GNOME Wayland cannot safely expose a macOS capability to a normal application.

## Product principles

- Prefer a faithful awkward transcript over a fluent factual change.
- Keep the raw transcript immutable and inspectable beside every derived version.
- Show volatile recognition in Mluva's display-only live surface; commit to the target application only when provider text is final and eligible for delivery.
- Make provider choice explicit. “Automatic” may route work, but it must reveal which provider handled it.
- Keep macOS Apple-only operation functional without an account, API key, downloaded third-party model, or network.
- Send audio or contextual content to a cloud only when the chosen mode permits it.
- Treat insertion, focus preservation, clipboard restoration, retry, and cancellation as core behavior.
- Keep dictation and meeting capture separate because their latency, speaker, and output contracts differ.

## Capture modes

### Dictation

- On macOS, hold a configurable global hotkey, speak, and release to finish. On supported Fedora GNOME, press a portal-approved function key once to start and again to finish; F9 is the default and Settings accepts F1 through F24.
- On macOS, double-tap the hotkey to enter hands-free capture and tap again to finish. Linux function-key capture is already toggle-based and needs no double-tap gesture.
- Provide an explicit cancel action that never inserts text. macOS supports global Escape under its Accessibility contract; Linux uses an approved cancel shortcut globally and bare Escape only while Mluva is focused.
- Show a live waveform, elapsed time, active provider, input device, and volatile transcript.
- Show preparing state immediately, keep capture gated until the selected recognition stream is writable or its controlled batch fallback is selected, and announce readiness before accepting speech.
- Support spoken structural commands such as “new paragraph,” “new line,” and “scratch that.”

### Command

- Operate on the selected text in the application that was focused when capture began.
- Interpret speech as an edit instruction when text is selected.
- Interpret speech as a short question or drafting instruction when no text is selected.
- On macOS, use Gemini 3.6 Flash first whenever Google Cloud processing is configured and allowed, regardless of the recognition provider; use Apple Intelligence as the command fallback.
- On macOS, keep Incognito commands local-only and use Apple Intelligence when Google Cloud processing is unavailable.
- On Linux, use a locally authenticated Codex app-server thread for Command mode and fail closed in Incognito while ephemeral Codex durability cannot be proven.
- Send a Command provider only the bounded spoken instruction and explicit selected text; exclude application identity, window title, and nearby text.
- Preview destructive or meaning-changing edits before replacing the selection.

### Scratchpad

- Capture longer-form thinking into an editable native buffer.
- Preserve raw audio and raw transcript until the user accepts or deletes the result.
- Allow the result to be copied or processed through a saved style; macOS may also insert it into the original application after explicit acceptance.

### Meeting

- Capture microphone and system audio only after explicit selection of Meeting mode.
- Keep meeting recordings and summaries separate from dictation history.
- Produce a transcript, speakers when available, summary, decisions, and action items.
- Never enable meeting capture as a side effect of ordinary dictation.

## Recognition providers

### Apple

- Use `SpeechAnalyzer` and `SpeechTranscriber` on supported macOS releases.
- Fall back to `SFSpeechRecognizer` on supported older systems.
- Prefer on-device recognition when Apple reports it available.
- Expose model availability and language support before capture begins.
- Handle Apple's task-duration limits by rolling sessions without duplicating or dropping text.

### Google Cloud

- Use Speech-to-Text V2 streaming recognition over gRPC.
- Default to the EU endpoint and Chirp 3 for supported languages.
- Send configuration before audio and keep every audio message within Google's strict size limit.
- Roll streams before the service limit, overlap audio safely, and deduplicate repeated words at the boundary.
- Authenticate through Application Default Credentials or an explicitly selected service-account file. Never store a private key in preferences.
- Preserve recorded audio after a recoverable provider failure so the same utterance can be retried.
- Treat the first successfully written streaming configuration as the Google readiness boundary and fail startup when that boundary is not reached within a fixed timeout.

### ElevenLabs Scribe v2

- Use Scribe v2 Realtime for Linux Dictation, Command, and Scratchpad, and complete its readiness handshake before microphone capture begins.
- Stream bounded mono PCM16 chunks while independently finalizing an owner-only local WAV for controlled fallback and retention policy.
- Keep partial and final-but-uncommitted events display-only; only provider `committed_transcript` events can enter raw History, enhancement, or delivery.
- Commit long captures at bounded intervals and at Stop, never upload a successful realtime result through batch recognition again, and use one explicit batch fallback when startup, streaming, or finalization fails.
- Use batch Scribe v2 for Meeting diarization and retained-audio recognition retry.
- Authenticate only from the process environment, preferably through the scoped managed personal-vault launcher, and never persist or render the key.

### macOS automatic routing

- Prefer Apple when offline, in Apple-only privacy mode, or when the language and requested mode are supported on-device.
- Prefer Google when the user requests it, Apple cannot provide the requested language, or cloud accuracy is enabled for technical dictation.
- Fall back only when the privacy policy permits it.
- Surface the fallback and its reason in the overlay and history entry.

### Linux recognition routing

- Prefer Scribe v2 Realtime for interactive capture and surface readiness before starting PipeWire.
- Fall back once to Scribe v2 batch from the finalized local WAV when realtime is unavailable or fails before committed text is available.
- Persist the route as realtime, batch, or batch retry and limit fallback metadata to reviewed non-content categories rather than arbitrary provider exceptions.

## Transcript pipeline

- Store raw recognition separately from normalized and enhanced text.
- Normalize whitespace and spoken punctuation deterministically.
- Apply exact dictionary replacements before generative enhancement.
- Expand user snippets through explicit triggers rather than fuzzy guesses.
- Support per-application vocabulary, replacement rules, style, and provider preference.
- Remove fillers and false starts only when enabled.
- Offer faithful cleanup and rewrite as distinct operations.
- On macOS, use Gemini 3.6 Flash through Vertex AI for Command whenever Google Cloud processing is enabled, with Apple Intelligence as fallback, and use Gemini for faithful cleanup and output-mode rewrites when Google Cloud handled recognition.
- On Linux, use the local Codex app-server for Command, optional faithful cleanup, and saved-style rewriting, then reject optional output that changes protected facts or technical tokens.
- Validate enhanced output against protected terms, URLs, numbers, file paths, code identifiers, and negation before delivery.
- Let users inspect and restore the raw transcript from history.

## Cleanup preserves immutable raw segments and ordered results

- Keep speech recognition and cleanup behind separate provider contracts.
- Resolve one cleanup provider by unique stable ID and full stable model identifier before capture begins; unresolved aliases and undisclosed runtime fallback are invalid.
- Freeze cleanup, context, voice profile, privacy, retention, and destination policy for the session so preference changes cannot alter work already in flight.
- Send only typed, disclosed, size-bounded context and bounded segment and vocabulary fields to cleanup providers.
- Send Gemini cleanup only transcript text, bounded protected vocabulary, and explicit output-mode instructions. Command fallback may additionally send the spoken instruction and explicit selected text. Never send application identity, window title, or nearby-text context to Gemini.
- Run final-segment cleanup with bounded concurrency while Raw Text remains visible and recoverable.
- Publish cleaned segments in capture order even when provider attempts finish out of order.
- Select the exact immutable Raw Text for a segment after timeout, cancellation, capacity pressure, unsafe output, malformed output, or provider failure.
- Treat overlay projection as display-only state; it never authorizes insertion, persistence, or command execution.
- On stop, cancel queued work and drain active attempts within a fixed bound before selecting the terminal ordered transcript.
- Record the cleanup provider ID, full model identifier, disclosed context kinds, and cleanup outcome with the history entry.
- Require cleanup providers used in Incognito mode to support ephemeral operation without durable side effects.

## Context

- Capture the target process and focused accessibility element before a global-shortcut workflow can redirect focus; button-started Linux capture is copy-only.
- On macOS, use application identity, window title, selected text, and nearby accessible text as context sources under the applicable controls.
- On Linux, use application identity only for local scoping and disclose only bounded explicit selected text in Command mode; do not read window titles or nearby text.
- Do not capture screenshots by default.
- Allow context collection to be disabled globally and per application.
- Show which context was used for every enhanced result.

## Delivery

- Insert a completed segment exactly once.
- Deliver no volatile partial transcript to the target application in release-to-insert mode.
- Restore the original application and focused element before insertion.
- Restore the captured selection before native replacement or paste confirmation so later focus and caret changes cannot redirect delivery.
- On macOS, prefer accessibility-native value or selected-text replacement when the target supports it, then use serialized full-pasteboard preservation and the Unicode-event fallback.
- On Linux, install the complete transcript on the clipboard, prefer one native AT-SPI editable-text replacement on the exact captured target, and dispatch at most one compositor-supported keyboard paste only when native editing is unsupported and the event-tracked target is revalidated immediately before input.
- Confirm fallback paste against the captured target without reading its text. An unconfirmed dispatch is terminal, is never retried automatically, and leaves the complete transcript on the clipboard as recovery.
- Show recovery guidance when target restoration, permission, clipboard installation, or safe clipboard recovery fails.
- Use copy-only delivery with explicit manual-paste guidance when Accessibility permission or the captured target is unavailable.
- Preserve word boundaries without adding an unconditional trailing space.
- Keep a delivery receipt so retries cannot duplicate an already inserted segment.
- Coalesce repeated stop requests into one finalization and one delivery while completing every caller with the same result.
- Use the provider's final available callback text if provider finalization returns no separate transcript or fails after a stable final callback.

## Recovery and history

- Save raw text, delivered text, speech provider, cleanup provider and model, language, mode, target application, timing, and delivery outcome.
- Keep audio only according to the user's retention policy; failure-only retention is the default cloud-safe option.
- Retry recognition from retained audio without recording again.
- Retry delivery independently of recognition.
- Copy, paste, reprocess, rename, export, and delete any history entry.
- Provide Incognito mode that writes no history and erases retained audio after delivery.
- Provide configurable retention and a one-action permanent delete.

## Personalization

- Maintain dictionaries for names, companies, product terms, acronyms, and domain language.
- Support exact replacements with case-preservation options.
- Support snippets with spoken triggers, deterministic variables, and per-application scope on both platforms. macOS additionally supports secure-field-aware typed expansion; Linux stores exact typed triggers for portability but does not run a broad desktop key listener under GNOME Wayland.
- Support saved styles for messages, email, prose, technical notes, prompts, and custom instructions.
- Present saved styles as output modes in the main menu, show the selected mode's full instructions, allow custom modes to be created and edited there, and include Google Chat and Tasks presets.
- Keep the full Capture mode label visible, and explain Dictate, Command, Notes, Meeting, and the selected output mode on hover before recording.
- Remember the last mode and style per application when enabled.
- Suggest vocabulary additions from corrected history without adding them automatically.

## Privacy and observability

- Clearly label whether audio and context remain on-device or are sent to Google, ElevenLabs, or the local Codex app-server boundary.
- Keep Fedora GNOME global shortcuts inside the XDG portal boundary: register only the recording-toggle and cancellation actions, receive only their activation metadata, and never open a broad key listener.
- Never send clipboard contents, selected text, nearby text, or screenshots without the applicable context setting.
- Redact secrets from diagnostics and exports.
- Expose capture latency, recognition latency, enhancement latency, delivery latency, provider failures, and fallback events locally.
- Provide a diagnostics export that contains configuration and event timing but excludes audio and transcript content by default.

## Acceptance evidence

- Pure unit tests prove routing, accumulation, rollover deduplication, deterministic cleanup, vocabulary, snippets, protected-token validation, and delivery idempotency.
- Contract tests exercise Apple and Google providers through injected transports without microphone or network dependencies.
- Integration tests exercise microphone-to-live-surface-to-delivery state with fake providers and target applications.
- Manual compatibility checks cover native AppKit and SwiftUI fields, browser inputs, Electron applications, terminals, editors, rich-text fields, and secure fields.
- Failure tests cover permission denial, hotkey auto-repeat, stop during startup, provider disconnect, stream rollover, empty speech, focus loss, clipboard mutation, target exit, and retry after restart.
- Cleanup failure tests cover provider identity, context overreach, out-of-order completion, timeout, ignored cancellation, bounded request and response data, raw fallback, stop-and-drain, and Incognito durability.
- The release build is signed, notarized, updateable, and exercised on every supported macOS release and architecture.
- The Linux package is exercised with local HTTP/WebSocket providers, fake PipeWire and AT-SPI boundaries, fake desktop delivery subprocesses, an independent fake Codex app-server, and an isolated installed-file layout. Manual target-application compatibility remains explicit evidence and is never inferred from unit tests.

## Research basis

- The current official-source competitor catalogue, capability inventory, Mluva parity ledger, and delivery sequence are maintained in the [AI dictation competitive feature inventory](competitive-feature-inventory.md).
- Wispr Flow documents release-to-insert dictation, command mode, dictionary, snippets, contextual formatting, configurable shortcuts, and retryable failed transcription.
- Apple documents modern on-device transcription through `SpeechAnalyzer` and the compatibility path through the Speech framework.
- Google documents the V2 streaming message order, payload limit, stream lifetime, and Chirp 3 regional availability.
- Current Reddit, Hacker News, X, and YouTube evidence consistently places the unmet value in domain terminology, faithful cleanup, low latency, privacy, app compatibility, and recovery. Promotional evidence was treated as product inventory rather than independent validation.
- The social evidence also contradicts a cloud-only Flow clone: on-device operation and user-controlled providers are now expected, while generative cleanup that silently changes technical meaning is actively distrusted.

## Primary references

- [Wispr Flow overview](https://docs.wisprflow.ai/articles/2772472373-what-is-flow)
- [Wispr Flow command mode](https://docs.wisprflow.ai/articles/4816967992-how-to-use-command-mode)
- [Wispr Flow snippets](https://docs.wisprflow.ai/articles/5784437944-create-and-use-snippets)
- [Wispr Flow dictionary](https://docs.wisprflow.ai/articles/4052411709-teach-flow-your-words-with-the-dictionary)
- [ElevenLabs Scribe v2 Realtime API](https://elevenlabs.io/docs/api-reference/speech-to-text/v-1-speech-to-text-realtime)
- [ElevenLabs realtime transcript and commit strategies](https://elevenlabs.io/docs/eleven-api/guides/how-to/speech-to-text/realtime/transcripts-and-commit-strategies)
- [XDG Desktop Portal Global Shortcuts](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.GlobalShortcuts.html)
- [XDG Desktop Portal Input Capture](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.InputCapture.html)
- [GNOME Shell extension architecture](https://gjs.guide/extensions/overview/architecture.html)
- [Wispr Flow context awareness](https://docs.wisprflow.ai/articles/4678293671-feature-context-awareness)
- [Wispr Flow failed-transcription retry](https://docs.wisprflow.ai/articles/2503460374-retry-failed-transcriptions)
- [Apple SpeechAnalyzer](https://developer.apple.com/documentation/Speech/SpeechAnalyzer)
- [Google Speech-to-Text V2 RPC reference](https://docs.cloud.google.com/speech-to-text/docs/reference/rpc/google.cloud.speech.v2)
- [Google Chirp 3](https://docs.cloud.google.com/speech-to-text/v2/docs/chirp-model)
- [Hacker News discussion of native Apple transcription](https://news.ycombinator.com/item?id=49073834)
- [Hacker News discussion of local-first provider choice](https://news.ycombinator.com/item?id=44942731)
- [Reddit discussion of app-aware insertion](https://www.reddit.com/r/macapps/comments/1umikks/os_typewhisper_150_opensource_macos_dictation/)
- [Reddit discussion of faithful domain transcription](https://www.reddit.com/r/ProductivityApps/comments/1ulciah/unpopular_opinion_ai_cleanup_dictation_apps_are/)
