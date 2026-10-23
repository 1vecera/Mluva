# Annotate the screen while dictating

Show an agent a problem on screen while you describe it. While Mluva records, hold Shift and move the pointer to draw; Shift-click drops a numbered pin. When you stop, the destination receives your transcript followed by a short summary: one line per keyframe with what you were saying at that moment and the absolute path of an annotated image. Claude Code and Codex can open those paths directly.

This is Experimental and Hyprland-only for automatic Shift handling. It is on by default.

## Turn it off or on

Open **Settings → Capture → Screen annotation** and switch **Annotate the screen while dictating**. The choice saves automatically to `screen-annotations.json` beside Mluva's other settings and applies to the next recording. Ordinary dictation (F9) and agent dictation (the agent key, the session wheel and `mluva herdr record`) both support it. Phone recordings do not.

## Draw while you talk

| Gesture | Result |
| --- | --- |
| Hold Shift and move | A red stroke follows the pointer and fades shortly after you release Shift. No click or drag is needed. |
| Shift-click | A numbered pin where the pointer is. The click does not reach the application. |
| Anything else | Clicks, scrolling and typing reach your applications as usual. Shift still types capitals. |

Mluva adds the two Hyprland bindings, Shift on its own and Shift with the left button, only while an annotated recording runs, and only after the desktop's shortcut portal has approved Mluva's ink and pin actions for this run. Without that approval, for example when global shortcuts are disabled or the portal is missing or still waiting, annotation stays off for the recording and says why, so Shift-click keeps working everywhere. If Shift or Shift-click already has another Hyprland binding, annotation also stays off and says why.

Mluva removes its bindings when recording stops, is cancelled or fails, and when it quits. If any step of adding them fails, it removes them again at once. A small watchdog removes them if Mluva crashes. A cgroup kill, such as `systemctl --user stop`, systemd-oomd or `pkill mluva`, also takes the watchdog: then the next start of Mluva with annotation on, or turning the setting off, removes the bindings and the raw frames left behind.

Stopping works exactly as before. A recording without marks behaves exactly as before: it takes one frame when recording starts on Hyprland and nothing else, adds nothing to the delivered text and waits for nothing at Stop. Screen sampling, and window-change frames, begin only with the first ink or pin. Accidental pointer drift while typing a capital is not treated as a mark. If no speech is recognized, the recording completes quietly as before and its marks are discarded.

## What the agent receives

When you stop, Mluva chooses 6–12 keyframes, or fewer when the recording has fewer distinct moments. Every mark or pin gesture gets its own keyframe; strokes drawn in quick succession share one. The remaining keyframes come from settled screen and window changes and from spoken cues such as "now", "here", "look", "tady" and "podívej", each only when it shows something new. Each keyframe shows whole windows, never text cut mid-word: a marked keyframe is cropped to the window under the marks with a small margin, widened to keep every mark, and other keyframes to the window that had focus. Where Mluva has no window geometry, the whole screen is kept. The ink is composited onto a clean frame, with a `#N M:SS` badge above it. A contact sheet shows all keyframes in time order.

```text
Fix the settings page. Look at this Save button, it sits outside the card.

[Screen annotation · 0:17 · 3 keyframes · contact sheet /home/you/.local/share/mluva/annotations/3f9c2a1b/sheet.png]
- #1 (0:04) [circled] “Look at this Save button, it sits outside the card.” /home/you/.local/share/mluva/annotations/3f9c2a1b/01.png
- #2 (0:10) [underlined] “And this sync folder label runs past the right edge.” /home/you/.local/share/mluva/annotations/3f9c2a1b/02.png
```

Ordinary dictation delivers this through the existing exact-target insertion or clipboard fallback. Agent dictation sends it through the guarded Herdr send after the usual two-second cancel window; **Keep draft**, **Send draft** and **Copy draft** keep that recording's summary with the edited text, never the summaries of earlier recordings on a continued or merged conversation. Quotes in the summary come from the raw recognition, because they describe what was said. The editor and History keep only your transcript and edits, so local paths never enter account sync.

Each spoken clause is matched by time. Scribe batch recognition returns word times; for other routes Mluva places words by when the live preview grew. That is precise to within the preview interval: about a second for Scribe realtime, three seconds for local models and the chunk length for compatible providers.

## Where the files live

Each annotated recording gets a private bundle under Mluva's data directory, `annotations/<id>/`, where `<id>` is eight random hex digits: the keyframes `01.png`, `02.png`…, the contact sheet `sheet.png`, `summary.md` and `annotation.json` with marks, window-class changes, window boxes, timings and crops. The short names keep each path in the summary near 60 characters. A new bundle never takes an identifier that is already in use. Bundles from earlier releases keep their longer `annotations/<uuid>/frames/frame-NN.png` and `contact-sheet.png` paths, so summaries already delivered stay valid; they are kept, exported, recovered and removed like new ones. Window titles are never read or stored. Directories are owner-only and files are readable only by you. The bundle belongs to the conversation: deleting, merging or pruning the conversation removes or moves it, and **Copy as path** includes its keyframes in the conversation's Markdown. Raw frames grabbed while recording live only in Mluva's private runtime directory, which must be memory-backed (`XDG_RUNTIME_DIR` on a tmpfs); without one, annotation stays off rather than write screen images to disk. They are bounded in number and size and are deleted once the bundle is written. A bundle interrupted by quitting is removed, never recovered as a conversation. Nothing is uploaded or synced.

Incognito never annotates: recording continues without the overlay or bindings.

## Limits

Only Hyprland gets automatic Shift handling, pointer position for pins, window geometry for crops and focus-change keyframes. On another layer-shell desktop the overlay and frame capture work, driven by Mluva's `annotation-ink-down`, `annotation-ink-up` and `annotation-pin` application actions; nothing binds them for you. GNOME and X11 sessions record without annotation.

After you release Shift, the first click without moving the pointer is absorbed by the transparent overlay; move the pointer slightly first. A stroke begun in the same instant as pressing Shift starts about 50 ms of motion late; a brief pause before moving avoids that. Each frame grab may briefly show Hyprland's screen-sharing indicator in bars that follow screencast events. The [verification record](verification/screen-annotations/README.md) lists what has and has not been exercised.
