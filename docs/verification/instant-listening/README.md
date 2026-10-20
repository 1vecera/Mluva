# Capture, widget and startup latency

Measured on 20 October 2026 against the installed production commit `45e3f624708e2a564f580a78eb03ac77968f91f8` (PR103–105). Candidate production source: `1a1ee178a1dce9fb7c71eb89026630ad865fc9f7`. These are actual optimized executables, with no application timing hooks or provider overrides added to production.

Capture and local preview latency improved. In the burst fixture, first saved PCM fell from 29.5 to 11.7 ms; an already-created widget appeared in 7 ms rather than 19 ms, and its provider-response-to-words interval fell from 260 to 29 ms. First widget creation still costs 100–215 ms in the candidate samples. Startup distributions overlap substantially; these measurements do **not** establish an application-startup speedup. Paced compatible recognition still needs its configured three seconds of audio.

[All samples, summaries and binary hashes](measurements.json) contain timings and synthetic scenario labels only. Raw private-session logs and screenshots are excluded because they contain fixture text and generated process/session identities.

## Boundaries and results

Values below are **median [minimum–maximum], milliseconds**. `n` is per variant. Six independent paired sessions were collected per audio mode, after a separate warmup pair. Each session starts the app with a fresh private profile, records twice, quits normally, restarts with the same profile and records twice again. This gives 24 measured sessions, 96 capture samples and 48 process launches across both variants. Four warmup sessions are retained in the data but excluded from summaries. There were no failed collection runs.

“First capture” is the first capture in either app process (`n=12` per mode); “repeat” is the second capture. “First widget” is its first overlay creation in a private desktop (`n=6`); the other three captures use the already-created overlay (`n=18`). Startup rows use one observation per process and pool both audio modes (`n=12` per profile/variant). The JSON also retains each profile/capture combination separately (`n=6` per mode/variant).

### Burst PCM: local overhead

The independent producer supplies three seconds of PCM without real-time pacing. This exposes local setup, dispatch and rendering overhead; it is not a speech-recognition latency claim.

| Action → observation, unless stated otherwise | n | Baseline | Candidate |
| --- | ---: | ---: | ---: |
| First saved PCM, first capture in each process | 12 | 29.5 [20.9–56.2] | 11.7 [8.3–16.9] |
| First saved PCM, repeat capture | 12 | 27.4 [23.1–97.7] | 10.6 [8.4–15.0] |
| Preparing status, first capture in each process | 12 | 15.7 [12.4–23.0] | 4.2 [2.1–4.2] |
| Widget frame, first creation | 6 | 199.0 [108.0–235.0] | 176.5 [135.0–215.0] |
| Widget frame, already created | 18 | 19.0 [14.0–32.0] | 7.0 [4.0–10.0] |
| First words frame, first widget creation | 6 | 290.0 [193.0–323.0] | 235.0 [183.0–272.0] |
| First words frame, already-created widget | 18 | 304.0 [70.0–377.0] | 39.5 [33.0–50.0] |
| Provider response → words frame, already-created widget | 18 | 260.0 [29.0–274.0] | 29.0 [23.0–35.0] |

### Paced PCM: accumulation plus local processing

The same producer supplies 320 bytes every 10 ms. Both variants retain the three-second compatible-preview chunk. The smaller baseline post-response median here is consistent with that chunk aligning with the old 250 ms polling timers; its observed tail still reaches 272 ms. The burst improvement must not be presented as a fixed saving for every provider response.

| Action → observation, unless stated otherwise | n | Baseline | Candidate |
| --- | ---: | ---: | ---: |
| First saved PCM, first capture in each process | 12 | 118.0 [110.4–127.7] | 100.6 [98.0–105.5] |
| First saved PCM, repeat capture | 12 | 115.2 [110.3–126.1] | 100.5 [98.6–106.3] |
| Widget frame, first creation | 6 | 163.5 [107.0–226.0] | 133.5 [100.0–155.0] |
| Widget frame, already created | 18 | 17.0 [13.0–26.0] | 6.0 [4.0–15.0] |
| First words frame, first widget creation | 6 | 3065.5 [3056.0–3076.0] | 3025.5 [3020.0–3029.0] |
| First words frame, already-created widget | 18 | 3055.5 [3042.0–3319.0] | 3021.5 [3010.0–3032.0] |
| Provider response → words frame, already-created widget | 18 | 26.0 [20.0–272.0] | 20.0 [9.0–25.0] |

“Saved PCM” is the first observed WAV size above its 44-byte header, a conservative upper bound on input arrival. The recorder still saves/submits 100 ms chunks, which dominate this endpoint in paced mode. Candidate “recording” status separately follows receipt of at least one complete PCM sample, including silence: its paced first-capture median was 17.8 ms [10.5–45.9], and repeat median 13.7 ms [10.5–23.6]. The baseline reported recording after process creation, so that old status marker is **not** equivalent evidence of actual microphone readiness.

### Process startup

Both profile variants launch a new production process. “Warm profile” means existing private configuration/database state after normal Quit; it does not mean a resident app. No OS caches were dropped, and these are not machine-boot/filesystem-cold timings.

| Process launch → observation | n | Baseline | Candidate |
| --- | ---: | ---: | ---: |
| Fresh profile: D-Bus owner | 12 | 76.7 [47.6–98.5] | 91.3 [47.2–118.4] |
| Fresh profile: visible window | 12 | 351.3 [216.8–421.9] | 359.1 [216.6–465.4] |
| Fresh profile: usable Dictate control | 12 | 489.2 [273.0–555.2] | 464.2 [271.8–586.3] |
| Warm profile: D-Bus owner | 12 | 65.8 [42.4–110.5] | 81.1 [43.3–112.8] |
| Warm profile: visible window | 12 | 321.8 [198.9–420.8] | 348.2 [201.4–381.4] |
| Warm profile: usable Dictate control | 12 | 447.6 [264.1–581.0] | 450.5 [244.2–517.3] |

The unused completed-onboarding view is now constructed only when opened, but a reliable startup improvement was not demonstrated. Usable-control timing includes external accessibility discovery after window mapping, so it is an upper bound rather than an internal readiness timestamp. Background load and scheduling varied enough that neither the small fresh-profile median reduction nor the warm-profile median increase should be presented as a proven startup change.

## Implementation and review

- Capture readiness previously built a speech client and discarded it, then rebuilt TLS/HTTP setup synchronously for every capture and compatible preview request. A validated transport now belongs to the immutable settings generation; sessions clone that transport with independent cancellation/request/model state. Settings changes replace it. No model prewarming, cross-capture resident model or new service was added.
- Readiness client construction runs off the GTK thread. Capture diagnostics use ordered background writes, drained at normal terminal paths after audio/provider cleanup. A locked-database regression proves readiness and audio erasure do not wait for those writes.
- Batch/local preview notifications wake the GTK owner directly instead of waiting for the two 250 ms UI/publication polls. The owner snapshots only its active recording and rejects stale identities. Wakeups are coalesced; no transcript queue is added. Clock/health and optional account updates keep their periodic cadence.
- Microphone capture remains Preparing until the recorder receives a complete PCM sample; no input within three seconds fails preparation. Cancellation can reap the input while readiness is pending. Phone readiness retains its handshake boundary because the phone sends audio only after desktop acceptance.
- Completed onboarding no longer builds duplicate controls during ordinary startup. First-run setup still builds them when opened and persists completion. No schema or settings migration is involved.

Final review covered cancellation isolation, delayed callbacks, audio erasure, Incognito, settings-generation changes, diagnostics drain ordering, account cadence and first-run navigation. F9/F12 actions, CoS selection, exact-target delivery and uncertain-receipt/no-replay code were preserved. Their assembled private application checks passed with the changed capture path. The current installed main's separate real Herdr/Codex acceptance is not relabeled as candidate verification.

## Verification

Heavy work ran serially with `CARGO_BUILD_JOBS=2`, nice 10 and two-CPU affinity. No compilation overlapped the measurement series.

| Check | Result and scope |
| --- | --- |
| `make linux-test` | Passed workspace tests, both strict Clippy configurations, formatting and feature-map consistency at `6d8f13c`. The subsequent changes were the GTK publication-cadence refinement and focused test fixtures; native core runtime code did not change afterward. |
| Final GTK follow-up | `cargo test --locked -p mluva-gtk --lib`, strict `cargo clippy --locked -p mluva-gtk --all-targets -- -D warnings`, formatting and the application/agent/account private suites passed on the final source. The unchanged Lua-interpreter test remains separately ignored by the ordinary unit invocation. |
| Capture factory/lifecycle | Passed independent real PCM/provider-process checks, including cancellation, Incognito, fallback, immutable personalization and 184 released states. Local model engines were protocol fixtures, not a new real-model benchmark. |
| Application, providers/onboarding, phone | Passed first-run navigation, autosave, stop/cancel/failure recovery, Quit during recording, readiness, phone/PC Stop, ordered PCM and disconnect cleanup. The failure fixture supplies a valid model and asserts both intended HTTP attempts. |
| Agent voice and shortcuts | Passed toggle, recipient replacement, cancellation/editing, uncertain-delivery/no-replay and 21 portal workflows / 59 states. No host keys or shared Herdr session were driven. |
| Widget and narration | Passed 37 widget states, focus/motion/owner-loss cases, 24 narration CLI cases and three ownership faults. |
| Account follow-up | Private native app plus independent browser receivers received provisional words before Stop; final recognition and continuity checks also passed. Dependencies came from the local npm cache with scripts disabled. |
| Frozen release smoke | The exact measured candidate passed production capture, immutable original, editor autosave, retained-data restart and audio retention checks. |
| Shell lint | `shellcheck install.sh linux/*.sh linux/tests/*.sh dev/*.sh linux/mluva-shell` passed. |
| Measurement invariants | Every run completed two normal process exits; every capture cancellation left empty History and erased recording files. A separate pre-series visual calibration confirmed painted widget words at the frame observation. |

The native volatile-storage fixture requires a disk-backed temporary root. Its unchanged assertion initially failed because this host's `/tmp` is tmpfs; the passing gate used a private bind of a checkout scratch directory to `/var/tmp` with `TMPDIR=/var/tmp`. No production behavior or assertion was weakened.

Earlier calibration under concurrent builds, temporary profiling instrumentation and an observer that could count an in-flight preceding Qt frame are excluded. The final observer waits for a subsequent swap after the text change; both variants use the identical observer and audio producer. No profiling instrumentation remains in the application.

## Reproduction and interpretation

Use the [production verification harness](../../../dev/verification.md#measure-production-latency). Preserve the baseline's production `mluva`, `mluva-shell` and `mluva-audio-cleanup` binaries. Build the candidate through `CARGO_BUILD_JOBS=2 bash linux/build-native.sh "$PWD/tmp/new-production-bundle"` and preserve its binaries separately. Build the release `mluva_verify` example and `audio-fixture-peer` once and copy the identical executables into both binary directories. The GTK resources and production widget sources are unchanged between the measured revisions. All input SHA-256 values are in the data file.

After all builds/checks have finished, use new output directories for each invocation:

```sh
MLUVA_VERIFY_PROFILE=release MLUVA_VERIFY_BINARIES="$PWD/tmp/main-baseline/bin" \
  bash dev/verify.sh performance "$PWD/tmp/baseline-burst-1"
MLUVA_VERIFY_PROFILE=release MLUVA_VERIFY_BINARIES="$PWD/tmp/final-candidate/bin" \
  bash dev/verify.sh performance "$PWD/tmp/candidate-burst-1"
# Repeat six pairs, alternating which variant runs first.
# Repeat the same series with --paced; retain one warmup pair per mode.
```

Each invocation is bounded to two launches and four captures, with observation deadlines. Aggregate medians/minima/maxima from `session.*/performance.json`; do not count repeated startup values twice. The committed data includes both warmups and every measured sample. Read `summary` for all endpoint combinations, or recompute directly from `runs`, excluding `warmup: true`.

The machine had an Intel Core i7-12700H, 20 logical CPUs and 64 GB RAM, running Arch Linux 7.1.9-arch1-2 and Rust 1.95.0. Other desktop/background work continued. Per-run aggregate CPU busy fraction was baseline median 28.9% [10.2–43.0%] and candidate 31.9% [9.9–61.5%]; these are loaded-workstation observations, not a quiescent hardware benchmark. Alternating order reduces ordering bias but does not remove this source of uncertainty. Small sample counts do not justify tail-percentile or statistical-significance claims.

Polling adds at least 2 ms plus IPC/subprocess cost. The widget's visibility property and first swapped frame are recorded separately. Words use two Qt swaps after the property update to avoid an in-flight preceding frame; this is a rendering upper bound, not optical first-pixel measurement. Qt timestamps use wall-clock milliseconds; other durations use monotonic time. The extra frame and initial overlay construction remain visible in the results.

These measurements exclude physical microphone startup, key/portal dispatch latency, live Wayland behavior, large real History, actual model loading/inference, TLS/network handshakes and real speech-provider latency. Realtime provider readiness and its timeouts are unchanged. The previously reported hidden/background GTK Quit crash is not fixed by this work, and the unchanged debug realtime cadence deadline is not claimed fixed. Successful measured foreground exits establish only the tested path.

No live desktop input, host microphone/clipboard, real provider request, paid CI, shared Herdr restart, installation, merge or release was performed. Rollback requires the previous application build; there is no data-format migration. Cold widget construction, startup variability and real provider/model latency remain limits rather than being hidden behind a premature listening claim.
