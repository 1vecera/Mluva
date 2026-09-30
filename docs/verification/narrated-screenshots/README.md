# Screenshot context and narrated text boxes

Verified on 30 September 2026. This is an experimental Linux feature preview, stacked on the automatic-paste fixes in [PR #66](https://github.com/1vecera/Mluva/pull/66). The public Mluva release remains 1.5.4.

F10 invokes the existing Omarchy region picker through `mluva-shell screenshot`. Mluva attaches the selected PNG to the current recording or saved conversation and opens Tensaku. **Add narration** lets the user click or drag a text area, dictate a fresh phrase and press **Stop narration**. The resulting text box is saved into the image through the normal canvas and undo/export path. Finish the annotation before requesting a rewrite that should use its pixels. The main narration can continue recording; the annotation command does not paste, change the clipboard or create another History item.

## Validation

- Linux: 628 tests passed; Ruff, formatting, ShellCheck and generated feature documentation checks passed. The existing PyGObject deprecation warning remains.
- Patched Tensaku: 270 application tests and 2 CLI tests passed; 10 upstream tests remain ignored. Workspace Clippy passed with warnings denied. The editor was rebuilt from the pinned upstream commit and the bundled source patch with `--locked`.
- Native GTK workspace: recording stop waited for the picker; changing the visible conversation did not change attachment ownership; the model subprocess received ordered PNG bytes; saved editor changes reached follow-up rewrites. Incognito cancellation, interrupted-recording recovery, removal and History deletion passed. A 360-pixel live layout kept the controls usable.
- Actual installed files in a disposable Wayland session: the registered F10 command reached the installed bridge and application, the real Omarchy picker saved a region, and the default upgraded Tensaku opened it with the narration button. Clipboard contents survived; Escape left no attachment or temporary image.
- Actual installed editor and `mluva-narrate`: the real recorder and compatible HTTP speech transport ran against synthetic PCM and a private loopback endpoint. Stop caused exactly one upload; independent OCR read the returned phrase in the saved PNG. Escape caused no upload, cleared volatile audio and left no empty box after a subsequent save. [The exported synthetic image](annotated.png) shows the chosen area.
- Production Codex image transport: a generated image contained a blue triangle, orange circle and random number. The authenticated model identified all three from the inline pixels. No desktop content or user recording was sent for this check.

The installed application and Omarchy widget retain version metadata 1.5.4; the editor is the patched Tensaku 0.29.0. The local upgrade preserved Mluva and Tensaku settings, restarted the idle application without showing its window, and preserved focus, pointer and workspace. F9 retains the automatic-paste shortcut path from PR #66; F10 is configured with no Hyprland configuration errors. Machine identifiers, private settings and recordings are excluded from the published receipts.

## Reproduction and limits

Run the normal Linux checks with `make linux-test` and reconstruct the editor with `bash linux/build-narrated-editor.sh "$PWD/tmp/narrated-editor"`. The source pin, MPL license, notice and complete patch are under `linux/integrations/tensaku/`. Activate that editor only on an intended installation with `bash linux/install-narrated-editor.sh`.

The committed workspace scenario runs through the repository's disposable X11 helper:

```sh
env -i PATH="$PATH" HOME="$HOME" USER="$USER" LANG=C.UTF-8 \
  OFFSCREEN_ENABLE_ATSPI=1 ATSPI_DBUS_IMPLEMENTATION=dbus-daemon \
  bash dev/run-isolated.sh tmp/screenshot-workspace-check -- \
  env PYTHONPATH=linux uv run --project linux --no-sync \
  python linux/tests/screenshot_workspace_smoke.py
```

That scenario substitutes the Omarchy/editor, microphone and speech boundaries, while retaining the real GTK application, stores, bridge and app-server transport. The separate installed-file checks used private session/accessibility buses, private XDG directories and a nested headless Hyprland with a private PID namespace; no host input devices or display sockets were exposed. Both private accessibility registries were healthy. Portal fallback warnings were present in the disposable environment and did not prevent the checked native controls from exposing and executing their actions.

The configured F10 command was dispatched through Hyprland's execution API. A physical F10 or F9 press and the user's actual microphone/provider were not exercised. Annotation tests used synthetic audio, so they do not prove simultaneous real-microphone recording. X11 checks alone do not prove Wayland shortcuts. Image processing requires a selected model that supports images; text-only providers retain the existing safe fallback. Screenshots are disabled in Incognito and Scratchpad and follow their owning History entry's retention/deletion.

Raw private-session evidence remains in this task's `tmp/installed-screenshot-capture/`, `tmp/installed-editor-speech/`, `tmp/screenshot-workspace-final/`, `tmp/screenshot-layout-live/` and `tmp/visual-input-proof/`. [Curated receipts](receipts.json) record the checked boundaries without publishing host state.
