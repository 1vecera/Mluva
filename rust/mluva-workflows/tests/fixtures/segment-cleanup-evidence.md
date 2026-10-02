# Ordered realtime cleanup

The immutable reference is Mluva 1.6.0 at `5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f`. An ignored collector imports its actual `SegmentCleanupSession`, preparation, integrity validator and `CodexSegmentCleanupAttempt`, verifies the recorded module bytes against that commit, and uses independent native JSONL children. [Released observations](released-segment-cleanup.json) contain 23 sessions and 26 intermediate projections; SHA-256 is `54dff5addd0e802f3e16b38691b70eedfda1d6f86395af72e56e82529cde1355`.

The native owner accepts committed recognition without waiting for rewriting. It freezes preparation, vocabulary, provider/model identity and the cleanup instructions; each attempt owns a separate Codex client. The released defaults remain two concurrent attempts, eight queued segments, an eight-second attempt deadline, a two-second Stop drain and 8,000-character input/output bounds. Candidates are validated before publication, and only a settled prefix becomes Cleaned or Fallback. Later candidates may finish while an earlier segment remains Rewriting; their text cannot be selected early.

| Independent reference boundary | Observed contract |
| --- | --- |
| Out-of-order completion and queued work | Capacity is released immediately, while publication and final text stay in capture order |
| Empty, duplicate, oversized and excess-capacity input | Admission, immutable raw fallback, exact projection/failure and no extra model request |
| Stop and cancellation | Queued work is discarded, Stop has one fixed drain bound, cleaned prefixes survive and unpublished candidates revert to raw |
| Attempt expiry during processing, preparation or factory construction | Capacity is released and late work cannot publish or start a provider child |
| Blank/oversized replies and unsafe numbers, negation, URL or vocabulary | Candidate rejection, unchanged raw text and exact safe/raw fallback outcome |
| Provider/factory/preparation failures and Unicode preparation | Controlled failure classes, source trimming, uppercase expansion and exact prepared prompts |

The native test drives the public scheduler and actual Codex transport in separate processes with private HOME/CODEX_HOME/TMPDIR and no inherited credentials. It compares accepted flags, every projection, complete terminal/selection/outcome fields, repeated Stop, admission after Stop, prepared prompt bytes, factory counts and actual process counts. Real `/proc` and private workspace observations require every child and 0700 workspace to be gone after cleanup acknowledgement. Stop timing is independently bounded; elapsed durations are excluded from cross-language value comparisons.

Workers and timers belong to the runtime, with completion guards created before spawning. Preparation and client construction run away from the async executor. Cancel and expiry invalidate the attempt token before interrupting the client; a late constructed client is closed before transforming text. Worker ownership retains resource cleanup even after the capture or drain caller ends.

The scheduler is also joined to the production capture session's committed WebSocket callback. [Seven actual capture transactions](../../../mluva-gtk/tests/fixtures/released-capture-segments.json), SHA-256 `1610063cc52756dc3660509f142fe4460e67ad5228519e5cf595b4caded6f226`, add 34 GTK states through unchanged released builders/callbacks. The native page/controller/session matches full results, History, warnings/outcomes, exact PCM and model prompts, process counts and cleanup for successful segments, safe fallback, active Stop, Escape, Incognito, realtime failure followed by batch cleanup, and frozen rules edited during capture.

A separate native exit fault observes the real socket closing while two gated cleanup attempts remain active. Closing the controller must reap both attempts and the cached readiness client, remove their workspaces, and preserve a private clipboard canary without a callback or History write. This failed before cancellation was added to the active drain wait and passes after that owner repair. The check observes cleanup after 600 ms; it is a lifecycle bound, not a performance claim.

```sh
cargo build --locked -p mluva-providers --bin codex-fixture-peer
mkdir -p tmp/native-tests
TMPDIR="$PWD/tmp/native-tests" cargo test --locked -p mluva-workflows --test segment_cleanup
```

The [capture evidence](../../../mluva-gtk/tests/fixtures/capture-lifecycle-evidence.md) documents the isolated GTK invocation and all 46 joined transactions/218 states plus three native faults. Source collectors stay outside the maintained tree, and native tests need no interpreter. The inputs and Codex responses are synthetic, not real-account inference. Complete service assembly, Live scheduling/manual-revision/final reconciliation, review/continuation/meeting/archive controllers, physical keys/microphone, distribution, final Python removal and whole-app performance remain pending. Every complete-workflow acceptance row remains Pending and the full Rust goal stays active.
