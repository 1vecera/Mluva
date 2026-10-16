# Mluva 2.4.0 distribution verification

Verified on Lenovo on 16 October 2026. This release packages the account implementation and onboarding already merged in PRs [#93](https://github.com/1vecera/Mluva/pull/93), [#94](https://github.com/1vecera/Mluva/pull/94) and the main-toolbar Account sync entry in [#95](https://github.com/1vecera/Mluva/pull/95). Release preparation starts at `fb9cb04b1e8c07761bb7d3687647694f4a1b0f7f`; its changes are version metadata and documentation, with no dependency or runtime logic changes.

The native application and bundled Omarchy widget both report **2.4.0**. Hosted package metadata follows the release version; this release does not redeploy the already published web app or activate its disabled microphone/rewrite providers. The private phone bridge and optional Tensaku installation are preserved; their unchanged standalone artifacts remain available in v2.3.0.

## Checks and evidence

| Check | Result |
| --- | --- |
| `make linux-test` | Passed: 155 tests, 74 explicitly ignored integration tests; both strict Clippy configurations, formatting and generated-feature consistency passed. |
| `make linux-shortcut-test` | Passed: 21 portal workflows / 59 observed states, including cleanup. |
| ShellCheck over the repository's prescribed shell paths | Passed using `uv run --no-project --with shellcheck-py shellcheck …`. |
| Hosted syntax and changed package JSON formatting | Passed: `cd hosted && node scripts/check.mjs`; Prettier on the four changed package/lock files. |
| Prepared archive | All 601 inventory files and four links verified; executable modes and app/widget versions checked. See [archive receipt](archive-verification.json). |
| Install from that extracted archive | Passed with an isolated `MLUVA_INSTALL_HOME`; no source checkout required at runtime. |
| Isolated installed-app smoke | Passed: launch outside the checkout, private D-Bus ownership, account action, registered `mluva://` handler, stale callback handling and normal Quit. No critical/panic output. |

[Native gate log](native-check.log), [shortcut log](shortcut-check.log), [installed smoke log](installed-smoke.log) and [smoke script](installed-smoke.sh) retain the release-specific evidence. The smoke uses the repository's disposable display/session runner and a device/system-bus sandbox; it does not operate the host desktop or microphone.

The first native-gate attempt hit Linux's Unix-socket path limit because its temporary directory was nested under the long worktree path. Rerunning with the short disk-backed `TMPDIR=/home/vecera/tmp/mluva-24-tests` passed. The test environment changed; no application fix was needed. Use a short disk-backed temporary path when reproducing the gate.

The account, GTK workflow, web and responsive screenshot evidence from [the account implementation](../account-sync/README.md) and [the toolbar change](../native-sync-entry/README.md) applies to the unchanged runtime inputs. Those checks include the actual GTK sign-in action with an isolated OAuth peer, native/web text continuity, hosted tests and browser flows. A version bump does not establish additional real-provider or physical-device proof. Hosted CI is manual-only and was not run for this release; no runner billing changes were made.

## Reproduce the package checks

Run from a clean release checkout with the documented dependencies and the pinned toolchain:

```sh
mkdir -p tmp
make linux-test linux-shortcut-test
shellcheck install.sh linux/*.sh linux/tests/*.sh dev/*.sh linux/mluva-shell
bash linux/build-native.sh "$PWD/tmp/native-bundle"
tmp/native-build/release/mluva-package --archive "$PWD" "$PWD/tmp/mluva-2.4.0-omarchy-x86_64.tar.gz"
tar -xzf tmp/mluva-2.4.0-omarchy-x86_64.tar.gz -C tmp
MLUVA_INSTALL_HOME="$PWD/tmp/staged-home" tmp/mluva-2.4.0/bin/mluva-install
bwrap --die-with-parent --bind / / --dev /dev --tmpfs /run/dbus -- \
  bash dev/run-isolated-browser.sh "$PWD/tmp/staged-startup" -- \
  bash docs/verification/release-2.4.0/installed-smoke.sh
```

Set `TMPDIR` to a short disk-backed scratch directory if needed. If reusing `CARGO_TARGET_DIR`, invoke `mluva-package` from that target's `release/` directory. The isolated runner needs Xvfb, Openbox, AT-SPI and a theme, as described in [the development guide](../../../dev/README.md). Output directories must be fresh. Release assets carry `SHA256SUMS`; verify the downloaded archive before installation.

Daniel explicitly authorized merging and distribution. Publication and local app/widget upgrade are separate verified operations recorded in the task handoff; this pre-publication evidence does not claim they have already happened. Remaining limits include physical microphone/Wayland permissions, phone hardware, and Daniel's specific reported sign-in failure, which was not reproduced. The toolbar makes sign-in directly reachable but does not establish that every external account failure is fixed.
