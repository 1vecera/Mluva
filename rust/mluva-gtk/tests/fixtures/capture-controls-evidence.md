# Native capture page and rewrite controls

The comparison reference is unchanged Mluva 1.6.0 at `5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f`. The temporary collector imports the frozen downloaded release outside the maintained Rust tree and verifies the relevant application, GTK, catalog and document source bytes against that commit. [Released observations](released-capture-controls.json) have SHA-256 `1745f06fbeff6b52f7371005e9de7aa4ef380b7c87a4ae37b3fb05431aeb36ae`.

The ignored native owner constructs the actual production `RewriteSettings`, `ThinkingRow` and `CapturePage`, including its real `ConversationWorkspace`, compatible empty SQLite stores, recovery editor and recording dock. Ordinary public GTK fields and signals drive the comparison; there is no production test seam or interpreter in the native check.

| Surface | Independently released observations |
| --- | --- |
| Rewrite model/thinking controls | 38 states: discovery/loading/failure, default/explicit/hidden/stale selections, identifier aliases, unavailable reasoning levels, Fast support and stale-Fast removal, provider/endpoint/key-variable changes, compatible-server aliases/defaults, selected-model effort reset, refresh/show callbacks and Unicode labels |
| Capture controls/recovery | 40 states: Off/Once/Continuous cycle, template choice and rejected changes, Skip, returned shortcut descriptions and tracking availability, status/error/retry callouts, hidden Dictation results, unresolved Command/Notes actions, Unicode whitespace, edits and callback dispatch |

The same actual released application builder constructs its capture page. Provider discovery, persistence, recording/review services, settings-page construction and window announcements are explicit external boundaries in this component comparison. The collector retains the original Live-control, status, error and output-visibility methods; controlled callbacks record requests, accept/reject a settings change and publish the accepted configuration. Native callbacks use the same boundary contract. A record click proves callback dispatch, not microphone capture or an asynchronous capture transaction. This comparison does not claim the remaining services or controllers were replaced or verified.

The native controls use the existing provider model type and selector. Catalog notifications never save preferences; a user model change clears its effort, a failed discovery retains usable choices, and a stale Fast selection remains recoverable. Unicode 16 lower/upper behavior comes from the native core's frozen properties; 135 first-character titlecase overrides preserve released capitalization expansions for labels. `resources/titlecase-first.json` has SHA-256 `611d65656ef1a4447f35c85c811077b8dc0a0041b2d91d11a29154531d49dd11`; the temporary generator checks the reference runtime's Unicode version. Actual released widget outputs independently check sharp S, titlecase digraphs, dotted I, final Greek sigma and Greek expansions.

The source and native runs use disposable display/session/accessibility buses, private HOME/XDG/font directories, separate network/PID namespaces and masked input/audio/GPU devices. GTK 4.22.4 and Pango 1.58.2 are recorded and checked; motion/cursor blinking are disabled equally. Model names and endpoints are synthetic; no provider account, user content, microphone, clipboard or host input is used. Raw logs and temporary collectors remain private under ignored `tmp/`.

```sh
bash dev/run-isolated-browser.sh tmp/native-capture-controls -- env TZ=UTC \
  cargo test --locked -p mluva-gtk --test capture_controls \
  --test conversation_page -- --ignored --test-threads=1
```

The final private run passes all 78 capture/control states and repeats all 113 conversation/scroll states after connecting the page. The final workspace run passes 118 ordinary native tests and deliberately ignores seventeen environment-dependent checks. Strict workspace Clippy, formatting and diff checks pass. Earlier actual diagram, document, recording, target/browser and inference checks retain their own scope and evidence.

The capture view is native. A subsequent [joined capture comparison](capture-lifecycle-evidence.md) now exercises asynchronous preparation/recording/session ownership with synthetic PCM and real batch/realtime transports. Production factory assembly, local preview/segment production, model discovery/persistence integration, Live/review/meeting/archive controllers, complete settings/welcome/application assembly, announcements, pixels/other scales, keys/widget, screenshot picker/narrated editor, packaging/Python removal and same-host performance acceptance remain pending. The installed app/widget remain verified 1.6.0 and the full Rust goal stays active.
