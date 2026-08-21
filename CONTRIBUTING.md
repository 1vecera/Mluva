# Contributing to Mluva

Mluva accepts focused changes that preserve its privacy, exact-target delivery, immutable raw-recognition, and recoverable-failure contracts. Fedora 44 with GNOME Shell 50 on Wayland is the initial supported release target; macOS remains a source preview until its distribution path is independently verified.

Contributions submitted to this repository are accepted under the repository's [Apache License 2.0](LICENSE).

## Local setup

Follow the [Linux guide](linux/README.md) for Fedora dependencies and the credential boundary. Tests use local protocol servers, fake audio and desktop boundaries, and generated content; they must not open a microphone, inspect the live accessibility tree, alter the clipboard, inject input, or open a shortcut approval dialog.

## Verification

Run the complete Linux gate from the repository root:

```bash
make linux-test
make linux-overlay-test
make linux-text-target-test
```

On macOS, run the deterministic suite before changing the preview implementation:

```bash
swift test --disable-dependency-cache
```

Changes to visible Linux UI or the GNOME extension also require isolated virtual-display screenshots and pixel inspection. Changes to focus capture or delivery require the private cross-process native text-target smoke test. Never use a contributor's active desktop as an automated test surface.

## Pull requests

Keep each pull request narrow, explain the user-visible outcome and failure behavior, include focused tests, and update the product or platform contract when behavior changes. Never include credentials, real transcripts, real recordings, private application or window names, or screenshots containing user data. Public claims about compatibility, privacy, speed, or accuracy require reproducible evidence.
