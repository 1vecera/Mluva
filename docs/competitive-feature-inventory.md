# AI dictation competitive feature inventory

Current as of 2026-08-19. The inventory was verified against current official product pages and help documentation found through Exa. It describes claimed and documented product behavior, not independently measured accuracy, latency, or reliability.

## Decision summary

The direct market baseline is now global dictation into any text field, automatic cleanup, multilingual recognition, a custom dictionary, history, and automatic insertion. The research exposed two immediate Fedora GNOME gaps: keyboard fallback was not gated on a ready daemon, and capture lacked a compact bottom-of-screen status surface. Mluva now gates the optional keyboard helper on its complete runtime boundary and provides a display-only bottom bar, while cross-application insertion still depends on each target exposing a trustworthy AT-SPI text object or accepting one explicitly configured keyboard fallback.

The shipped slice is deliberately narrow and visible: paste readiness is reported honestly, and the transient bar shows waveform, elapsed time, mode, status, delivery readiness, and a bounded volatile transcript preview before disappearing at every terminal state. On GNOME Wayland, that nonactivating projection is a separate display-only GNOME Shell extension because an ordinary always-on-top GTK window can steal focus and break target delivery.

“Copy every feature” should mean functional parity research and deliberate implementation, not copying proprietary visuals, wording, code, or trade dress. The market is open-ended and keeps changing, so this document covers the direct products with useful official documentation and screens the long tail for distinct capabilities. Every candidate feature still has to earn its complexity against Mluva’s product contract.

## Products covered deeply

| Product | Primary position | Distinctive documented capabilities | Official evidence |
| --- | --- | --- | --- |
| Wispr Flow | Cross-device polished dictation benchmark | Floating Flow Bar, contextual cleanup, backtracking, dictionary, snippets, per-context styles, developer context, meetings, notes, teams, analytics, mobile controls | [Features](https://wisprflow.ai/features), [app map](https://docs.wisprflow.ai/articles/5096240724-navigating-the-wispr-flow-app-desktop-ios-and-android), [first dictation](https://docs.wisprflow.ai/articles/6409258247-starting-your-first-dictation), [Flow Bar](https://docs.wisprflow.ai/articles/1790396454-move-and-dock-the-flow-bar-on-desktop) |
| Superwhisper | Configurable power-user dictation | Local and cloud models, customizable modes, selected-text and clipboard context, file transcription, rich history and reprocessing, meeting modes, compact and expanded recording windows | [documentation index](https://superwhisper.com/docs/llms.txt), [modes](https://superwhisper.com/docs/modes/modes), [recording window](https://superwhisper.com/docs/get-started/interface-rec-window), [history](https://superwhisper.com/docs/get-started/interface-history) |
| Aqua Voice | Low-latency contextual dictation and voice editing | Destination-aware formatting, selection-aware Edit Mode, stacked spoken edits and undo, dictionary, custom instructions, local recent audio, rerun transcription | [user guide](https://aquavoice.com/guide), [Edit Mode](https://aquavoice.com/guide/edit-mode), [dictionary](https://aquavoice.com/guide/dictionary), [history](https://aquavoice.com/guide/history) |
| Willow | Polished dictation plus intent-first writing | Contextual formatting, auto-learning vocabulary, personal and team shortcuts, per-app tone, offline mode, whisper/noise support, developer context, Scribe intent-to-draft workflow | [dictation](https://willowvoice.com/features/dictation), [Scribe](https://help.willowvoice.com/en/articles/15043797-introduction-to-scribe-in-willow), [personal dictionary and shortcuts](https://help.willowvoice.com/en/articles/13183918-using-personal-dictionary-and-shortcuts), [team collaboration](https://help.willowvoice.com/en/articles/13959267-using-collaboration-features-for-your-team) |
| Typeless | Dictation plus voice-driven AI assistant | Real-time auto-editing, self-correction, intent cleanup, per-app tone, dictionary, whisper mode, selected-text rewrite, translation, question answering and web search | [key features](https://www.typeless.com/help/quickstart/key-features), [Ask Anything](https://www.typeless.com/ask-anything), [pricing and plan features](https://www.typeless.com/pricing) |
| Monologue | Apple-first voice productivity suite | Dictation, voice notes, bot-free meetings, system audio, local models, modes and instructions, multi-device sync, Apple Watch, MCP/API/CLI access | [product](https://www.monologue.to/), [guides](https://docs.monologue.to/) |
| Spokenly | Provider-flexible dictation and automation | Multiple local and cloud models, modes with AI instructions, dictionaries and replacements, local-only network block, punctuation commands, macOS agentic actions | [documentation](https://spokenly.app/docs), [local-only mode](https://spokenly.app/docs/local-only-mode), [agentic actions](https://spokenly.app/docs/modes/agentic-actions) |
| Voquill | Open-source cross-platform voice OS | Linux/macOS/Windows, local/API/cloud recognition, BYOK, styles and dictionary, post-processing, offline operation, basic agent mode, enterprise on-prem deployment | [product](https://voquill.com/dictation/), [documentation](https://docs.voquill.com/), [transcription modes](https://docs.voquill.com/guides/transcription/) |
| OpenWhispr | Open-source all-in-one dictation and knowledge capture | Local/cloud/BYOK recognition, AI agents, meeting detection and diarization, notes with folders and semantic search, shared spaces, public REST API and MCP | [documentation](https://docs.openwhispr.com/), [source](https://github.com/OpenWhispr/openwhispr) |
| VoiceInk | Minimalist offline-first dictation | Local Whisper, optional live cloud streaming, custom vocabulary, configurable hotkey/language/model/recording mode, nine recording animations | [product](https://www.voice-ink.com/) |
| Vibe Typer | Cross-platform dictation with Linux emphasis | Per-application paste methods, AI commands, custom tone and dictionary, terminal compatibility, waveform customization, 99 languages, X11 and Wayland coverage | [features](https://vibetyper.com/features), [documentation](https://vibetyper.com/docs) |
| TypeWhisper | Extensible private desktop workflow | System-wide dictation, file transcription, workflow outputs, history, dictionary learning, snippets, plugins and integrations, local model management | [product](https://www.typewhisper.com/en/) |

## Long-tail and adjacent products screened

The direct long tail includes [Voibe](https://www.getvoibe.com/), [SpeakoFlow](https://www.speakoflow.com/), [HyperVoice](https://hypervoice.app/), [Voxtype](https://voxtype.io/), [DictaFlow](https://dictaflow.io/), [FluidVox](https://www.fluidvox.com/), [TalkTastic](https://talktastic.com/), [VoiceHotKey](https://voicehotkey.com/), [BetterDictation](https://betterdictation.com/), [Letterly](https://letterly.app/), [TalkText](https://talktext.io/), [Tota](https://www.heytota.com/), [Dictately](https://dictately.io/), [SpeechType](https://speechtype.ai/), [Simplevoice](https://simplevoice.app/), [Wspr](https://getwspr.com/), [Kalam](https://kalam.stream/), and [Yakki](https://yakki.ai/). Their distinct signals are useful even when the full products overlap heavily: local-only operation, auto-stop through voice activity detection, per-application injection strategy, terminal and VDI compatibility, plugin systems, multimodal screen context, long recordings, system-audio capture, provider choice, and aggressively simple one-time pricing.

Adjacent incumbents include [Apple Dictation](https://support.apple.com/guide/mac-help/use-dictation-mh40584/mac), [Windows Voice Access](https://support.microsoft.com/en-us/accessibility/windows/voice-access/dictate-text-with-voice), [Dragon Professional](https://dragon.nuance.com/en-us/dragon-professional.html), and [Talon Voice](https://talonvoice.com/). They broaden the comparison from transcription into operating-system control, accessibility, mature correction vocabularies, and programmable voice commands. They should not turn Mluva into a general desktop-control system unless that becomes an explicit product decision.

## Complete capability catalogue

### Capture and activation

- Global push-to-talk: hold a shortcut, speak, and release to finish.
- Global toggle capture: press once to start and once to stop.
- Hands-free capture through a dedicated shortcut or a double-tap lock gesture.
- Multiple bindings per action, including modifier combinations and extra mouse buttons where the platform permits them.
- Separate stop, cancel, dismiss, and paste-last-transcript actions.
- Configurable input device and language from the recording surface or tray menu.
- Immediate preparing state before the microphone is ready, followed by an explicit ready/listening state.
- Audible start, ready, stop, paste, failure, and cancellation cues with independent controls.
- Live microphone level or waveform so silence and wrong-device failures are visible before the user finishes.
- Elapsed recording time and a warning before the session limit.
- Volatile live transcription that is visibly provisional and never inserted before finalization.
- Push-to-talk, hands-free, whisper, long-form, and meeting capture profiles.
- Voice-activity auto-stop for a deliberately hands-off flow.
- Cancel-without-transcribe and cancel-after-transcribe recovery rules.
- Rejection of overlapping or operating-system-reserved shortcuts during setup.

### Floating recording surface

- Compact bottom bar that is absent while idle and appears only while preparing or recording.
- Waveform, elapsed time, active mode, language, provider/route, and short live transcript preview.
- Distinct visual states for preparing, ready, hearing speech, quiet/no signal, approaching time limit, cancelling, and failure.
- Obvious stop and cancel controls with accessible names and keyboard equivalents.
- Mini and expanded presentations, with the small state optimized for glanceability rather than configuration.
- Saved docking at the bottom, left, or right edge on platforms that safely support it.
- Click-through outside the actual controls so the surface does not block the target application.
- No keyboard focus acquisition and no mutation of the captured target.
- Optional hide, snooze, or “always show while idle” behavior.
- Language and mode pickers that appear only when there is an actual choice.
- Context indicator showing that selection, app, code, or clipboard context was captured, without revealing private text.
- Microphone/no-signal diagnosis directly in the bar.
- Screen-share visibility control for meeting or sensitive workflows.
- Custom appearance such as size, opacity, waveform style, or animation, kept secondary to state clarity.

### Recognition and models

- Fast cloud speech recognition optimized for interactive dictation.
- Fully local/offline Whisper, Parakeet, or other device models.
- User-selectable model size and speed/accuracy tradeoff.
- Bring-your-own-key provider support.
- Hosted cloud mode requiring no external provider account.
- Private self-hosted or on-prem provider endpoints.
- Automatic local/cloud routing with a visible reason and privacy boundary.
- Real-time streaming partials and batch finalization.
- Controlled batch fallback from retained local audio after a streaming failure.
- 99–100+ recognition languages and automatic language detection.
- Mixed-language or mid-sentence language switching.
- Translation during or after transcription.
- Whisper/quiet-voice support.
- Noise-tolerant recognition and microphone gain guidance.
- Custom recognition prompt or vocabulary biasing.
- File transcription for audio and video.
- Long-recording support with bounded rollover and deduplication.
- Speaker diarization for meetings and imported recordings.
- System-audio plus microphone capture without a meeting bot.

### Cleanup and formatting

- Automatic punctuation, casing, spacing, and paragraph breaks.
- Filler-word removal.
- Repetition removal.
- False-start and mid-sentence self-correction handling.
- Spoken “scratch that” or backtracking.
- Structured numbered and bulleted lists.
- Spoken punctuation and structural commands when literal control is needed.
- Recognition and formatting of URLs, email addresses, numbers, currency, dates, file names, paths, code identifiers, and syntax.
- Smart formatting based on the destination app or writing surface.
- Per-context tone for chat, email, documents, support, code, and social writing.
- Saved modes/styles with editable instructions and examples.
- Raw faithful mode that skips generative rewriting.
- Separate AI-polish stage with provider and model disclosure.
- Protected-token validation so cleanup cannot silently alter facts, negation, numbers, URLs, or code.
- “Press Enter” or submit-after-paste command with one-time opt-in.
- Optional automatic title, summary, decisions, tasks, and action items for long-form capture.

### Selection editing and voice commands

- Detect an explicit selection at capture start and switch into edit/rewrite behavior.
- Replace selected text in place only after target restoration.
- Natural edit instructions without rigid command syntax.
- Stacked revisions and spoken undo back to earlier or original versions.
- Shorten, expand, fix grammar, change tone, restructure, or translate selected text.
- Delete the selection through an explicit command.
- Ask a question about selected text and show the answer without replacing it.
- Draft from spoken intent when there is no selected text.
- Search the web, open a URL, or hand text to an AI assistant from a separate command/agent mode.
- Launch or quit applications and run approved OS shortcuts as explicit automation actions.
- Preview destructive or meaning-changing edits before application.

### Context awareness

- Active application identity for local scoping and output style.
- Text-field role and password/secure-field exclusion.
- Explicit selected text for editing or bounded command context.
- Nearby field text for app-aware continuation where the privacy setting permits it.
- Clipboard context captured only through an explicit, time-bounded rule.
- IDE file names, symbols, paths, and code context.
- Application or website profiles with local classification.
- Per-application mode, style, provider, vocabulary, and delivery strategy.
- Global and per-application context switches.
- A history label showing which context kinds were used without leaking the context itself.
- No screenshots by default; multimodal screen context only as a separate explicit capability.

### Personalization

- Personal dictionary for names, companies, brands, acronyms, jargon, and invented terms.
- Exact misspelling replacements with optional case preservation.
- Recognition-level pronunciation or prompt hints where the provider supports them.
- Search, sort, star, edit, delete, and bulk-manage dictionary entries.
- Automatic vocabulary learning from corrections.
- Review-only vocabulary suggestions as a safer alternative to silent learning.
- Voice snippets that expand short cues into long formatted text.
- Exact replacement rules and deterministic date/time variables.
- Per-application dictionary, snippets, and styles.
- Personal writing-style learning with an off switch.
- Saved custom modes and reusable instructions.
- Cross-device sync for selected personalization data.
- Shared team dictionary and snippets.
- Department-specific shared terminology and templates.

### Delivery and app compatibility

- Insert into the exact text field that was focused when capture began.
- Restore the captured application, accessible object, selection, and caret before delivery.
- Prefer accessibility-native editable-text insertion.
- Use a compositor-supported keyboard paste only when direct insertion is unavailable.
- Configure different delivery methods per application for terminals, browsers, rich editors, VDI, or other difficult targets.
- Install the complete result on the clipboard before any fallback.
- Confirm insertion without reading target text where possible.
- Dispatch at most once and never retry an uncertain paste automatically.
- Preserve the user’s pre-existing clipboard where the platform permits reliable serialization and restoration.
- Copy-only recovery when target identity, permission, or focus cannot be proven.
- One-action paste-last-transcript shortcut.
- Retry delivery independently from recognition.
- A visible Paste button after automatic insertion fails.
- Explicit compatibility guidance for terminals, browser renderer accessibility, rich text, secure fields, Remote Desktop, and virtual desktops.

### History and recovery

- Local chronological transcript history grouped by date.
- Search and filter by transcript, mode, status, provider, or application where privacy permits it.
- Raw recognition and delivered/processed result stored separately.
- Copy, intentional paste, edit, rename, share, export, flag, and permanently delete.
- Reprocess the same retained audio with another recognition model, cleanup model, or mode.
- Retry failed transcription without recording again.
- Retry failed delivery without retranscribing.
- Local playback of recently retained audio.
- Configurable audio and transcript retention.
- Automatic deletion schedules.
- Incognito/privacy mode with no history or recovery audio.
- Per-session recognition, cleanup, and delivery route and timing.
- User feedback on a result without silently changing future behavior.
- Recovery of failed or interrupted sessions after restart.

### Notes, meetings, and knowledge

- Editable scratchpad for long-form thinking before delivery.
- Voice notes grouped by date with search, pinning, tags, summaries, and share actions.
- Cross-device note sync.
- Bot-free meeting capture from microphone and system audio.
- Automatic meeting-app detection.
- Live meeting transcript and live word count.
- Speaker labels and post-meeting speaker correction.
- Summary, decisions, action items, questions, and follow-ups.
- Calendar connection, upcoming-meeting reminders, and one-action recording.
- Pre-meeting briefs built from connected context.
- Search and ask questions across recorded meetings and notes.
- Public API, CLI, and MCP access for approved agents.
- Folders, semantic search, shared spaces, and team access controls.
- Mobile and watch capture for voice notes.

### Privacy, security, teams, and administration

- Clear local, BYOK, hosted-cloud, and on-prem processing choices.
- Zero-retention provider routes and user-level model-training opt-out.
- A true network-blocking local-only mode.
- Owner-only files, encrypted credentials, and OS-keychain or managed-vault secret injection.
- Content-free diagnostics and reviewed exports.
- HIPAA-ready workflows and business associate agreements where relevant.
- SOC 2 Type II and ISO 27001 claims backed by the vendor’s own compliance material.
- Centralized billing, team membership, roles, and complimentary IT-admin seats.
- SAML SSO, SCIM, audit logs, MDM, and domain controls.
- Organization-enforced retention, privacy, and cloud-processing policy.
- Team and department dictionaries, snippets, and style controls.
- Usage analytics for words, apps, active days, adoption, and trends.
- Self-hosted or on-prem deployment for regulated teams.

### Product experience and platform reach

- Guided permission setup and a live first-dictation exercise.
- In-product troubleshooting for microphone, shortcut, accessibility, and paste failures.
- macOS, Windows, Linux, iPhone, Android, iPad, and watch clients in different competitor combinations.
- Desktop tray/menu-bar control and launch-at-login.
- Mobile keyboard, floating bubble, Dynamic Island/Live Activity, Control Center, Siri shortcut, and Action Button entry points.
- Localized interface, automatic language sync, and accessibility labels.
- Configurable recording animation, sound, size, and opacity.
- Usage stats such as words dictated, estimated time saved, words per minute, streak, and top apps.
- Free usage allowance, unlimited paid tier, team tier, and enterprise tier.
- One-time/lifetime purchase and open-source alternatives to subscriptions.

## Mluva parity ledger

| Capability family | Current state | Product decision |
| --- | --- | --- |
| F9 global toggle and cancel | Implemented through the XDG portal; actual desktop binding is surfaced | Keep; this is the correct GNOME boundary |
| Real-time and batch recognition | Implemented with Scribe v2 Realtime, committed segments, retained WAV, and one controlled batch fallback | Keep and expose route/status more clearly in the transient bar |
| Waveform, elapsed time, route, volatile transcript | Implemented in the application window and the optional display-only bottom bar | Keep both views bounded and erase every volatile field at terminal states |
| Reliable insertion | Native AT-SPI is preferred; keyboard fallback is advertised only when its complete daemon/socket boundary is ready; unsupported targets remain recoverable copy-only | Keep the readiness probe and exact-once receipt as release gates |
| Exact-target and exact-once safety | Stronger contract than most competitors | Preserve; no overlay or fallback may weaken it |
| Dictation, Command, Scratchpad, Meeting | Implemented | Keep; reduce main-screen density through progressive disclosure |
| Selected-text command editing | Implemented with preview | Keep; later consider a faster non-destructive edit loop |
| Dictionary, replacements, snippets, styles | Implemented, including per-app scope | Surface more simply; retain review-only learning instead of silent auto-learning |
| History and recovery | Implemented with raw/delivered text, retries, retention, diagnostics, and Incognito | Make paste-last and failed-delivery recovery easier to reach |
| Local/offline recognition on Linux | Not implemented | High-value next provider slice after daily paste and capture UX are trustworthy |
| BYOK recognition and cleanup | Not implemented on Linux | Consider after local provider abstraction; do not multiply provider complexity first |
| File transcription | Not implemented in the Linux surface | Useful power-user slice after the core daily loop |
| Notes/meeting knowledge API or MCP | Meeting and Scratchpad exist, but no public retrieval surface | Consider only after the native capture loop is excellent |
| Team, enterprise, and mobile surfaces | Not implemented | Defer until an individual daily-use cohort validates the product |

## Delivery sequence

### Shipped foundation: trust and visible capture

- The keyboard-paste fallback is operational only when its required daemon and owner-only socket are ready; executable presence alone never advertises automatic paste.
- Direct insertion, keyboard dispatch, confirmation, and copy-only fallback produce bounded, content-free receipts.
- The bottom recording bar projects preparing or recording phase, waveform, elapsed time, active mode, route, delivery readiness, and a short volatile preview while Stop and Cancel remain in the approved global bindings and main application.
- The bar hides and erases its fields after stop, cancellation, failure, shutdown, or application-owner loss.
- The GNOME Shell package is display-only, has no inbound commands or keyboard focus, persists nothing, and is verified through isolated virtual-session scenarios before live installation.

### Next: daily dictation parity

- Add paste-last-transcript as a first-class action and make a failed insert recoverable from the status surface.
- Complete a real target matrix for GTK text, browser textareas, Electron, terminal, rich text, password fields, and Remote Desktop, recording copy/insert/paste-confirmed outcomes separately.
- Make language, microphone, and mode visible but progressively disclosed; the active choice is glanceable, configuration stays in Settings.
- Add optional start/ready/stop/paste sounds and explicit quiet/no-signal guidance.
- Add bounded session-limit warnings and safe auto-stop behavior.

### Then: editing and personalization speed

- Shorten the selected-text Command path for safe transformations while preserving preview for meaning-changing edits.
- Add spoken undo for explicit selection edits if the original selection can remain recoverable without retaining unrelated target content.
- Improve dictionary suggestion review, bulk management, and one-action correction from History.
- Improve snippets and mode switching from the compact capture surface without duplicating Settings.

### After the daily loop is proven: provider control

- Add one local/offline Linux recognizer behind the existing provider contract and measure startup, latency, memory, language, and recovery behavior.
- Add BYOK only after the provider lifecycle and secret-boundary tests cover a second engine cleanly.
- Add file transcription and reprocessing through the same immutable raw-result and retention model.

### Later: knowledge and collaboration

- Make Scratchpad and Meeting outputs searchable and agent-accessible through a deliberately scoped local API or MCP server.
- Add calendar-triggered meeting affordances only if meeting capture becomes a validated primary workflow.
- Add shared vocabulary, snippets, administration, analytics, or mobile clients only after a named team or mobile cohort proves demand.

## Remaining gates before broader compatibility claims

- A real F9 capture into each supported target class produces one receipt: inserted, paste-confirmed, paste-unconfirmed, or copied. No ambiguous state is relabeled as success.
- Preparing, speech, quiet/no-signal, approaching-limit, cancellation, and provider-failure states are distinguishable without relying on color alone.
- The bar remains legible at narrow and wide widths, clips neither controls nor status, and exposes accessible names.
- All headless tests, installed-layout smoke tests, and virtual-display pixel scenarios pass; rendered screenshots are manually inspected.
- Broader claims remain prohibited unless no feature uses broad input capture, reads secure fields, persists volatile transcript text, or weakens the exact-once delivery contract.

## Research limits

Competitor documentation changes frequently, some features are staged rollouts, and vendor performance claims were not treated as facts. “Works in every app,” latency multipliers, accuracy multipliers, privacy, compliance, and zero-retention claims require independent testing or primary audit material before Mluva uses them in comparative marketing. This inventory should be refreshed before each roadmap decision rather than treated as a permanent market truth.
