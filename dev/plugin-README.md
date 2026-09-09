# Mluva for Omarchy

**Speak freely. Stay in flow.**

Five lines of live dictation, a translucent preview that follows your theme, and Polish or Structure when you finish. Your original and every completed rewrite stay together in [Mluva](https://github.com/1vecera/Mluva).

![Mluva's real GTK conversation workspace and Omarchy widget, captured with scripted demo content](preview.png)

This is the community Quickshell plugin `mluva.dictation`, exported from Mluva **@RELEASE@**. The integration is **Experimental** while live Hyprland acceptance is completed. It is not an official Omarchy product.

## Install

Install [Mluva @RELEASE@](https://github.com/1vecera/Mluva/releases/tag/@RELEASE@) and its [Linux system dependencies](https://github.com/1vecera/Mluva/blob/@RELEASE@/linux/README.md) first. Start `mluva`, then add this plugin to Omarchy Quattro:

```sh
omarchy plugin add https://github.com/1vecera/omarchy-mluva.git --enable
```

The plugin needs Omarchy's Quickshell shell and the `mluva-shell` executable installed by Mluva. If the shell cannot find that command, set the widget's **Mluva shell executable** setting to the absolute installed path, usually `~/.local/bin/mluva-shell` expanded to your actual home directory.

Plugin installation handles the shell integration. Install and start Mluva separately, and configure F9 and Shift+F9 through the application setup. A previously copied `mluva.dictation` folder must be backed up or removed through `omarchy plugin remove mluva.dictation` before adding this Git-managed copy; the installer refuses duplicate IDs.

## Use

- Left-click the bar widget to start or stop clipboard dictation. Right-click to cancel; middle-click to open the latest conversation.
- Watch five lines while speaking. The recording surface passes pointer and keyboard input through.
- After dictation, choose **Polish**, **Structure**, or a saved prompt through **More**. Rewrites keep the original.
- Choose **Copy** to use the displayed version, or **Open** to continue in the full app. Partial rewrites cannot be copied.
- The review closes after four idle seconds by default. Configure the duration in Mluva's workspace settings. Hover, keyboard focus and menus pause it; rewriting pauses it too.

## Update or remove

```sh
omarchy plugin update mluva.dictation
omarchy plugin disable mluva.dictation
omarchy plugin remove mluva.dictation
```

Removal affects the shell plugin. Mluva's application, settings and saved conversations remain separate. The plugin itself does not write your shell configuration; Omarchy's explicit add, enable, update and remove commands manage that state.

## Data and dependencies

The QML runs inside your existing shell and invokes `mluva-shell`, which communicates with the local Mluva application over the session bus. The widget receives a bounded volatile text preview, status, elapsed time and conversation/style identifiers. It does not persist or log that stream. It does not make its own network requests or hold provider credentials.

Choose speech recognition independently from rewriting in Mluva's provider settings. Speech can use ElevenLabs Scribe, an OpenAI-compatible transcription endpoint, or local Voxtype/Whisper. Rewriting can use an authenticated Codex app-server or an OpenAI-compatible text endpoint; generated titles use the selected rewrite route. Cloud routes send audio or text to their configured provider and require that provider's credentials, supplied through your secret manager. Provider usage or account costs are separate from this open-source plugin. Voxtype/Whisper can transcribe offline after its local model is installed; cloud rewriting remains optional.

Automated evidence covers the real QML, application bridge, five-line geometry, scrolling, countdown and review actions in an isolated X11 session. That does not establish physical F9 capture, microphone quality, or live Hyprland focus behavior. Use clipboard delivery; automatic insertion is disabled by default and remains Experimental. The preview uses scripted content, not a recorded recognition benchmark.

## Source and license

Apache-2.0; see [LICENSE](LICENSE). The QML and manifest are copied byte-for-byte from Mluva's tagged source. [SOURCE.json](SOURCE.json) records the exact commit and file hashes. Improvements belong in the [Mluva repository](https://github.com/1vecera/Mluva), under `linux/quickshell/mluva.dictation`; this repository is its installable distribution.
