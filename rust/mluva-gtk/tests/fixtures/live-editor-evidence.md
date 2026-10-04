# Native Live editor finalization

`make linux-live-rewrite-test` now runs the native assembled application, conversation/document components and Live/review controllers. It has no Python setup or interpreter dependency. Four new application workflows add 21 independent states; the shared application harness now checks fifteen workflows/143 states.

The reference is unchanged v1.6.0 at `5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f`. The temporary observer imports the downloaded release outside the maintained source, verifies all 77 modules against that commit before activation, and verifies imported module hashes after shutdown. It operates the actual root application, registered actions, GTK buffers/buttons, SQLite stores, synthetic PCM process, loopback speech HTTP and native Codex JSONL peer. No application handler, scheduler, result, revision counter or delivery function supplies a substituted outcome.

[Released observations](released-application-live-editor.json) have SHA-256 `d0f4ec7d269044bfb726727cd9bd13771d0f585b3bde63fe73ac3995a1364a0b`. They retain exact source/draft/document/history/clipboard contents, notices, button visibility/sensitivity, actual D-Bus review projections, geometry, eight speech requests and nine Codex turns. Each WAV is 96,044 bytes with SHA-256 `185359fae7943409c35d1116f8a07d6009b2c873a96d08babefd52d22c98f9bc`. The PCM input is represented losslessly as one 100-ms block repeated thirty times. Provider addresses use a placeholder; prompts, model/effort, source and saved text remain exact.

| Workflow | Actual boundary and observed result |
| --- | --- |
| Manual edit during finalization | First final Codex turn is held. An edit in the real GTK buffer survives its stale response and reaches a fresh third turn. Exactly one reconciled reply is saved and automatically copied. The same registered review Copy action is refused while pending and succeeds after completion, with the clipboard reset between observations. |
| Save failure | A real SQLite `BEFORE INSERT` trigger rejects the final reply. Immutable recognition remains saved, both source and draft stay available, review reports the save error, and automatic copy does not run. The actual draft Copy button recovers the complete text after the fault is removed. |
| Copy failure | An available private `xclip` consumes and records the attempted text, then exits unsuccessfully. The reply remains saved, the prior clipboard remains intact and the notice reports failure. Removing that helper lets the actual Copy button recover the draft. |
| Cancel | The actual Live Cancel button ends a held final rewrite, retains the original and publishes the released cancellation notice. The provider process is reaped before its gate is released; no final reply or clipboard change appears afterward. |

Every workflow starts without a selected conversation and uses fifty-line source/draft documents. Frame sampling begins before Stop and continues for at least thirty actual GTK frames after the final provider request arrives. All frames keep the Live panel and draft editor mapped, with identical panel bounds and source/draft reading positions of 90/80. The final source runs each observe 57 frames; the final native run observes 57/57/57/58. Every frame matches the released geometry. The existing compact comparison separately owns finalization from a previously selected conversation, whose released selection behavior differs.

## Repair and canonical coverage

The first native application run failed at the edit-during-finalization review projection: Rust immediately published the manual edit in the widget preview, whereas the released app retained its last published preview until reconciliation. `CaptureCallbacks::live_draft_edited` duplicated the Live controller's buffer listener, and the root wired the extra callback to `refresh_review`. Removing that callback field, signal and root binding restores the released publication behavior. The Live controller still owns manual revision protection; the full third provider input and saved result prove that the edit is preserved. No production test API is added.

The pre-fix failure is `tmp/native-live-editor-first-application.log`, with actual receipts in `tmp/application/run.YKcoKT/session.xYIW3j/native-live-editor-manual-edit/`. The unchanged released observations pass after the repair. Final review strengthened the hold from a total frame count to thirty frames after request arrival, then recollected all four released workflows. Every state, request, turn and geometry value remained identical.

The obsolete callback-only capture-controls case is removed with its field and seven no-op initializers. That case recorded a substituted `draft-edit` notification without observing revision protection, a provider request or saved text. All other 77 capture/model/control observations remain byte-for-byte unchanged. The actual Live controller and new root transaction own the editing contract.

The retired `linux/tests/live_workspace_smoke.py` contained 437 lines. Its unchanged 43-line `paint`/`settle` helpers move into the existing `conversation_ui_smoke.py`; eight remaining consumers import them there. The old Make dependency and runner branch are removed. The retained contracts have these owners:

| Retired assertion | Stronger remaining owner |
| --- | --- |
| Save an edited source without replacing raw recognition; complete long editor; hidden Copy/Save | `conversation_page`, including editor heights, failed Save and current-text actions; root `application_commands` for actual keyboard dispatch |
| Rewrite from an edited document; copy defaults and streaming | `review_controller`, with exact provider input, SQLite/clipboard results and cancellation/privacy/selection guards |
| Preview/manual revision races, coalesced Stop, matching-final reconciliation, frozen settings, partial/paused labels, Once and Incognito | `live_controller` plus its joined capture cases: 26 transactions/122 states, exact prompts and real child cleanup |
| Final panel/scroll stability, manual-final edit, save/copy failure and cancellation | New assembled Live editor workflows above |
| Narrow editor and template/settings presentation | Existing compact, application Live workspace, conversation and capture/preferences owners |

## Verification and limits

Final source logs are `tmp/native-live-editor-reference-final-<case>.log`. Each source case uses a fresh disposable X11 session. Native application evidence is `tmp/application/run.KAymxH/session.cEaHSa/`, components `tmp/application/run.XMfKXl/session.944ReP/`, and controllers `tmp/application/run.cZKX4J/session.SrH1MZ/`.

Actual `make linux-live-rewrite-test` passes in `tmp/native-live-editor-final-make.log` with both `/usr/bin/python3` and `uv` traps positively verified first and unused afterward. The old route fails at the interpreter trap in `tmp/native-live-editor-before.log`. The final run covers fifteen application workflows/143 states, the existing command/settings comparison, 123 conversation states, fifteen renderer states, 26 Live/capture transactions/122 states, and 29 document/review transactions/102 states plus its owner-exit fault.

Affected sibling checks pass in `tmp/native-live-editor-siblings.log`: 77 capture/model controls, Command/Notes, six capture-factory transactions/eight states, 46 capture transactions/218 states plus three native faults, and seventeen preference workflows/132 states. All eight retained Python consumers import successfully; the existing feedback UI smoke passes all six assertions and writes usable window captures with the moved helpers. Its split workspace image was inspected. Evidence is under `tmp/native-live-editor-siblings-evidence/`, session `yh7jqQ`.

Actual `make test` passes 139 tests, zero failures and 68 ignored environment checks across 98 suites, both strict Clippy configurations, formatting and generated-document consistency. Relevant ignored GUI checks run explicitly above. Actual `make linux-python-test` passes 571 tests, Ruff and formatting for 155 files. Full ShellCheck and diff checks pass. Logs are `tmp/native-live-editor-final-native.log`, `tmp/native-live-editor-final-python.log` and `tmp/native-live-editor-final-gates.log`; gate session is `tmp/native-live-editor-final-gates-evidence/session.0tcbp5/`. Existing narrow GtkText allocation, transient AT-SPI startup and PyGObject deprecation warnings remain; no assertion is waived.

Production removes seven lines. Make/runner tooling is +11/-7; tests/support are +336/-466; independent JSON observations are +1,416/-201. Documentation is separate. No maintained Python helper is introduced, and the retained shared helpers still belong to the unfinished Python conversion.

GTK 4.22.4, Pango 1.58.2 and Libadwaita 1.9.3 run on private X11/Openbox with software Mesa. HOME/XDG, accessibility/session buses, clipboard, network/PID/mount namespaces are isolated; physical input/audio/GPU devices and the system bus are masked. No host input, microphone, credentials, user upload or installed-app change occurs. CI remains manual-only and no hosted runner was used. Complete joined workflows, remaining Python/tooling removal, clean-distribution ABI checks, Wayland/physical-device acceptance and final performance measurements remain open. Every whole-workflow matrix row stays Pending; installed app/widget remain 1.6.0.
