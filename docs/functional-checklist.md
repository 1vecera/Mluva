# Main functionality checklist

Acceptance is a working native Rust app with preserved data, privacy protections and diagnostics. Small UX differences are acceptable. Use synthetic content and the private desktop for automation; do not open the user's microphone, alter the host clipboard or inject host input. Reuse existing passing evidence when runtime inputs are unchanged, and rerun a relevant check after a meaningful change or failure.

| Check | Success means | Existing focused evidence or check |
| --- | --- | --- |
| Record, stop and cancel | Recognition completes, raw text remains available, cancellation prevents delivery, and failures retain recoverable audio. | `make linux-application-test`; [local model evidence](verification/rust-release-performance/README.md). |
| Paste and copy | A global recording inserts once into its captured target; a changed target stays clipboard-only. Manual recording remains copy-only. | `make linux-text-target-test`; [browser delivery evidence](verification/browser-direct-transport/README.md); [Wayland terminal evidence](verification/current-wayland-delivery/README.md). |
| Edit, rewrite and Command | Edits survive; a rewrite produces a reviewable result; Command requires Apply; paused or stale work cannot overwrite newer edits. | `make linux-application-test`; existing Live and review owners. |
| History, settings and diagnostics | Saved recordings reopen, settings survive restart, recovery/export works, and timing/error diagnostics remain available. | `make linux-test-fast`; [persistence and diagnostics](../rust/mluva-core/tests/fixtures/README.md). |
| Screenshots and narration | An explicit capture attaches to the current narration, a fresh narrated text box saves, and an image-capable rewrite receives the saved image. | [Wayland screenshot evidence](verification/current-wayland-screenshots/README.md); `make linux-screenshot-test` when this workflow changes. |
| Privacy and installation | Incognito retains no History/audio; credentials stay private; app and widget upgrade together without losing settings, History or models; installed startup and Quit work. | Native privacy/install tests, [cutover evidence](verification/rust-cutover/README.md), and the exact release's install receipt. |

The managed choices remain Whisper Tiny, Parakeet v3 and default Qwen3-ASR 1.7B. The native release gate is `make linux-test`; additional GUI checks depend on the changed boundary. Existing performance measurements are sufficient unless a change or a new performance claim needs measurement.

Remaining real-device checks are one short F9 recording into the intended editor, F10 plus a narrated screenshot, and Meeting microphone/system-audio capture if used. These require deliberate user interaction; isolated synthetic tests do not establish physical keys, microphone quality or real permission dialogs. Mixed-Unicode selection capture in Chromium Command is a known limitation. Fedora GNOME has no recent desktop acceptance. Automatic paste, Live rewrite, Meeting and screenshot context keep their existing Experimental labels.
