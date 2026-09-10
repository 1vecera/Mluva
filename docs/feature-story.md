# Mluva features

Speak a rough idea, shape it into useful text and keep the original. Omarchy is the primary platform, with the core dictation, rewrite and widget workflow tested end to end and used daily. The [feature matrix](feature-maturity.md) lists individual experimental capabilities.

## Everyday workflow

| Feature | What it does |
| --- | --- |
| Dictation | F9 starts and stops capture. Completed text copies to the clipboard and becomes an editable conversation. |
| Rewriting | Polish, Structure, saved prompts and custom follow-ups transform the working text while keeping the original. |
| Existing text | Paste into a new conversation, then edit or rewrite without recording. |
| Live rewrite | Start or pause during dictation. Grilling pins evolving questions above architecture notes and Mermaid sketches; Task spec, Structured note, Polish and Custom remain available. Active drafts reconcile at Stop; paused drafts retain a review label. Experimental. |
| Omarchy widget | Starts with five live preview lines, a bare light at top left and timer at top right. Choose lower-left/bottom/lower-right placement, drag the text or status row, resize or use Super+T to tile. Opening preserves typing focus; clicking allows interaction. Polish, Structure, More, Copy and Open follow dictation. Hover, focus and rewriting pause the four-second dismissal. |
| Editing | Edit originals or completed replies. Ctrl+S saves; Ctrl+Enter sends a rewrite. Raw recognition remains recoverable. |
| Commands | Ctrl+P searches actions and every settings control, including direct widget, date, sidebar and Live template choices. |
| History and export | The sidebar starts hidden and keeps live and saved conversations separately navigable. Search, reopen, rename, delete or export saved conversations as Markdown or JSON; choose a 12/24-hour clock. |
| Markdown | Read headings, emphasis and code in native text views; focus reveals editable source. Copy and Save preserve it. |
| Themes and layout | The app and widget follow Omarchy's palette. The recording light and timer sit in the title bar above the text. Wide Live panes sit side by side; narrow windows stack them. |

Automatic copy is configurable. Partial speech and rewrite output never count as completed delivery. Automatic insertion is off by default; use clipboard delivery unless you have verified insertion in the target application.

[Workspace behavior](conversation-workspace.md) covers editing and history. [Configuration](providers-and-live-rewrite.md) lists copy, scrolling, provider and Live settings.

## Provider matrix

Choose speech and rewriting independently in **Settings → Providers**. A discovered model is not proof that an account can run it.

| Task | Route | Setup and limits |
| --- | --- | --- |
| Speech | ElevenLabs Scribe | Native realtime recognition with explicit batch fallback. Requires an account/key. Meeting diarization also uses ElevenLabs. |
| Speech | Voxtype / local Whisper | Uses an installed local model. Ordinary dictation recognizes at Stop; Live mode can preview chunks. |
| Speech | Compatible API | Needs an audio-transcription deployment accepting WAV multipart requests and returning text. A chat-only endpoint is insufficient. |
| Rewriting | Codex app-server | Uses the authenticated local client, model discovery and streaming. Inference location and credits depend on the account/model. |
| Rewriting | Compatible API | Needs a text deployment serving streaming chat completions. Optional model discovery; explicit deployment aliases are supported. |

Alternative provider routes and Live rewrite remain Experimental. Cloud compatibility depends on the endpoint, deployment, credentials and response protocol. Mluva does not provision a proxy or download models. Local speech alone does not make rewriting or titles local. See [provider setup](provider-selection.md) and the [privacy contract](product-contract.md).
