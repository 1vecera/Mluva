# Narrated text in Tensaku

This source patch extends Omarchy's existing screenshot editor, Tensaku 0.29.0, pinned to the commit in `upstream-commit`. It keeps the normal canvas, text styles, editable text boxes, undo stack and PNG export. The upstream MPL-2.0 license and authorship notice are preserved beside the patch.

`--narration-command EXECUTABLE` enables the **Add narration** button. Click the button, then click or drag an image area. The executable starts after selection. **Stop narration** writes `stop\n` to its stdin and closes stdin. The executable returns complete UTF-8 text on stdout and exits successfully; stderr is not displayed. A provider command must treat EOF without `stop` as cancellation. No shell expansion is used. Cancellation, an editor close, a different tool or a different text box invalidates the result and terminates the owned process.

Completed text enters the selected text box and saves through Tensaku's normal PNG export. The image is not copied to the clipboard by this action. A failed or cancelled command leaves the saved image unchanged. Output is bounded to 120,000 characters; the editor cancels after five minutes.

Build with Rust and the GTK4, libadwaita, Fontconfig, Epoxy, gtk4-layer-shell and Wayland development packages installed:

```sh
bash linux/build-narrated-editor.sh "$PWD/tmp/narrated-editor"
```

The script builds only into the chosen directory. To activate it after installing or upgrading Mluva, run `bash linux/install-narrated-editor.sh`. This builds into the Mluva data directory and links `~/.local/bin/tensaku-edit` to the installed editor wrapper. It preserves an unrelated user wrapper rather than replacing it. `MLUVA_INSTALL_HOME` supports the same isolated staging layout as the native installer. The system Tensaku binary and its existing settings are preserved. The native Mluva screenshot action uses the installed wrapper directly; Omarchy's ordinary screenshot editor also uses it when the user bin directory precedes `/usr/bin` in PATH.

The release also provides a prebuilt editor for Omarchy on x86_64. Verify the downloaded archives against `SHA256SUMS`, extract the editor archive and run `bash linux/install-narrated-editor.sh --prebuilt-dir /absolute/path/to/mluva-1.6.0-tensaku-omarchy-x86_64`. This uses the existing native-app installation and checks that the bundled source pin, patch, license and notice match this release. Other platforms should use the source build; the binary was tested with GTK 4.22.4 and Libadwaita 1.9.3.

Mluva's `mluva-narrate` command supplies a fresh phrase for each selected box. It uses the saved microphone, speech provider and language, then applies the same deterministic transcript rules. The main narration can keep recording the same words. Annotation audio remains in crash-cleaned memory-backed storage; only an explicit Stop uploads it to the chosen speech provider. EOF, editor dismissal, cancellation and Incognito before Stop return no text and erase audio. No clipboard, automatic insertion, rewrite model or extra History item is involved. The button saves the completed annotation into the PNG; finish it before asking the main narration to use the image.

Reconstruct the corresponding source with `git checkout $(cat upstream-commit)` in [Tensaku's repository](https://github.com/jondkinney/tensaku), then `git apply narration.patch`. Retain this source patch, pin, license and notice when distributing a modified binary.
