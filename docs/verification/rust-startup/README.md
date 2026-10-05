# Native startup and idle measurements

This record concerns the earlier `44e22ca` candidate. The [published 2.0.0 startup and later interaction observations](../rust-release-performance/README.md) identify their own exact binaries; these historical measurements are not relabeled as release results.

On this host, the optimized native candidate reaches its first visible window sooner and uses less resident memory than released 1.6.0. These are bounded startup/idle observations, not a claim that recording, inference or every interaction is faster. Full UX and performance acceptance remain unfinished.

| Metric | Released Python 1.6.0 | Native Rust candidate |
| --- | --- | --- |
| First visible window, median (range) | 440 ms (419–522) | 251 ms (235–255) |
| Application D-Bus name, median (range) | 203 ms (191–246) | 40 ms (38–41) |
| Resident memory (RSS), median | 258.5 MiB | 234.9 MiB |
| Proportional memory (PSS), median | 116.1 MiB | 97.2 MiB |
| Idle CPU, observed range of one core | 1.00–1.33% | 1.00–1.33% |

Median visible-window latency is about 43% lower and RSS about 9% lower in this sample. D-Bus name ownership occurs before the window and is not a recording-readiness measurement. PSS apportions shared pages and depends on other processes. Idle CPU samples are short and quantized by kernel ticks; their overlapping ranges do not support an idle-CPU improvement claim. Quit observations are roughly 33–34 ms and limited by subprocess polling; they are retained in the raw data without a speed claim.

## Method

Reference: unchanged release `v1.6.0`, commit `5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f`. Candidate: `44e22cae90d039339933073776a1c9fbd322ab24`, built with Rust 1.95 and ordinary `cargo build --locked --release` settings. The real nine-binary runtime is assembled by its production package builder. Its app SHA-256 is `472fa8e8503458189b2d62a2cbeed3def052ea0ce0522935ec58c3fe31966aca`; the reference app source SHA-256 is `1456c6af104ff95433b1d20ead793f628fada4ef9a67d76b5cf1dc4431885d67`, independently checked against the immutable Git blob.

The same Intel Core i7-12700H host reports 62.5 GiB RAM, Linux `7.1.9-arch1-2` and reference Python 3.14.7. Both versions run sequentially in one private Xvfb/Openbox session, with private D-Bus/accessibility, software rendering, no real input/audio/GPU devices and no external network. The compiler finished before measurements began. No host cache was dropped and other host work was not suspended.

Each version has its own persistent private XDG state/cache and identical configuration from the frozen normal-launch bootstrap case. It selects a compatible speech route to an inactive private loopback endpoint, disables rewriting/titles/global shortcuts and opens empty synthetic stores. No recognition, rewrite, model download or real provider is requested. The source runs in its existing locked reference environment; the native candidate runs directly from its prepared bundle. The temporary observer imports only its own Gio API, not either application's implementation.

One excluded warmup per version populates application caches, including private Python bytecode. Six measured starts per version follow, alternating Python/Rust and Rust/Python order. These are fresh processes with warm caches, not machine-boot or filesystem-cold starts. Time begins before process creation. A separate D-Bus observer records name ownership, and `xdotool` polls for the first visible window; polling/tool overhead bounds timing precision. All thirteen public action names match between versions, every process exits normally through its public Quit action, and all fourteen application logs are empty.

After two seconds of settling, `/proc` supplies process-tree RSS/PSS and CPU ticks across a three-second idle interval. Each observed tree contains one application process; the observer, X server, accessibility service and window manager are excluded. Reported medians exclude both warmups; all fourteen observations remain in [measurements.json](measurements.json).

The temporary observer is `tmp/benchmark-startup.py`, SHA-256 `48a36c3fb5036f540b0695502cac4b765e778691f8b2c735f49ded4dc945880c`. Raw logs and the optimized bundle remain in `tmp/native-startup-benchmark-evidence/session.5AhHjw/`, with `tmp/native-startup-benchmark.log` and `tmp/native-release-build.log`. No Python benchmark helper is added to the maintained or distributed implementation.

The optimized renderer separately repeats all six frozen diagram states and five PNG hashes. The exact benchmark package passes the resident bootstrap comparison: 38 public states, thirteen action contracts, five CLI cases, forwarding, cold synthetic-PCM Record/quit and three startup faults. Accepted evidence is `tmp/native-release-validation-evidence/session.2a8c7G/` and `tmp/native-release-validation.log`. This validation uses real optimized production binaries with synthetic external provider/audio boundaries; no real microphone is exercised.

## Remaining acceptance

This comparison does not measure physical microphone behavior, local/cloud recognition throughput, paced Live processing, loaded histories, diagram-heavy documents, GUI action-to-paint latency or native Wayland/compositor costs. The shortcut portal is deliberately disabled in both versions, so startup including real portal approval is also outside this evidence. Daniel subsequently authorized the [2.0.0 cutover](../rust-cutover/README.md) with those risks outstanding. Complete parity and the unmeasured workloads remain acceptance obligations.
