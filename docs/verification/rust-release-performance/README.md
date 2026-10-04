# Released native startup and rendered interaction observations

Published 2.0.0 starts sooner and uses less resident memory than immutable 1.6.0 in this same-host warm-cache sample. Five sampled UI endpoints have matching rendered output after restoring the released Capture shortcut guidance; their timing distributions overlap and do not establish a UI-speed improvement. Complete parity, capture and inference performance remain unfinished.

## Published 2.0.0 startup

These observations use the downloaded, checksum-verified published application, SHA-256 `7ae478a038e66f52c807783928b590161d72491bb4f8f258af7052c2d0cea080`, built at `943d8d4bdf019be52a0e3dfb18f50a3bb1b12d2d`. It is byte-identical to the application installed after [PR #70](https://github.com/1vecera/Mluva/pull/70). That installation changes the installer, not the application binary.

| Metric | Released Python 1.6.0 | Published Rust 2.0.0 |
| --- | --- | --- |
| First visible window, median (range) | 456 ms (402–496) | 260 ms (245–298) |
| D-Bus name ownership, median (range) | 217 ms (190–228) | 42 ms (39–50) |
| Process-tree RSS, median | 260.3 MiB | 236.5 MiB |
| Process-tree PSS, median | 108.6 MiB | 89.8 MiB |
| Idle CPU, observed range of one core | 0% | 0–0.33% |

Median window latency is about 43% lower and RSS about 9% lower in these six measured starts per version. D-Bus ownership precedes the visible window and is not recording readiness. PSS depends on shared pages and other host processes. Short, tick-quantized idle samples do not support an idle-CPU improvement claim. Quit observations remain in [release-startup.json](release-startup.json), without a speed claim because subprocess polling bounds their precision.

## Guidance regression and candidate interactions

The independent rendered-window probe found a stable 2.0.0 Capture-settings mismatch: Rust added “on a standard keyboard” to the group description and changed its punctuation. The resulting extra line displaced controls below it. The complete reference and native frames differed in 21,801 pixels within `(262,675)–(837,794)` at 1100×800; repeated native frames retained the same mismatch. The unchanged published app failed the complete-frame assertion. Its failure is retained in `tmp/native-release-performance-calibration-probe2.log` and `session.EW06vs/release-performance/` beneath that evidence directory.

The one-description repair is commit `2568b250b8c1948bbacdeca8f7aaabe7eef4a97b`, based on merged main `87d1b3655b4ba32bc0826c2b2616d6db738aadf0`. The candidate is built by the actual `linux/build-native.sh` production recipe; its app SHA-256 is `ab9eaac73366257df03a1a0bd5cf82f7b8d24e9031511fc1faa7e0150a680085`. The changed source file SHA-256 is `5537d813c919bbf99bb57a0ffda0021fa8b708e266abfc685548af6e423fa935`. This is a prepared candidate, not a newly published or installed release. The earlier application-only build produced a different binary fingerprint; its prototype timings remain private and are not substituted for this production-build comparison.

Each endpoint now reaches the exact complete reference RGB frame. Ordered visible accessible names/roles also match in the excluded comparison warmups: History 45, Capture settings 176, Personalization 124, Meeting 82 and Commands 391 observations. This is sampled output/accessibility evidence, not complete focus, action, theme or platform acceptance.

| Input to complete matching frame, median (range) | Released Python 1.6.0 | Guidance-fix Rust candidate |
| --- | --- | --- |
| Open History | 53 ms (50–55) | 52 ms (50–56) |
| Click Capture in Settings | 485 ms (480–489) | 482 ms (477–485) |
| Open Personalization | 53 ms (50–55) | 57 ms (53–61) |
| Open Meeting | 51 ms (49–55) | 51 ms (47–56) |
| Press Ctrl+P | 529 ms (524–549) | 526 ms (517–552) |

The measurements include dispatch, default transitions and full-image readback. Image readback alone takes about 20–26 ms per poll, with a further 2 ms polling delay. Differences smaller than that observation granularity are not evidence of a faster interaction. The Settings and Commands observations include their actual settled transition. Caret blinking is disabled identically in both private variants; toolkit transitions retain their defaults. Every image comparison covers the complete RGB frame, with no crop, mask or pixel tolerance. [guidance-interactions.json](guidance-interactions.json) retains every warmup, measured row, matched-frame hash, role hash/count, dispatch/readback duration and summary value.

## Method and identity

The unchanged reference is release `v1.6.0`, commit `5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f`; its app source SHA-256 is `1456c6af104ff95433b1d20ead793f628fada4ef9a67d76b5cf1dc4431885d67`. Before either experiment, all 91 packaged reference modules/resources are compared byte-for-byte with that Git archive, and all 520 native payload hashes plus four managed links are checked against the actual bundle receipt. Each variant runs from its real package and uses its own persistent private XDG state/cache with the identical frozen normal-launch configuration.

The host is an Intel Core i7-12700H with 62.5 GiB RAM, Linux `7.1.9-arch1-2`, GTK 4.22.4, Libadwaita 1.9.3, Pango 1.58.2 and reference Python 3.14.7. The compiler finishes before observations start. No host cache is dropped and other host work is not suspended. Both variants run sequentially in one private Xvfb/Openbox session per experiment, scale 1, Cairo/software rendering, private HOME/XDG, session/accessibility buses, network/PID namespaces and devices. No host input, focus, clipboard, provider account, real audio/GPU device or user content is used.

One excluded warmup per version precedes six interleaved fresh processes per version; order alternates Python/Rust and Rust/Python. These are warm-cache application-process starts. The source starts as its Python module in the existing locked reference environment; native starts its packaged executable directly. Launcher-wrapper overhead is outside these timings. Timing begins before process creation; external D-Bus ownership and private X11 visible-window polling supply startup observations. After two seconds of settling, `/proc` supplies application-tree RSS/PSS and CPU ticks over three idle seconds. Every observed tree contains one application process. Each experiment compares all thirteen public action names and exits all fourteen processes normally through public Quit; all application logs are empty.

The interaction experiment fixes each window to 1100×800 before the timed inputs. History, Personalization and Meeting use public application actions. Capture uses the actual tab's AT-SPI action after unmeasured Settings preparation; Commands uses private XTest Ctrl+P input. Unmeasured 0.8-second gaps separate completed endpoints. Each starting frame must differ from its destination. The excluded reference warmup establishes independent full PNG/RGB targets after 1.2 seconds and verifies PNG/raw calibration; measured inputs have no fixed settling delay. Polling stops only at exact equality with complete reference pixels. The Workspace preview retains its actual continuously changing light; the Capture tab supplies the measured Settings endpoint.

An earlier excluded Commands calibration failed PNG/raw agreement at 0.5 seconds. Its reference PNG and failure log remain in `tmp/native-release-performance-render-probe/`; that attempt did not save the mismatched raw frame, so it does not independently establish the cause. The subsequent 1.2-second excluded reference calibration passes for all five endpoints. This changes neither application's animation nor the measured wait. The published Capture mismatch above is a separately preserved, stable application regression repaired in production source.

The full production-build probe additionally exposed a Commands raster variant. Twenty unchanged-reference fresh starts match the primary frame; a later source-only timed trace independently reproduces the same alternate frame seen in native, before the focus fade. Their complete RGB hashes are `f3df0a913b9a4a2d21ddeaa933fb50eb375082951b735240360557d33ab8e303` and `cbcf84864af57728d1ef403b1354e6ae6731df5b9c72f491d697aa2415978013`. They differ at thirteen background-heading pixels beneath/around the dialog. Source snapshots at 0.764, 1.166 and 2.032 seconds are identical to native's alternate frame, byte for byte. The later focus-outline disappearance also matches; [GTK 4.22.4's window implementation](https://github.com/GNOME/gtk/blob/4.22.4/gtk/gtkwindow.c#L195) sets its ordinary three-second focus timeout. No application focus override is added.

The observer admits those two independently captured, complete early reference frames for Commands. Both source files and their input identity are verified before measurement; `independent_command_reference_frames` records their provenance. Every measured Commands row in the final run matches the primary frame. Failed native/source probes and complete traces remain in `tmp/native-guidance-canonical-benchmark/`, `tmp/native-canonical-command-diagnostic/`, `tmp/native-guidance-settled-canonical-benchmark/` and `tmp/source-command-timed-trace/`; the independent twenty-start control remains in `tmp/command-reference-render-variance/`. This is a sampled reference variation, not evidence of every rendering or accessibility state.

The external observer imports Gio/AT-SPI/GdkPixbuf, not either app's modules or private handlers. It remains ignored scratch at `tmp/benchmark-release-2.0.py`; no Python helper is maintained or shipped. Each JSON file records its actual observer fingerprint: startup used `2178be97e20fb55c226796b9d7946c63e5c99fe26f881284e3544cf89f80d79e`, while final interactions use `ce294eee5d905016be41ae93ce7b8f8588e608a10b9f1ef880baa8c0632294df`. Raw startup evidence is `tmp/published-2.0-startup-benchmark/session.otEDpE/` and its adjacent log. Raw final interaction evidence is `tmp/native-guidance-final-canonical-benchmark/session.2VtIpD/` and its adjacent log. The two committed JSON files preserve every original value; the candidate file adds its actual source patch commit/file identity.

The repaired Capture preference owner also passes its existing seventeen transactions/132 GTK-store states in `tmp/native-capture-guidance-preferences.log`. Actual `make linux-test` passes 140 tests, zero failures and 68 ignored environment checks across 98 suites; both strict Clippy configurations, formatting and generated consistency pass in `tmp/native-capture-guidance-full-gate.log`. No test fixture is regenerated from the candidate, and no new test-only production API is added.

These observations exclude real microphone/portal/Wayland costs, loaded History, diagram-heavy documents, provider/model recognition throughput, paced Live processing and physical F9/F10. The earlier [optimized-candidate startup record](../rust-startup/README.md) remains historical. The [whole-workflow matrix](../../rust-port-parity.md) and [authorized cutover](../rust-cutover/README.md) retain the remaining acceptance obligations.
