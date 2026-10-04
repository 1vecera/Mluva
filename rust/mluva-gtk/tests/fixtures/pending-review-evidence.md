# Released Command and Notes decisions

The unchanged reference is Mluva v1.6.0 at `5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f`. [The fixture](released-pending-review.json), SHA-256 `c3a6b6f38b62d20deaf7b56d213ce5ac2e5ba72e4cd0f5356314ffd3fbe16dd2`, records nineteen actual released application workflows/71 GTK, store and target states. The external observer verifies imported source bytes against the pinned commit. It supplies a synthetic completed WorkflowResult at the public completion boundary, then invokes the actual application completion, acceptance and recovery handlers through real GTK signals. It does not replace those handlers or exercise a speech/rewrite request in this comparison.

PendingReview now owns explicit Command preview decisions and durable editable Notes. It connects the actual capture/preferences widgets, shared services and HistoryController. Command acceptance restores only its captured target, retains preview after delivery failure, records the receipt and frozen audio-retention choice, and releases selected-text context after a decision. Discard preserves the clipboard and target. Notes edits save immediately; Copy trims the same released whitespace and resolves only after clipboard delivery and History persistence succeed. Confirmed deletion coordinates the draft, History record and retained audio. Recovery restores the draft and mode; Incognito retains editable text without durable recovery or diagnostic content.

Three cases use a separate real GTK editor on the private accessibility bus: replace a selected emoji, insert at the captured caret, and copy after the captured process exits. Actual text, selection/caret and edit counts are compared. Every state reads the real private clipboard. The other observations include exact status/disclosure/button text, editor and control availability, archive counts, complete History/draft documents, diagnostic outcome tuples, callback order and audio/file existence. Missing History during Command acceptance/discard keeps the released key-specific warning; failed Notes resolution leaves the draft available. Frozen generated draft UUID/time fields and private roots are normalized; fixed History/session IDs remain literal, timings retain presence, and peer PID/request serials are excluded.

Malformed recovery files remain byte-for-byte unchanged. Three independent malformed-object cases preserve the released repair message and character position across ASCII, Unicode and multiple lines. Loading now parses the entire JSON document before typed fields, so an earlier unknown key cannot conceal a later syntax error. Other parser diagnostics retain their native detail; these observations do not claim identical wording for every malformed JSON shape.

The native test additionally closes an unresolved owner after a failed Notes acceptance, invokes actual Copy/Delete buttons and recovery, and verifies no clipboard, diagnostic, dialog, draft or History change. Remaining capture-factory, History and Meeting comparisons are repeated because the shared graph and preference-activity restoration changed. The graph supplies actual pending-review activity before enabling controls, including cleanup/style guards.

Reproduce with the documented native environment inside the disposable helper:

```sh
cargo build --locked -p mluva-gtk --example text_target_peer
bash dev/run-isolated-browser.sh tmp/native-pending-review -- \
  cargo test --locked -p mluva-gtk --test pending_review -- --ignored --test-threads=1 --nocapture
```

The helper isolates the display, HOME/XDG, session/accessibility buses, network/PID/mount namespaces and input/audio/GPU devices. All content and retained bytes are synthetic. Temporary released observers/raw logs stay ignored under `tmp/`; native verification consumes static JSON without an interpreter. Private AT-SPI stale-cache warnings after peer exit remain in the logs. No host clipboard, input, microphone or visible desktop is used.

This establishes bounded pending-review ownership. Complete root assembly, actual capture-to-review in the final application, full visual/accessibility/platform acceptance, screenshot/shortcut lifecycle, packaging/Python removal and same-host performance remain required. Every complete-workflow matrix row remains Pending. No Rust installation, merge or release is claimed.
