# Providers, editable documents and live rewrite

Settings → **Workspace** controls editing actions, automatic copying, scrolling and Live rewrite. Settings → **Providers** chooses speech recognition independently from rewriting. Press **Apply** to save a page. Provider changes apply to the next recording or request and are unavailable while work is active.

**Ctrl+P** searches every settings row and opens its page at that control. Widget position, time format, sidebar visibility and Live templates also have direct commands. The history sidebar starts hidden; its toggle reveals saved conversations and a **Live conversation** entry during recording and finalization. Incoming speech does not switch you away from a selected older conversation. Dates use `Mon 7 Sep · 14:30`, with an optional `2:30 PM` clock.

The [provider selection guide](provider-selection.md) covers provider → model choices, optional discovery, connection guidance and advanced endpoint/key-variable settings.

The same settings live in `$XDG_CONFIG_HOME/mluva/config.json`, normally `~/.config/mluva/config.json`. Edit this JSON while Mluva is closed, then restart; it is not watched for external changes. Existing installations keep their previous settings, and omitted new keys receive the defaults below. The file is owner-only. It contains environment variable **names**, never API key values.

## Editable documents and clipboard behavior

Click the original or any completed rewrite to edit it. The Save icon or **Ctrl+S** persists edits. Requesting a follow-up also saves the current edits before building its context. Copy uses the current editor contents; Save does not invoke a model or alter the clipboard. Unsaved edits survive navigation within the running workspace; save them before quitting. History keeps raw recognition separately for recovery, and exports include the saved working original and edited replies.

Completed dictation and successful rewrites copy automatically by default. Volatile speech and partial rewrite responses never auto-copy. In Live rewrite mode the completed dictation may copy first, followed by the final structured draft when it succeeds. Turning off both automatic copy options leaves Copy available; turning off dictation copying also suppresses automatic paste for that capture. Notes and Command keep their explicit acceptance step.

| JSON setting | Default | Meaning |
| --- | --- | --- |
| `auto_copy_dictation` | `true` | Copy completed dictation automatically. |
| `auto_copy_rewrite` | `true` | Copy successfully completed rewrites, including the final live draft. |
| `show_copy_action` | `true` | Show Copy icons in documents and the Omarchy review widget. |
| `show_save_action` | `true` | Show Save icons in editable documents; Ctrl+S remains available. |
| `review_timeout_seconds` | `4` | Omarchy review dismissal, 1–60 seconds; hover, focus, menus and rewriting pause it. |
| `widget_position` | `"bottom-center"` | Omarchy widget preset: `bottom-left`, `bottom-center` or `bottom-right`; manual dragging remains available. |
| `history_sidebar_visible` | `false` | Show the history sidebar in the wide layout; compact windows use an overlay. |
| `time_format` | `"24h"` | History clock: `24h` or `12h` with AM/PM. |
| `smooth_scrolling` | `true` | Animate following new text. Scrolling away pauses following. |
| `scroll_duration_ms` | `800` | Scroll animation, 0–2,000 milliseconds. |
| `scroll_lookahead_lines` | `2` | Advance room below new text, 0–6 lines; the five-line widget scales this down to fit. |

The widget's finished note is already persisted, so it has no redundant Save control. Editing takes place in the main window.

## Rewrite providers

**Codex app-server** remains the default. Authenticate the installed Codex CLI as usual. Its model picker and optional Fast tier remain available; the chosen model is frozen for each request. Fast and reasoning overrides apply only to Codex. The local app-server is a client boundary, not a claim that model inference stays on the device.

**LiteLLM / compatible API** connects to an existing LiteLLM proxy or another compatible server. Mluva uses `GET /models` for the picker and streaming `POST /chat/completions` for text. Choose a deployment alias exposed by that server; an explicit alias also works if the server does not expose a model catalog. Responses must contain text and a successful `stop` finish reason. Tool requests, truncated output, cancellation and broken streams are rejected instead of saved as completed rewrites. No tool definitions are sent.

| JSON setting | Default | Meaning |
| --- | --- | --- |
| `rewrite_provider` | `"codex"` | `codex` or `litellm`. |
| `litellm_base_url` | `"http://localhost:4000/v1"` | Base URL including any API prefix. |
| `litellm_model` | `null` | Required deployment alias when using LiteLLM; select in Settings or the picker. |
| `litellm_api_key_env` | `"LITELLM_API_KEY"` | Environment variable containing the proxy key; absent means no Authorization header. |
| `codex_model` | `null` | Existing Codex default for cleanup and titles. |
| `rewrite_model` | `null` | Codex rewrite override; null follows the configured/catalog default. |
| `rewrite_fast_mode` | `false` | Codex Fast tier, only where advertised. |

For LiteLLM, cleanup, titles, normal rewrites and Live rewrite use the configured rewrite deployment. Service credentials and routing belong in the proxy; see [LiteLLM proxy authentication](https://docs.litellm.ai/docs/proxy/user_keys). Mluva does not install a proxy or download models. Endpoints must use HTTPS, except loopback HTTP for local servers; URLs cannot embed credentials, queries or fragments. The client rejects redirects to keep requests on the configured endpoint.

## Speech providers

**ElevenLabs Scribe** retains its native realtime path and controlled batch fallback. Existing credential names remain supported. **Voxtype · local Whisper** runs the installed Omarchy dictation engine on Mluva's finalized WAV, with local Whisper forced explicitly. It does not start a Voxtype recording daemon or inject text. An empty model setting uses the installed Voxtype configuration; otherwise set an already installed model name/path. See Voxtype's [file transcription implementation](https://github.com/peteonrails/voxtype/blob/main/src/app/transcribe_file.rs). Local speech recognition does not make cloud rewriting or automatic titles local; configure those independently.

**LiteLLM / compatible API** uploads WAV audio using multipart `POST /audio/transcriptions`. The server must expose an audio-transcription deployment that accepts `model`, `file`, `response_format=json`, and optional language, returning a JSON `text` field. A chat-only server is insufficient. LiteLLM documents routes including OpenAI, Azure, Vertex/Gemini, Deepgram, Groq, Fireworks and Mistral; actual availability depends on the proxy configuration and account. See [audio transcription support](https://docs.litellm.ai/docs/audio_transcription) and [Vertex transcription](https://docs.litellm.ai/docs/providers/vertex_transcription). Mluva speaks the shared endpoint rather than implementing each cloud SDK.

| JSON setting | Default | Meaning |
| --- | --- | --- |
| `transcription_provider` | `"elevenlabs"` | `elevenlabs`, `voxtype` or `litellm`. |
| `transcription_base_url` | `"http://localhost:4000/v1"` | Speech API base URL, independent of rewriting. |
| `transcription_remote_model` | `"whisper"` | Speech deployment alias on the server; replace with your configured alias. |
| `transcription_api_key_env` | `"LITELLM_API_KEY"` | Environment variable containing the speech API key. |
| `voxtype_model` | `null` | Optional installed local Whisper model override. |
| `transcription_chunk_seconds` | `8` | Preview chunk length for batch providers during Live rewrite, 3–30 seconds. |

Ordinary Voxtype/LiteLLM dictation transcribes at Stop. With Live rewrite enabled, sequential chunks provide provisional words while recording; Stop cancels remaining preview work and recognizes the complete audio to reconcile chunk boundaries. Preview audio is bounded to 30 minutes, and unavailable/overloaded previews fall back to the finalized recording. Temporary preview files are private and erased after each request. Cloud preview mode sends audio more than once and can increase usage; local preview latency depends on the machine and model. Meeting mode continues to use ElevenLabs diarization.

For example, a partial config for local speech with a local or remote rewrite proxy is:

```json
{
  "transcription_provider": "voxtype",
  "voxtype_model": null,
  "rewrite_provider": "litellm",
  "litellm_base_url": "http://localhost:4000/v1",
  "litellm_model": "my-rewrite-deployment",
  "litellm_api_key_env": "LITELLM_API_KEY"
}
```

`my-rewrite-deployment` is a placeholder for an alias you configure on your server. Keep the server key in Mluva's process environment, including when launching from the desktop; entering its variable name in Settings does not fetch or create that secret. Local/compatible speech configurations launch without resolving an unrelated ElevenLabs credential.

## Live rewrite

Turn on **Live rewrite** beside Dictate before or during dictation. Its small downward arrow selects **Grilling**, **Task spec**, **Structured note**, **Polish** or **Custom**. The selection persists and stays in sync with Settings → Workspace, where custom instructions can be edited. The original speech appears beside an editable structured draft; narrow windows stack them. Grilling is the default template for new settings; existing explicit template choices are preserved. Live rewrite itself remains opt-in.

Grilling keeps up to three unanswered questions pinned above the evolving architecture notes. Each update is instructed to retire answered questions and advance to the next material decisions. Intent, constraints, preferences, technologies and other sections appear only when the speaker supplies content. Small Mermaid sketches describe supplied relationships. This adapts the question progression in [Matt Pocock's grilling skill](https://github.com/mattpocock/skills/blob/main/skills/productivity/grilling/SKILL.md) to uninterrupted speech. Task and note templates retain their `[Missing: …]` markers; Custom accepts your own structure.

You can change templates or pause Live while the microphone stays open. Pausing cancels pending rewrites and keeps the current draft, including manual edits. Resuming incorporates current speech without restarting capture. With batch speech providers, audio collected before enabling Live is included in the first available preview; no preview requests start while Live is off. If you stop while paused, the draft is saved as **not reconciled** and never auto-copied. Live controls wait during preparation and finalization; provider changes remain unavailable during capture.

| JSON setting | Default | Meaning |
| --- | --- | --- |
| `live_rewrite_enabled` | `false` | Enable before or during a Dictation capture. |
| `live_rewrite_template` | `"grilling"` | `grilling`, `task-spec`, `structured-note`, `polish` or `custom`. |
| `live_rewrite_custom_instructions` | `""` | Instructions required when the custom template is selected. |
| `live_rewrite_min_characters` | `160` | Group new characters after the first draft, 40–4,000; a paused short tail also updates. |
| `live_rewrite_interval_seconds` | `4` | Minimum time between later requests and pause before flushing a short tail, 2–60 seconds. |

Live rewrite is off by default. Switching it on allows provisional recognition to be sent to the selected rewrite model before the speech provider commits it. The live editor and model input identify this text as provisional. The first request starts after 40 characters, or after a shorter utterance stays unchanged for the configured interval. This avoids spending the first request on opening filler without making brief dictation wait indefinitely. Later requests follow the configured interval and character grouping; short additions and same-length corrections update after a pause. Batch speech engines still update by chunk, not every word. Model processing remains part of the wait. The displayed rewrite timing measures worker dispatch to the first nonempty text response, including setup and discovery; it is not a provider latency guarantee.

Each update uses a frozen speech/draft snapshot, with at most one request in flight. Whole draft updates keep template structure readable and retain a manual reading position. If you type during a request, that result cannot replace the newer edit; a later request incorporates your current draft. The model is instructed to preserve supplied facts and deliberate edits, surface unresolved questions, and avoid inventing owners, dates or decisions. These are model instructions, so review the result.

Scribe keeps its established 25-second commit cadence. Provisional recognition is model input only: it never replaces raw recognition or automatic dictation delivery. When Live is active, Stop reconciles against the complete committed transcript, even when its text matches the last preview or is below the usual thresholds, then saves one final reply with the source. Only a successful final rewrite may auto-copy. A failed update pauses automatic requests, retains the available draft and labels an incomplete saved draft; it never auto-copies a failed final update. Cancellation, deletion and Incognito invalidate late results. Live rewrite is unavailable in Incognito and does not run for Command or Notes captures. Closing the window leaves capture running; quitting ends it.

## Mermaid sketches

Live drafts and saved replies render closed Mermaid fences using a bundled local renderer and WebKitGTK 6.0. Successfully rendered source folds while reading and reappears when the editor receives focus. Copy, Save and exports retain the exact Markdown. Invalid or incomplete diagrams remain visible source, as do sketches when WebKit is unavailable. Rendering is limited to three sketches of 12,000 characters each, with bounded output and a timeout. Strict Mermaid mode, blocked navigation/network content, rejected configuration directives and an ephemeral browser session keep sketches local. Native image decoding rejects external references and embedded browser content.

## Verification

[Contributor checks](../CONTRIBUTING.md#verification) exercise provider protocols, editing, automatic copy, late responses and final reconciliation with synthetic content. Provider choice and Live rewrite remain Experimental; quality and account compatibility depend on the selected speech and rewrite models.
