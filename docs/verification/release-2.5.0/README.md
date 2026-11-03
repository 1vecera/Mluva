# Mluva 2.5.0 distribution verification

Verified on Lenovo on 3 November 2026. This release packages the cloud speech providers merged in PR [#126](https://github.com/1vecera/Mluva/pull/126) (`49b0685`) and the launch-film README edit that followed it (`f7516e9`, README only). Release preparation starts at `f7516e9`; its changes are version metadata, the changelog and this evidence, with no dependency or runtime logic changes.

The native application and bundled Omarchy widget both report **2.5.0**. Hosted package metadata follows the release version; this release does not redeploy the already published web app. The provider evidence for the new routes, including one live call per route against public clips, is in [the cloud provider verification](../cloud-providers/README.md).

## Checks and evidence

| Check | Result |
| --- | --- |
| `make linux-test` | Passed: 403 tests passed, 0 failed, 65 explicitly ignored integration tests (sum of the `test result` lines in the log); both strict Clippy configurations, formatting and generated-feature consistency passed. Run with an isolated `XDG_RUNTIME_DIR`, see below. |
| `make linux-shortcut-test` | Passed: 21 portal workflows / 59 observed states, including cleanup. |
| ShellCheck over the repository's prescribed shell paths | Passed (ShellCheck 0.11.0): `install.sh linux/*.sh linux/tests/*.sh dev/*.sh linux/mluva-shell`, plus the smoke and archive-verification scripts in this folder. |
| Hosted syntax and changed package JSON formatting | Passed: `cd hosted && node scripts/check.mjs`; `npx prettier@3.8.2 --check` on the four changed package and lock files. |
| Prepared archive | All 594 inventory files and two links verified, no shipped file missing from the inventory, executable modes and app/bundle/widget versions checked. See the [archive receipt](archive-verification.json) and [verification script](verify-archive.sh). |
| Install from that extracted archive | Passed with an isolated `MLUVA_INSTALL_HOME`; no source checkout required at runtime. |
| Isolated installed-app smoke | Passed for five saved-settings scenarios: launch outside the checkout, private D-Bus ownership, account action, settings left byte-identical and normal Quit, with no critical/panic output. The first scenario also covers the registered `mluva://` handler and a stale callback, as in 2.4.0. |

[Native gate log](native-check.log), [shortcut log](shortcut-check.log), [installed smoke log](installed-smoke.log) and [smoke script](installed-smoke.sh) retain the release-specific evidence. The smoke uses the repository's disposable display/session runner and a device/system-bus sandbox; it does not operate the host desktop or microphone. The logs end with the wrapper's exit status.

The native gate ran under `systemd-run --user --scope -p MemoryMax=12G -p MemorySwapMax=0` with `CARGO_BUILD_JOBS=4` and the short disk-backed `TMPDIR=/home/vecera/tmp/mluva-25-tests`. The gate also ran with `XDG_RUNTIME_DIR` pointing at an empty private directory and `HC_SOCKET` unset. On a PC that runs a Herdr server, five `mluva-ffi --test bridge` tests find its live socket instead of an empty one and fail; they fail the same way on `main` before this release, and all 12 bridge tests pass without a Herdr runtime directory. No application change was needed.

## The new provider routes in the smoke

The smoke's added scenarios save `transcription_provider` as `microsoft` (with `transcription_mode":"batch` and `microsoft_region":"swedencentral`), `xai` and `mistral`, plus an upgraded 2.4.0 file that has none of the new settings, and start the installed app on each with no key set. They prove the packaged binary starts, stays up, leaves the settings file untouched and quits cleanly. They do not prove the file is accepted rather than reported as needing repair, because both start the app and leave the file untouched; that acceptance is covered by `cargo test -p mluva-core --test cloud_providers` (8 tests), which the native gate ran.

## Differences from the 2.4.0 package

- The archive holds 594 inventory files and 2 links, not 601 and 4, and is 14.6 MB rather than 17.9 MB. The Tensaku narration editor left the product before this release (`bin/mluva-narrate`, `bin/mluva-screenshot-editor`, the two narrated-editor scripts and `linux/integrations/tensaku/*`, nine files and two links), and the screen-annotation work added `resources/session-wheel.css` and the Scrawl licence.
- The smoke runs five settings scenarios instead of one.
- The native gate ran with an isolated runtime directory, as above.
- The source archive is made with `git archive --format=tar --prefix=mluva-2.5.0/ v2.5.0 | gzip -n > mluva-2.5.0-source.tar.gz`. The same command with `v2.4.0` reproduces the published 2.4.0 source asset byte for byte (SHA-256 `e08424c6442a76cff91651ae81eea7aa18ee43b940ffccc6ebf1570bec784b47`). The archive records the commit id, so make it from the final release commit or tag.

## Reproduce the package checks

Run from a clean release checkout with the documented dependencies and the pinned toolchain:

```sh
mkdir -p tmp
make linux-test linux-shortcut-test
shellcheck install.sh linux/*.sh linux/tests/*.sh dev/*.sh linux/mluva-shell
bash linux/build-native.sh "$PWD/tmp/native-bundle"
tmp/native-build/release/mluva-package --archive "$PWD" "$PWD/tmp/mluva-2.5.0-omarchy-x86_64.tar.gz"
tar -xzf tmp/mluva-2.5.0-omarchy-x86_64.tar.gz -C tmp
bash docs/verification/release-2.5.0/verify-archive.sh "$PWD/tmp/mluva-2.5.0-omarchy-x86_64.tar.gz" "$PWD/tmp/mluva-2.5.0"
MLUVA_INSTALL_HOME="$PWD/tmp/staged-home" tmp/mluva-2.5.0/bin/mluva-install
bwrap --die-with-parent --bind / / --dev /dev --tmpfs /run/dbus -- \
  bash dev/run-isolated-browser.sh "$PWD/tmp/staged-startup" -- \
  bash docs/verification/release-2.5.0/installed-smoke.sh
```

Set `TMPDIR` to a short disk-backed scratch directory, and on a PC running Herdr set `XDG_RUNTIME_DIR` to an empty private directory and unset `HC_SOCKET` for the native gate. If reusing `CARGO_TARGET_DIR`, invoke `mluva-package` from that target's `release/` directory. The isolated runner needs Xvfb, Openbox, AT-SPI and a theme, as described in [the development guide](../../../dev/README.md). Output directories must be fresh. Release assets carry `SHA256SUMS`; verify the downloaded archive before installation. The archive contents depend only on the packaged sources, so documentation commits after packaging, such as this folder, do not change it. The binaries were built from the same Rust sources the release commit contains (the two trees differ only in this folder), and packaging again from the final release commit reproduced the same SHA-256.

Daniel asked for the merge and the release. Tagging, the GitHub release and any local app or widget upgrade are separate operations that this pre-publication evidence does not claim have happened. Remaining limits: no physical microphone or Hyprland acceptance of the new routes; the macOS app dictates with ElevenLabs only and its Swift settings mirror was not built here; ElevenLabs keeps its released behavior of accepting an empty realtime result, which the new providers treat as a failed stream; and the Azure free tier's one-stream limit and Mistral's HTTP 429 on bursts were checked against local peers only. Hosted CI is manual-only and was not run; no runner billing changes were made.
