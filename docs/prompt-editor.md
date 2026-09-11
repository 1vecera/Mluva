# Editing prompts

Open **Settings → Prompts**, or press **Ctrl+P** and search `Edit prompt` plus a name, such as `Grilling`. A small settings button also appears when you hover or keyboard-focus a Live template, Polish/Structure action, or saved style. Tab to that button and press Space to edit without running the prompt. The full catalog stays available even without a conversation.

The native editor shows the prompt's purpose, exact file path, and whether you are viewing its built-in default, original saved text, or a local override. It preserves multiline Markdown and literal braces. **Save** applies an override; **Cancel** or Escape asks before discarding changed text. **Restore default** (or **Restore original** for a custom prompt) stages the baseline; Save confirms the reset. Validation and disk errors leave your draft in place. If the file changed externally, copy your draft before cancelling and reopening; Mluva will not overwrite that external edit.

## Local configuration

Prompt overrides are UTF-8 Markdown files under `$XDG_CONFIG_HOME/mluva/prompts/`, normally `~/.config/mluva/prompts/`. Each identifier below resolves to `<identifier>.md`; this filename convention is the configuration reference, so no separate manifest can drift out of sync. Missing files use their built-in default or original custom text. Only overrides are written, atomically and owner-only, by the editor. To start from the default, open it in the UI and press Save without changing the text; this creates its local file. Create the same file in your text editor to override locally; remove it to restore the baseline. The app never interprets braces as interpolation or Markdown as configuration syntax.

| Prompt | Identifier / filename without `.md` |
| --- | --- |
| Live Grilling, Task spec, Structured note, Polish, Custom | `live-grilling`, `live-task-spec`, `live-structured-note`, `live-polish`, `live-custom` |
| Initial Task spec and Structured note Markdown | `template-task-spec`, `template-structured-note` |
| On-demand Polish and Structure | `rewrite-polish`, `rewrite-structure` |
| Automatic conversation title | `title` |
| Optional faithful dictation cleanup | `cleanup` |
| Message, Google Chat, Tasks, Email, Prose, Technical notes, Prompt, and saved custom styles | `style-<lowercase UUID>`; the editor shows the exact path |

For example, create `~/.config/mluva/prompts/live-custom.md` containing your multiline instructions, then select Live → Custom. Use `live-grilling.md` to change Grilling instructions. Its Questions pinning still recognizes the exact `## Questions` and `## Architecture` headings; keep those headings if you want the pinned presentation. Default text can be inspected in the editor without creating an override; repository defaults live in `linux/mluva_linux/prompt_defaults.py` and `personalization.py`.

Each prompt is limited to 8,000 characters; empty text is accepted only for Live Custom, which cannot run until instructions are provided. Invalid UTF-8, control characters, oversized or empty required prompts produce an error in the editor and Settings catalog. Requests use the original baseline while the invalid file stays intact. The editor shows readable invalid file contents for repair. Unreadable files must have their permissions repaired locally before saving.

## Existing installations and precedence

The precedence is **valid `.md` override → original custom text → built-in default**. Existing `config.json` → `live_rewrite_custom_instructions` and `personalization.json` → saved-style instructions remain lossless compatibility/recovery baselines. There is no destructive startup migration and no rewritten copy of those originals. Once an override exists, edits belong in that file or the shared UI editor; baseline JSON does not compete with it. Creating a named prompt still records its name, UUID and original text in `personalization.json`; subsequent instruction edits use the corresponding `.md` override. Style selections retain their UUIDs. Removing a custom style removes its catalog entry; its override file is retained for recovery and is no longer executed.

Other settings remain in `~/.config/mluva/config.json`. If that JSON is malformed, Mluva opens with defaults and a repair notice, preserves the original file, and refuses to overwrite it through settings. Prompt editing remains available. Repair that JSON and restart to reload its settings. A malformed personalization file is likewise retained by its existing recovery behavior; repair it and restart to recover its custom-style catalog.

## When changes take effect

Files are re-read when a prompt is opened or a new on-demand rewrite/title is requested; no restart or file watcher is needed. An already running request keeps its constructed prompt. Live and cleanup instructions are frozen at recording start, including the structures for other templates. Saving a prompt during recording does not restart the provider, change the draft, reset its revision, or discard in-flight work. Toggling Live or switching its template keeps that recording's prompt snapshot; the next recording reads the new files. Existing session, revision and cancellation checks still reject stale provider results after manual edits, cancellation or navigation changes.

Incognito permits inspection but disables prompt saves and resets. Raw recognition, exact Markdown/Mermaid source, Grilling pinning, copy/save behavior and finalization gates are unchanged. One-off rewrite instructions remain directly editable in the conversation composer; use its Save prompt action to give one a durable name. Spoken Command instructions are the user's captured request, not a configurable template. Dictionary/snippet contents remain in Vocabulary.

The fixed application rules are intentionally separate: response shape and limits, source-versus-instruction boundaries, no-tool rules, fact/token integrity checks, and provisional/final transcript reconciliation are not prompt overrides. Cleanup customization still passes the existing integrity checks. Title output still has the fixed 64-character, single-line limit. Editing task instructions does not bypass these contracts.

## Verification

`make linux-prompt-test` exercises native hover/focus, actual Ctrl+P deep links, Save/Cancel/reset, validation and external conflicts, persisted text, active-session snapshots and Incognito on a private X11 display. The viewport matrix includes 420×520, 480×640 and 1060×780, with dark/light and malformed-config scenarios. Evidence is synthetic and stays under `tmp/prompt-editor/`. Xvfb verifies native Linux UI behavior, not physical Wayland shortcuts or live provider quality. This guide covers the Linux client.
