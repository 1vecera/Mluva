# Native narrated screenshot text

The reference is unchanged Mluva v1.6.0 at `5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f`. [The CLI fixture](released-annotation-cli.json), SHA-256 `bd38e632d76184e1259fa81da910ac9910b6b80772baf2105781a5705128b6cc`, records 24 actual reference narration processes. Its independent observer verifies every released Python module against that commit, launches the real module entry point and observes stdin/stdout/stderr, signals, external PCM processes, loopback HTTP, private memory directories and clipboard bytes. Reference collectors remain under ignored `tmp/`; maintained acceptance runs need no Python.

The production `mluva-narrate` executable selects the existing native speech transport and freezes microphone, language, model and deterministic transcript preparation before capture. It opens no History, diagnostics, rewrite or delivery owner. Audio lives only in the existing memory-backed store with an independent EOF janitor, including when the saved retention policy is Always. The command rechecks the saved Incognito setting after Stop and before upload. A cancelable descriptor reader permits SIGTERM/SIGINT cleanup while the editor keeps stdin open; normal cleanup drains the recorder owner and closes its provider. Missing initialization resources return the released generic error without exposing provider details.

The CLI comparisons cover fresh Unicode text, frozen dictionary/snippet rules and provider settings, EOF/cancel, missing newline, CRLF/bare-CR rejection, leading whitespace, ignored trailing lines, the 32-character input bound with open ASCII and Unicode pipes, malformed UTF-8, empty/Unicode-whitespace recognition, exactly 120,000 Unicode characters and an over-limit result, provider failure, Incognito at launch/before Stop, damaged settings, SIGTERM/SIGINT and the editor's SIGKILL of the whole recording group. Exact stdout hashes/byte and character lengths, stderr, exit status, multipart fields and WAV bytes match. The observers also compare microphone arguments, 0700/0600 staging permissions, eventual child/directory disappearance, unchanged clipboard and absence of persistent data files. Three additional native faults cover cancellation while the PCM peer delays finalization, cancellation with a pending HTTP response and a missing adjacent cleanup executable. These are explicit native lifecycle checks, not additional released fault comparisons.

The [editor fixture](../../../mluva-gtk/tests/fixtures/released-editor-narration.json), SHA-256 `1af9d8bf28de9f230a6bab12b8fc75b6a31e9310146dbb80d8e6d5d4f8d66189`, observes the actual released Tensaku artifact, SHA-256 `79de63b635ec5a711bb250bb4fa7cd2dd69aeeb70dffe735a39c3669f7082831`. Its accompanying source pin and narration patch match this repository. The reference observer uses libatspi; the Rust observer reads the public accessibility D-Bus protocol independently. Both activate Add narration, select an area with private X11 input and activate Stop narration, using the actual helper, PCM recorder and compatible HTTP provider.

The native helper produces the same saved PNG pixels and independent OCR text (`Narration selected area 71`). A second box is canceled with Escape while recording; it neither uploads nor alters the PNG, including on a subsequent Save. Undo returns exactly the original pixels; Ctrl+Y restores exactly the annotated pixels. Both implementations leave the clipboard untouched, remove private audio and create no persistent data files. The final reference and native runs have clean accessibility/editor logs. The generated native PNG was also visually inspected. Evidence is retained in `tmp/editor-narration-source-evidence/` and `tmp/editor-narration-native-evidence/`.

Build the native helper and its ordinary process dependencies, then execute the explicit comparisons:

```sh
cargo build --locked -p mluva-workflows --bin mluva-narrate --bin meeting-audio-fixture-peer \
  -p mluva-audio --bin mluva-audio-cleanup
bash dev/run-isolated-browser.sh tmp/annotation-cli -- \
  cargo test --locked -p mluva-workflows --test narration -- --ignored --test-threads=1 --nocapture
bash dev/run-isolated-browser.sh tmp/annotation-editor -- \
  env MLUVA_TEST_EDITOR=/absolute/path/to/verified-v1.6.0-editor/tensaku \
      MLUVA_TEST_NATIVE_BIN_DIR=/absolute/path/to/cargo-target/debug \
  cargo test --locked -p mluva-gtk --test editor_narration -- --ignored --test-threads=1 --nocapture
```

The editor comparison requires that exact pinned release artifact, ImageMagick, Tesseract, Xclip and Xdotool. Both tests enforce private HOME/XDG/Xauthority, network/PID isolation and masked host input/audio/GPU devices. The editor uses software rendering in a private Xvfb/Openbox and accessibility session; no host focus, pointer, clipboard, physical microphone, credentials or remote provider is involved. All speech and images are synthetic.

This is the standalone native annotation command plus real editor integration. The installed wrapper still belongs to the safe v1.6.0 release. The separate [resident-process comparison](../../../mluva-gtk/tests/fixtures/bootstrap-evidence.md) now verifies main-executable dispatch and repeats all 24 CLI cases and three native faults without a display/session bus. Native packaging/install, actual Wayland/Omarchy selection and compositor keys, annotation through every physical local/provider route, joined main-recording/editor lifecycles, full acceptance and performance measurements remain required. All complete-workflow parity rows remain Pending; installed app/widget remain 1.6.0.
