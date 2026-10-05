# Screenshot context and narrated text boxes

6 October 2026: [the current Wayland renewal](../current-wayland-screenshots/README.md) compares unchanged v1.6.0 with the exact installed Rust 2.1.0 package. Actual virtual F10, real Omarchy/Hyprpicker/Slurp/Grim selection, fresh Tensaku text-box narration, complete PNG/RGBA/WAV/OCR outputs, clipboard/History preservation, Escape cleanup and normal Quit match across eight states. Native app/helper execution uses positively verified Python/uv traps. The 1.5.4 receipts below remain historical; physical keys/microphone and complete platform acceptance are still unverified.

Historical preview verified on 30 September 2026, stacked on the automatic-paste fixes in [PR #66](https://github.com/1vecera/Mluva/pull/66). Version statements and installed-file receipts below describe that checkpoint, when the public release was 1.5.4. The current Rust-port reference is v1.6.0; [native joined comparison evidence](../../../rust/mluva-gtk/tests/fixtures/application-images-evidence.md) records the current application/editor check.

F10 invokes the existing Omarchy region picker through `mluva-shell screenshot`. Mluva attaches the selected PNG to the current recording or saved conversation and opens Tensaku. **Add narration** lets the user click or drag a text area, dictate a fresh phrase and press **Stop narration**. The resulting text box is saved into the image through the normal canvas and undo/export path. Finish the annotation before requesting a rewrite that should use its pixels. The main narration can continue recording; the annotation command does not paste, change the clipboard or create another History item.

## Validation

- Linux: 628 tests passed; Ruff, formatting, ShellCheck and generated feature documentation checks passed. The existing PyGObject deprecation warning remains.
- Patched Tensaku: 270 application tests and 2 CLI tests passed; 10 upstream tests remain ignored. Workspace Clippy passed with warnings denied. The editor was rebuilt from the pinned upstream commit and the bundled source patch with `--locked`.
- Native GTK workspace: recording stop waited for the picker; changing the visible conversation did not change attachment ownership; the model subprocess received ordered PNG bytes; saved editor changes reached follow-up rewrites. Incognito cancellation, interrupted-recording recovery, removal and History deletion passed. A 360-pixel live layout kept the controls usable.
- Actual installed files in a disposable Wayland session: the registered F10 command reached the installed bridge and application, the real Omarchy picker saved a region, and the default upgraded Tensaku opened it with the narration button. Clipboard contents survived; Escape left no attachment or temporary image.
- Actual installed editor and `mluva-narrate`: the real recorder and compatible HTTP speech transport ran against synthetic PCM and a private loopback endpoint. Stop caused exactly one upload; independent OCR read the returned phrase in the saved PNG. Escape caused no upload, cleared volatile audio and left no empty box after a subsequent save. [The exported synthetic image](annotated.png) shows the chosen area.
- Production Codex image transport: a generated image contained a blue triangle, orange circle and random number. The authenticated model identified all three from the inline pixels. No desktop content or user recording was sent for this check.

At that checkpoint, the installed application and Omarchy widget retained version metadata 1.5.4; the editor was the patched Tensaku 0.29.0. The local upgrade preserved Mluva and Tensaku settings, restarted the idle application without showing its window, and preserved focus, pointer and workspace. F9 retained the automatic-paste shortcut path from PR #66; F10 was configured with no Hyprland configuration errors. Machine identifiers, private settings and recordings are excluded from the published receipts.

## Reproduction and limits

Run the normal Linux checks with `make linux-test` and reconstruct the editor with `bash linux/build-narrated-editor.sh "$PWD/tmp/narrated-editor"`. The source pin, MPL license, notice and complete patch are under `linux/integrations/tensaku/`. Activate that editor only on an intended installation with `bash linux/install-narrated-editor.sh`.

The former Python workspace scenario is retired. Its contracts now run through the native disposable X11 target, which also launches the actual pinned editor and Rust annotation helper:

```sh
MLUVA_TEST_EDITOR=/absolute/path/to/verified-v1.6.0-editor/tensaku \
  make linux-screenshot-test
```

The current target uses external synthetic selection, PCM and provider inputs while retaining the actual GTK application, stores, bridge, app-server transport, editor wrapper and Tensaku canvas. The separate historical installed-file checks used private session/accessibility buses, private XDG directories and a nested headless Hyprland with a private PID namespace; no host input devices or display sockets were exposed. Both private accessibility registries were healthy. Portal fallback warnings were present in that disposable environment and did not prevent the checked native controls from exposing and executing their actions.

The configured F10 command was dispatched through Hyprland's execution API. A physical F10 or F9 press and the user's actual microphone/provider were not exercised. Annotation tests used synthetic audio, so they do not prove simultaneous real-microphone recording. X11 checks alone do not prove Wayland shortcuts. Image processing requires a selected model that supports images; text-only providers retain the existing safe fallback. Screenshots are disabled in Incognito and Scratchpad and follow their owning History entry's retention/deletion.

Raw private-session evidence remains in this task's `tmp/installed-screenshot-capture/`, `tmp/installed-editor-speech/`, `tmp/screenshot-workspace-final/`, `tmp/screenshot-layout-live/` and `tmp/visual-input-proof/`. [Curated receipts](receipts.json) record the checked boundaries without publishing host state.
