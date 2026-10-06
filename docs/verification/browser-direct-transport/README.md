# Firefox default accessibility transport

The existing bus monitor could not observe direct application connections. This comparison keeps the released v1.6.0 client at `5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f` unchanged and observes the distribution's libdbus send APIs instead. Default runs unset `ATSPI_DISABLE_P2P`, `ATSPI_IN_TESTS`, `ATSPI_NO_CACHE` and `PYATSPI_NOCACHE`; the browser and both clients retain normal cache/direct-connection behavior.

The unshipped [Rust observer](../../../rust/mluva-gtk/examples/atspi_read_audit.rs) forwards all original send arguments and return values. It records only outgoing `Text.GetText` request bounds, caller PID, valid argument types and whether the connection has a registered bus name. It records no replies or text content. Nested calls for the same message are counted once; distinct nested messages remain observable. The [existing browser owner](../../../rust/mluva-gtk/tests/browser_target.rs) requires the three deliberate Command reads as positive controls, including traffic outside case intervals, and verifies their caller is the actual client. No production target, capture, restoration, authorization or delivery function is replaced.

On the monitored bus route, both observers agree exactly on `(7,17)`, `(0,2000)` and `(0,2002)`. On default direct connections, the outgoing observer records those same three requests on peer connections while the independent bus monitors see zero. Delivery, restoration, confirmation, password, stale-target and refused-paste cases issue no `GetText` request. The oversized Command retains its existing bounded read/error behavior. Disabling `LD_PRELOAD` makes the first explicit-read assertion fail, rather than accepting a blind monitor's zero count.

The untouched source's complete 21 default-transport observations are byte-identical to the original [frozen fixture](../../../rust/mluva-gtk/tests/fixtures/firefox-target-cases.json), SHA-256 `1f5042faac2213dded8f5d473d45547cd13ce5f22b29624941c69c0289ba0f96`. Native runs on both transports preserve every original snapshot, five-field DOM value, UTF-16 caret/selection, focus, input-event sequence, complete clipboard bytes and delivery receipt. No new source oracle, test owner, dependency or maintained Python is added.

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

The following hashes identify the retained artifacts without copying the browser corpus or logs into the repository.

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
| `tmp/target/debug/examples/libatspi_read_audit.so` | 4608272 | `9f7468f761de527c631ef3b1813007edcf09a3f9a478141c17d976fbdde91f30` |

This establishes default direct-connection behavior for the existing private X11 Firefox corpus. Actual recording/application controllers, Chromium, broader rich editors, Wayland browser focus, ambiguous/discovery-limit races, physical F9/F10/microphone and GNOME/clean-distribution acceptance remain separate requirements. The installed 2.1.0 app is unchanged. This does not establish full parity or authorize merge, release or installation.
