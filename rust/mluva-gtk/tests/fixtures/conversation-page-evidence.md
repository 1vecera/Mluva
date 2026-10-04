# Native conversation workspace and offline documents

Invariant observation fields now use the shared [lossless fixture storage](fixture-storage.md). Native equality and identical canonical hashes preserve every complete original observation, action and source metadata; the existing comparisons and scope below are unchanged.

The subsequent [Live workspace comparison](live-workspace-evidence.md) adds ten stability observations to this same conversation owner and nine rendering transitions to the same document owner, bringing their totals to 123 page/scroll/stability states and fifteen actual render states. The earlier observations and evidence below remain unchanged. Both owners now run through `make linux-live-workspace-test` alongside the assembled application.

The comparison reference is unchanged Mluva 1.6.0, commit `5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f`. Temporary collectors import the downloaded release outside the maintained Rust tree and compare imported source bytes against that commit. The JSON fixtures record those source digests. Native tests use the production `ConversationWorkspace`, compatible SQLite stores and ordinary GTK widgets without an interpreter, source replacement or a production test seam.

| Frozen reference | Observations | SHA-256 |
| --- | --- | --- |
| [Conversation page](released-conversation-page.json) | 86 actual released GTK actions and 16 lossless Questions splits | `8b6b1711bee8833a66e336cdf056466127678c619123f8583a86a857f6ffb382` |
| [Transcript scrolling](released-conversation-scrolling.json) | 27 actual frame-clock/adjustment states | `71eb20bb1eccdae953d2d62028367077c05944d5c523faa4090c256cba7b30f2` |
| [Document surfaces](released-document-surfaces.json) | 14 fence parses, 20 XML decisions, 36 forecast samples and six actual diagram renders | `02165ba60fbc479e425ca40297cf3f5215ff8c56952e62f66226a3402090f132` |

The page comparison covers history/search/pagination and timestamps; original/rewrite documents and heights at 320/680/1040 pixels; navigation with unsaved text and prompt drafts; Copy, Save and failed-Save recovery; prompt overrides; rename validation, cancellation and empty persisted titles; live source/draft toggles and pinned Questions; transient/private results; screenshot ownership and rounded offsets; volatile rewrite previews; and delete/merge dialogs. Its callbacks use the real native stores for rename, delete and merge. The merge positive control waits for GTK's search debounce, checks that confirmation is enabled, observes the actual merge callback and verifies combined editable text/replies while preserving the original recognition. A chooser that never commits cannot pass this fixture. The empty-title case failed before the display fallback repair; the failed-Save case retains the open draft and immutable original before a corrected Save succeeds.

Scrolling uses actual GTK adjustments and frame clocks. Three successive short captures remain at the origin with no false overflow; long captures follow the tail, user reading positions survive updates, and shortened ASR revisions preserve the reading origin. Smooth scrolling, duration zero, reduced motion and a fresh short capture are observed. No production clock or geometry is replaced. Sixteen independent Questions splits run in the ordinary document check; Unicode separator controls failed before matching the release's full whitespace definition. Extending these pure observations preserved every page action/output byte for byte.

The offline diagram check runs the pinned Mermaid bundle in a real ephemeral WebKitGTK view, converts accepted SVGs to native GDK textures and compares PNG hashes, dimensions, notices, visible text and authoritative source. Flowchart, sequence, Unicode, invalid source, rejected configuration directives and two diagrams yield five rendered textures across six states. All recorded PNG hashes match. No static SVG substitute supplies the result. Fence bounds and XML policy have separate unchanged released outputs; the initial XML implementation failed unbound-prefix, internal-entity and NUL controls before replacement with a proper XML parser. Editable source remains available after rendering failures.

Source and native runs use disposable display/session/accessibility buses, private HOME/XDG/font directories, an isolated network/PID namespace and masked input, audio and GPU device nodes. The comparison timezone is UTC. Page observations pin GTK 4.22.4 and Pango 1.58.2; the same host provides Libadwaita 1.9.3 and WebKitGTK 6.0 2.52.6. Motion/cursor blinking are disabled for stable page/diagram observations and explicitly enabled or disabled for the scrolling cases. Software EGL avoids loading the host NVIDIA display driver without device access. Logs, database files, temporary reference collectors and generated images remain private under ignored `tmp/`.

From the repository root, with the native dependencies and private runner installed:

```sh
cargo test --locked -p mluva-gtk --test document_surfaces
bash dev/run-isolated-browser.sh tmp/native-conversation -- env TZ=UTC \
  cargo test --locked -p mluva-gtk --test conversation_page \
  --test document_surfaces --test document_widget -- \
  --ignored --test-threads=1
```

The final private run passes all 113 page/scroll states and six diagram states. The separate ordinary check passes all 70 fence/XML/forecast observations and 16 Questions splits. The existing 56 document-widget observations were explicitly rerun after document sizing changed and still pass; the released mixed-CR/LF Pango warning remains disclosed in the shared fixture notes. Strict workspace Clippy, formatting and diff checks pass, and ordinary GTK tests pass after final review.

This proves the actual workspace component and recorded diagram textures, not complete conversation-page pixels, every animation/scale/theme, capture scheduling or a runnable replacement. The native capture/settings/review/meeting controllers, asynchronous session/revision guards, complete provider/application integration, global keys/widget, screenshot picker/narrated editor, optional-library and distribution contracts, Python removal and same-machine performance acceptance remain required. The installed 1.6.0 app/widget are unchanged. The full Rust goal remains active.
