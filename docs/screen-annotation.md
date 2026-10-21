# Annotate the screen while dictating

Show an agent a problem on screen while you describe it. While Mluva records, hold Shift and move the pointer to draw; Shift-click drops a numbered pin. When you stop, the destination receives your transcript followed by a short summary: one line per keyframe with what you were saying at that moment and the absolute path of an annotated image. Claude Code and Codex can open those paths directly.

This is Experimental and Hyprland-only for automatic Shift handling. It is off by default.

## Turn it on

Open **Settings → Capture → Screen annotation** and enable **Annotate the screen while dictating**. The choice saves automatically to `screen-annotations.json` beside Mluva's other settings and applies to the next recording. Ordinary dictation (F9) and agent dictation (the agent key, the session wheel and `mluva herdr record`) both support it. Phone recordings do not.

## Draw while you talk

| Gesture | Result |
| --- | --- |
| Hold Shift and move | A red stroke follows the pointer and fades shortly after you release Shift. No click or drag is needed. |
| Shift-click | A numbered pin where the pointer is. The click does not reach the application. |
| Anything else | Clicks, scrolling and typing reach your applications as usual. Shift still types capitals. |

Mluva adds the two Hyprland bindings, Shift on its own and Shift with the left button, only while an annotated recording runs. It removes them when recording stops, is cancelled or fails, and when Mluva quits. A small watchdog removes them if Mluva exits unexpectedly, and the next annotated recording replaces any that remain. If Shift or Shift-click already has another Hyprland binding, annotation stays off for that recording and says why.

Stopping works exactly as before. A recording without marks behaves exactly as before and creates nothing extra. Accidental pointer drift while typing a capital is not treated as a mark. If no speech is recognized, the recording completes quietly as before and its marks are discarded.

## What the agent receives

When you stop, Mluva chooses 6–12 keyframes, or fewer when the recording has fewer distinct moments. Every mark or pin gesture gets its own keyframe; strokes drawn in quick succession share one. The remaining keyframes come from settled screen and window changes and from spoken cues such as "now", "here", "look", "tady" and "podívej", each only when it shows something new. Each keyframe is cropped to the activity, with the ink composited onto a clean frame and a `#N M:SS` badge above it. A contact sheet shows all keyframes in time order.

```text
Fix the settings page. Look at this Save button, it sits outside the card.

[Screen annotation · 0:17 · 3 keyframes · contact sheet /…/annotations/<id>/contact-sheet.png]
- #1 (0:04) [circled] “Look at this Save button, it sits outside the card.” /…/annotations/<id>/frames/frame-01.png
- #2 (0:10) [underlined] “And this sync folder label runs past the right edge.” /…/annotations/<id>/frames/frame-02.png
```

Ordinary dictation delivers this through the existing exact-target insertion or clipboard fallback. Agent dictation sends it through the guarded Herdr send after the usual two-second cancel window; **Keep draft**, **Send draft** and **Copy draft** keep the summary with the edited text. The editor and History keep only your transcript and edits, so local paths never enter account sync.

Each spoken clause is matched by time. Scribe batch recognition returns word times; for other routes Mluva places words by when the live preview grew. That is precise to within the preview interval: about a second for Scribe realtime, three seconds for local models and the chunk length for compatible providers.

## Where the files live

Each annotated recording gets a private bundle under Mluva's data directory, `annotations/<id>/`: `frames/frame-NN.png`, `contact-sheet.png`, `summary.md` and `annotation.json` with marks, events, timings and crops. Directories are owner-only and files are readable only by you. The bundle belongs to the conversation: deleting, merging or pruning the conversation removes or moves it, and **Copy as path** includes its keyframes in the conversation's Markdown. Raw frames grabbed while recording stay in Mluva's private runtime directory until the bundle is written and are then deleted. Nothing is uploaded or synced.

Incognito never annotates: recording continues without the overlay or bindings.

## Limits

Only Hyprland gets automatic Shift handling, pointer position for pins, active-window context and focus-change keyframes. On another layer-shell desktop the overlay and frame capture work, driven by Mluva's `annotation-ink-down`, `annotation-ink-up` and `annotation-pin` application actions; nothing binds them for you. GNOME and X11 sessions record without annotation.

After you release Shift, the first click without moving the pointer can be absorbed by the transparent overlay; move the pointer slightly first. Each frame grab may briefly show Hyprland's screen-sharing indicator in bars that follow screencast events. The [verification record](verification/screen-annotations/README.md) lists what has and has not been exercised.
