# Mluva repository guide

Mluva is a native dictation application with an initially supported Fedora GNOME client and a macOS source preview. The product behavior is specified in `docs/product-contract.md`, while `docs/linux-platform-profile.md` defines the supported GNOME Wayland boundary; keep implementation, documentation, and tests aligned with both.

## Build contract

- Run `swift test` for the complete deterministic suite.
- Run `make linux-test` for the complete Linux suite, Ruff lint, and format verification.
- Let `make linux-setup` create or repair the Linux environment with distribution PyGObject access; do not rely on an ambient `.venv`.
- Run `make linux-text-target-test` for real cross-process AT-SPI focus, restoration, and Unicode insertion on a private virtual display.
- Run Linux GUI acceptance only through the repository's isolated offscreen harness; never drive the active desktop.
- Run `scripts/build.sh` to make `build/Mluva.app`.
- Run `scripts/smoke-test.sh` for tests, a release build, launch verification, and clean shutdown.
- Keep provider logic behind `TranscriptionProvider`; microphone capture and delivery must remain provider-neutral.
- Keep raw recognition immutable and separate from processed and delivered text.
- Never deliver volatile recognition to the target application.
- Preserve recovery audio only according to `AudioRetentionPolicy`; Incognito mode must write no history or retained audio.
- Never persist or log Google access tokens or service-account secrets.

## Architecture

- `RecordingController` owns capture-session state, transcript processing, target restoration, delivery, and recovery metadata.
- `AppleSpeechTranscriptionProvider` uses `SpeechAnalyzer` on macOS 26 and the legacy Speech framework on older supported systems.
- `GoogleCloudTranscriptionProvider` uses Speech-to-Text V2 native gRPC on macOS 15, rotates long streams with bounded overlap, and retains a synchronous compatibility path for macOS 14.
- `TranscriptProcessor` performs deterministic, faithful cleanup before delivery.
- `KeyboardTextDestination` restores the captured target and delegates serialized clipboard insertion to `KeyboardSimulator`.
- `TranscriptionRecoveryService` reprocesses retained PCM without requiring a new recording.

## Platform behavior

- Microphone permission is required to capture.
- Apple Speech permission is required only for Apple recognition.
- Accessibility permission enables the global shortcut and automatic insertion. Clipboard-only delivery remains available without it.
- The audio boundary is signed little-endian PCM at 16 kHz, 16-bit, mono.
