# Optional WebKitGTK

The native app and narration command no longer link WebKitGTK or JavaScriptCore at startup. `MermaidPreview` loads the system WebKitGTK 6.0 renderer on its first diagram request. Missing libraries leave the document editable and display the unchanged released notice. When available, rendering still uses an ephemeral WebKit view, disabled local storage/page caching, the same navigation restrictions, offline Mermaid assets and native textures.

The private loader resolves the required C interface before creating any WebKit objects and retains the library for the process lifetime, as GObject types require. GTK owns the widgets on its main thread; Rust closures retain only weak preview owners. Allocated JavaScript strings use GLib ownership, and navigation request/URI pointers remain borrowed only during the policy callback. The interface was reviewed against the installed WebKitGTK 2.52.6 headers/GIR and primary [JavaScript evaluation](https://webkitgtk.org/reference/webkitgtk/unstable/method.WebView.evaluate_javascript.html), [ephemeral network session](https://webkitgtk.org/reference/webkitgtk/unstable/class.NetworkSession.html) and [script-message signal](https://webkitgtk.org/reference/webkitgtk/unstable/signal.UserContentManager.script-message-received.html) documentation. No extra renderer process wrapper, Python bridge or production test switch was introduced.

## Reference and regression

The unchanged reference is v1.6.0, commit `5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f`. [The frozen optional-renderer fixture](released-optional-webkit.json), SHA-256 `5c85e451ed64f7b5d6804a5664fe8aca39951e2094da92dd9df290c43cf51133`, records three actual released GTK states: initial diagram source, edited Unicode source and replacement with ordinary text. It includes verified source hashes. The temporary observer remains in ignored `tmp/export-optional-webkit.py` and imports the separate downloaded release without changing it.

In a guarded private session, a nested mount withholds WebKit's typelib from the reference and replaces the renderer library with an unloadable file. The released editor remains usable, preserving exact source/visible text, editability, notice, preview visibility and the absence of pictures/WebKit views. The pre-fix native bundle fails before `--help` with status 127 (`tmp/optional-webkit-before.log`). The repaired main executable matches the released help; the actual native document matches all three states with the library still masked and absent from `/proc/self/maps`.

The normal-library comparison independently reruns [all six released diagram states](conversation-page-evidence.md). Five PNG hashes/dimensions, visible/source text and failure notices still match. Its first run caught an incorrect read of WebKit's write-only settings property; using the supported C getter fixes it (`tmp/optional-webkit-native.log`, followed by the passing final run). Frozen reference outputs did not change.

## Verification

Build the production binaries first, then run with the documented private-runner prerequisites:

```sh
bash dev/run-isolated-browser.sh tmp/native-optional-webkit -- bash -c '
  set -e
  cargo test --locked -p mluva-gtk --test document_surfaces -- --ignored --nocapture
  bash rust/mluva-gtk/tests/support/without-webkit.sh \
    cargo test --locked -p mluva-gtk --test optional_webkit -- --ignored --nocapture
'
```

The mask helper is scoped to the recorded Arch library path and requires the guarded private session. It masks only the native shared library and has no introspection/Python requirement. Accepted component evidence is `tmp/optional-webkit-final-evidence/session.COoXrj/` and `tmp/optional-webkit-final.log`, with the simplified native-only mask rechecked in `tmp/optional-webkit-mask-evidence/` and `tmp/optional-webkit-mask.log`.

The complete nine-binary package also passes construction/inventory checks, actual private installation/self-upgrade/public help and the resident comparison while WebKit is unavailable. Resident startup matches 38 public states, 13 actions and five CLI contracts, including second-instance forwarding, cold synthetic-PCM Record/quit and three startup faults. Packaging/installation use the actual production binaries; no real audio device, clipboard, host desktop, provider account or system installation is touched. The full workspace passes 137 ordinary tests, zero failures and 60 ignored environment checks across 94 suites (`tmp/optional-webkit-workspace.log`). Strict all-target Clippy (`tmp/optional-webkit-clippy.log`), formatting, diff and ShellCheck pass.

This closes the specific optional-renderer launch regression. It does not establish every distribution ABI, complete page/scale parity, physical capture/keys or an overall performance improvement. Final source/distribution conversion, complete workflow acceptance and removal of the maintained Python tree remain required.
