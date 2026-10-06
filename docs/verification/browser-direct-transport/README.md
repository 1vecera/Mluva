# Firefox recording and text-read privacy

The existing bus monitor could not observe direct application connections. This comparison keeps the released v1.6.0 client at `5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f` unchanged and observes the distribution's libdbus send APIs instead. Default runs unset `ATSPI_DISABLE_P2P`, `ATSPI_IN_TESTS`, `ATSPI_NO_CACHE` and `PYATSPI_NOCACHE`; the browser and both clients retain normal cache/direct-connection behavior.

The unshipped [Rust observer](../../../rust/mluva-gtk/examples/atspi_read_audit.rs) forwards all original send arguments and return values. It records only outgoing `Text.GetText` request bounds, caller PID, valid argument types and whether the connection has a registered bus name. It records no replies or text content. Nested calls for the same message are counted once; distinct nested messages remain observable. The [existing browser owner](../../../rust/mluva-gtk/tests/browser_target.rs) requires the three deliberate Command reads as positive controls, including traffic outside case intervals, and verifies their caller is the actual client. No production target, capture, restoration, authorization or delivery function is replaced.

On the monitored bus route, both observers agree exactly on `(7,17)`, `(0,2000)` and `(0,2002)`. On default direct connections, the outgoing observer records those same three requests on peer connections while the independent bus monitors see zero. Delivery, restoration, confirmation, password, stale-target and refused-paste cases issue no `GetText` request. The oversized Command retains its existing bounded read/error behavior. Disabling `LD_PRELOAD` makes the first explicit-read assertion fail, rather than accepting a blind monitor's zero count.

The untouched source's complete 21 default-transport observations are byte-identical to the original [frozen fixture](../../../rust/mluva-gtk/tests/fixtures/firefox-target-cases.json), SHA-256 `1f5042faac2213dded8f5d473d45547cd13ce5f22b29624941c69c0289ba0f96`. Native runs on both transports preserve every original snapshot, five-field DOM value, UTF-16 caret/selection, focus, input-event sequence, complete clipboard bytes and delivery receipt. The existing standalone fixture and test owner are retained; no dependency or maintained Python is added.

Run `make linux-browser-target-test` with the [existing private runner prerequisites](../../../dev/README.md). It builds the two unshipped Rust examples and selects the same existing test twice in separate bus/default sessions. The regular workspace gate leaves this GUI owner explicitly ignored; the dedicated target executes it. Temporary source collectors remain under ignored `tmp/browser-direct-transport/` and import only the external released package.

Both compared source module hashes are verified against the immutable commit after import and before exercising their production clients. The complete final source output hash remains the frozen fixture hash above. The observer source SHA-256 is `ee48ce310e61f48d66de8dec768a72abd6acae105b2055011934d5efa0b17cb0`; its retained binary is identified below.

The final comparison sessions below use private HOME/XDG, X11/Openbox, session/accessibility buses, network/PID namespaces, masked system/session activation and absent host input/audio/GPU devices. The reference and blind-control launchers explicitly mask `/run/dbus` outside the existing private browser runner, as the native application runner already does. Earlier source/control trials lacked that outer mask and remain exploratory evidence only. Firefox 154.0/build `20260818182641`, GTK 4.22.4, libatspi 2.60.6 and libdbus 1.16.2 are the observed versions. Firefox retains its unrelated offline startup diagnostics. No registration, dbind or cache failure is accepted; both successful native runs require normal peer exit and disappearance of Firefox and its status owner.

Retained session directories are local ignored scratch, not shipped fixtures. The source completed its 21-case/three-read assertion and wrote the byte-identical output; its original process handle no longer exposes an exit code. The two native routes exited 0. The blind control exited 101 at `command-utf16-selection`, requiring one actual `GetText` request but observing zero with `LD_PRELOAD` unset.

| Run | Local session directory | Complete outcomes | Actual caller / outgoing reads / bus reads |
| --- | --- | --- | --- |
| Untouched v1.6.0, default | `tmp/browser-direct-transport/source-v3/session.agrnG5` | 21, byte-identical source output | PID 62 / three peer reads / not separately monitored |
| Native, bus | `tmp/application/run.vsoOrF/bus/session.vnmI2j` | 21, every original field equal | PID 71 / three bus reads / three |
| Native, default | `tmp/application/run.vsoOrF/default/session.oDn1jG` | 21, every original field equal | PID 68 / three peer reads / zero |
| Native, deliberately blind | `tmp/browser-direct-transport/native-blind-control-v3/session.BChq3S` | Expected failure at first Command read | Observer absent / zero observed |

`make linux-test` passed 142 tests across 98 reported suites, with zero failures and 71 explicitly ignored environment-dependent tests. After the final audit-path validation and assertion-message edits, both affected native browser routes passed again, as did both strict Clippy configurations, formatting and ShellCheck. The generated-feature check passed in the workspace gate. All 65 original GTK JSON fixtures remain byte-identical to the main-branch baseline. The installed 2.1.0 manifest, all 520 files, four symlinks and running PID 4173598 remain unchanged.

The following hashes identify the retained outputs and current rebuilt observer without copying the browser corpus or logs into the repository. The observer source is unchanged; the earlier run's binary identity remains in its local handoff.

| Local artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `tmp/browser-direct-transport/source-v3.log` | 1320 | `a6aa7eb330b6b1a0c9a25b81cde8a8e0a37d4c91fbe5995d7c2006b9ad7a6328` |
| `tmp/browser-direct-transport/source-v3/session.agrnG5/source-firefox-target/source-observations.json` | 71869 | `1f5042faac2213dded8f5d473d45547cd13ce5f22b29624941c69c0289ba0f96` |
| `tmp/browser-direct-transport/source-v3/session.agrnG5/text-read-audit.jsonl` | 169 | `68b88d39866815d257adc736a59f75e47e29761033a957fc625e9a858b1988a2` |
| `tmp/browser-direct-transport/native-final-v2.log` | 2030 | `c138fd1f64766aa26a455590594a2b227abac278999abd66441e5926b683096f` |
| `tmp/application/run.vsoOrF/bus/session.vnmI2j/native-firefox-target-observations.json` | 35728 | `d9c2294341960c58f21e1c50b6422cdd84b519288a144356e77f191494f67f60` |
| `tmp/application/run.vsoOrF/bus/session.vnmI2j/text-read-audit.jsonl` | 172 | `ea4daed67c735dbad55410c813679e4f3f5d7d77c40a668e1d7838eeebc63145` |
| `tmp/application/run.vsoOrF/default/session.oDn1jG/native-firefox-target-observations.json` | 35703 | `4ec7970498c8997aae42251bb10eecfd1f5a00907f591ef2ea5c311e2085386e` |
| `tmp/application/run.vsoOrF/default/session.oDn1jG/text-read-audit.jsonl` | 169 | `cd6038576aafee69c9f821b720d2c4787c02022dc0ac0978e6365718171567a1` |
| `tmp/browser-direct-transport/native-blind-control-v3.log` | 1391 | `5e25f6831ba8c4b4ba6c4de076a7331851910fdb9b0abbef549b944e2e5f0cec` |
| `tmp/browser-direct-transport/native-gate.log` | 43901 | `4ad7b65d980fd0114021c5872b542bce99931c2c36a0b304142961d53013c4b1` |
| `tmp/browser-direct-transport/final-static-checks.log` | 238 | `8a3219a008b3f41e35f4ec23ef48895ab6d486cf10c16ead25f4f6ffc06e531b` |
| `tmp/target/debug/examples/libatspi_read_audit.so` | 4608272 | `a2840de2b54f624a3fc089d0a42ad634e33d2fbbd509dea2ec4d596c91f41dd7` |

## Recording through the application

`make linux-browser-application-test` extends the existing `released_application_portal_actions_settings_and_target_delivery` owner. It constructs the actual `ApplicationDesktop`, services, capture controller and workflow, then exercises recording through real private portal signals, the existing native PCM process and a loopback speech endpoint. The target is the real separate Firefox process; no capture, target, provider-result or delivery method is replaced.

Four independently observed v1.6.0 workflows preserve 19 complete states: global insertion, button-only copying stopped through the global action, cancellation and rejected shortcut approval with button copying. Held-key activation does not stop capture, changing the key while recording is refused, and the latest-conversation action retains its existing result. All raw recognition, output, History outcomes, full clipboard text, capture/preference labels, original audio uploads and provider requests match. The global flow replaces the selected emoji exactly once; recovery/cancellation cases preserve the browser text. Every DOM field, UTF-16 caret/selection, focus and paste/input event remains independently observed.

The new [browser fixture](../../../rust/mluva-gtk/tests/fixtures/released-application-browser.json) retains three unique complete DOM observations, one full portal trace and their 19 state references. It reuses the unchanged [GTK fixture](../../../rust/mluva-gtk/tests/fixtures/released-application-shortcuts.json), SHA-256 `1f556ecf98a75575c353b76b86daec9dc7efe5bd7c97083dddbc3c93f280fef5`, for every input, PCM byte, provider request and common UI/store field. Only the independently observed browser target and its two success-guidance fields differ. All 69 loaded source module hashes match the immutable source and the existing reference manifest. Original fixtures are neither rewritten nor rebaselined.

Each process checks zero outgoing `GetText` requests through its automatic recording flow, then requires one real explicit read at `(7,9)` as its positive control. That selection's exact released value is `🐎\uFEFF`; the extra Unicode marker is preserved. The source control uses its real application tracker; the native control uses the production tracker in the same application client. The actual reference requests travel on the registered accessibility bus, while native requests use peer connections with all four transport/cache debug flags unset. Both routes are observed; the transport choice is reported rather than forced into the fixture.

Firefox registers `org.mozilla.firefox` with the private portal before Mluva registers its own client. Both registrations and the full binding/closure trace are retained. Shutdown first requires Mluva's caller to disappear, then normal browser-peer exit, disappearance of the actual Firefox PID and accessibility status owner, and both portal callers disconnected in their observed order. Earlier reference attempts incorrectly treated the still-running browser as a leaked Mluva client; their failed traces remain exploratory. The accepted source runs close both owners normally. Native shutdown also verifies the owned audio process is gone and its recording file removed.

All four source case receipts under `tmp/application-browser/source-case{0,1,2,4}-v9/` have exit 0. The native final 19-state run has exit 0 in `tmp/application/run.BbzT1d/default/session.dxoZZA/`, retained by `tmp/application-browser/native-final.log` and `.exit`. Complete source observations and native per-state outputs remain local ignored scratch; no Python collector is maintained or shipped. The final native application run takes 80.76 seconds including four fresh browser/client processes and deliberate read controls, not a performance comparison.

The application blind control reaches all seven global-recording states unchanged, then exits 101 at `explicit GetText control must observe one real request`, observing zero with `LD_PRELOAD` unset. Its private session is `tmp/application-browser/blind-control/session.6R1rIZ`; the failed test leaves Firefox alive and Openbox's cleanup waiting. Both verified private processes were terminated to finish that negative run. This is failure-detection evidence, not normal-shutdown acceptance. Every positive run verifies normal cleanup independently.

After the final application and shared-observer-reader edits, the original GTK owner preserves all eight workflows/34 states and its pending-readiness fault in `tmp/application-browser/gtk-regression/session.3qtGmd`, exit 0. Both standalone Firefox routes preserve all 21 complete outcomes again in `tmp/application/run.c5ycql/{bus,default}/`, exit 0. `make linux-test` passes 142 tests, zero failures and 71 explicit ignores across 98 reported suites, both strict Clippy configurations, formatting and generated consistency; ShellCheck passes. Read-only review reconstructs every new source/native state from the independent fixture, validates all 69 source hashes and leaves all 65 original GTK JSON fixtures byte-identical to `origin/main`. The installed 2.1.0 manifest, 520 files, four symlinks and PID 4173598 are unchanged.

The 6,600-byte browser fixture has SHA-256 `8a86182fabfc1bb6c1a1cc667b5d2910bfe84cffcf50bbb0dc8cbb98b6a2fe2b`. These receipts identify the four full source outputs and final native verification. Exit receipts are `tmp/application-browser/source-case{0,1,2,4}-v9.exit` and the corresponding native log stems with `.exit`. The detailed local review is `tmp/application-browser/review-receipts.json`.

| Local artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `tmp/application-browser/source-case0-v9/session.pTgddf/source-observations.json` | 43884 | `745fedb206dbeadce729146826c94341e48b15f2b0392ffccdbc059a82ba238e` |
| `tmp/application-browser/source-case1-v9/session.9R0xbi/source-observations.json` | 34131 | `335dd0a3647efd4524ade8f8142fc8c1c2e8b34519e4b69e66fc259c86783c17` |
| `tmp/application-browser/source-case2-v9/session.1Isotn/source-observations.json` | 28889 | `0517d3efed6a8f52ac511888ad51c48551425cc41f62fab9e5d5f23522fa8cc3` |
| `tmp/application-browser/source-case4-v9/session.jbnYfC/source-observations.json` | 34495 | `68a3ef05c13db4c2a23ca97dbfbd01396ec874c02cf808b11c676d7dcf680bd5` |
| `tmp/application-browser/native-final.log` | 2368 | `d7685ebf3068bfe872283e992871e12fdaa6bf45f6348b0d93954ba58e300644` |
| `tmp/application-browser/gtk-regression.log` | 3545 | `e4d57bcd3e50b2eadb0d1bbf63fc19ea9c3ef773d04e4a9cf057e971b5ff3849` |
| `tmp/application-browser/standalone-final.log` | 1936 | `74fd4dc937b3cce593713040b41ac37d479453dd70bf42b0df12c7752695c975` |
| `tmp/application-browser/blind-control.log` | 1345 | `0bbc93fa9d20b24e11555372788423132034dccb50929561038e4a1bb5b0d7e6` |
| `tmp/application-browser/native-gate.log` | 43805 | `7d78f450a1bd6c4d7774f6b04284776abc53a50c4f7dcd46e8d240783bfce38d` |

The standalone corpus establishes default direct-connection behavior for the private X11 Firefox target cases; the additional application comparison establishes the four named recording workflows. Broader Live/Command/provider interleavings, Chromium, rich editors, Wayland browser focus, ambiguous/discovery-limit races, physical F9/F10/microphone and GNOME/clean-distribution acceptance remain separate requirements. The installed 2.1.0 app is unchanged. This does not establish full parity or authorize merge, release or installation.
