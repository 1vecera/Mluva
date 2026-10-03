# Native combined setup

The prepared runtime now supports the documented `bash install.sh [--yes|-y] [--app-only]` entry point. Its Bash coordinator preserves platform selection, widget ownership preflight before package changes, the installation plan and terminal confirmation, app installation, widget enablement and partial-failure recovery. Native Rust commands own app/widget changes. `bash linux/install.sh` installs only the app, including disposable prefixes; `bash linux/uninstall.sh` removes it. These scripts are inventory-checked package files, not aliases to ELF executables.

This records the initial prepared-bundle setup checkpoint. The subsequent [combined source setup checkpoint](source-setup-evidence.md) switches the maintained root installer and source widget command to Rust, shares one root Bash script with prepared bundles, and repeats this evidence. The candidate still contains nine production binaries; remaining Python implementation/tooling conversion and complete acceptance are pending. No native replacement was installed on Daniel's desktop.

## Independent reference

The unchanged reference is v1.6.0, commit `5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f`. Released root `install.sh` SHA-256 is `4053fc7d3f192319978992e05d947b6c4b0ca9cbe7d9abcbde596e54292c2b58`, verified against its Git blob. [The frozen setup fixture](released-setup.json), SHA-256 `2c2001670581fb608acd3fccbe3415b3fb6e86c9b60f731ad3c70aff59e9cdd4`, records thirty actual released processes. The temporary Python observer remains in ignored `tmp/export-setup.py`; native comparisons need no interpreter.

The cases cover ordered option handling, help/errors, Omarchy/Fedora selection, app-only setup, root/staged/incomplete-source guards, unavailable plugin managers, noninteractive refusal, five real private-terminal answers and failures at every external installation phase. Separate peers record package/app/widget requests and supply controlled failures. Exact statuses, stdout/stderr and request ordering are frozen; private paths and terminal CR characters are normalized. The coordinator must leave the otherwise empty private home unchanged when those external endpoints are peers.

Native package requests preserve the released runtime list except Python, `uv` and GI bindings/tooling. OpenSSL and SQLite are added because the actual app ELF links their libraries. Names were checked in the primary catalogs: [Arch OpenSSL](https://archlinux.org/packages/core/x86_64/openssl/), [Arch SQLite](https://archlinux.org/packages/core/x86_64/sqlite/), [Fedora openssl-libs](https://packages.fedoraproject.org/pkgs/openssl/openssl-libs/) and [Fedora sqlite-libs](https://packages.fedoraproject.org/pkgs/sqlite/sqlite-libs/). The comparison derives this explicit reviewed delta from the frozen package request before comparing all other output. It does not install system packages or establish Fedora binary compatibility.

## Actual package and regression

Five additional workflows use the actual assembled app and widget executables: app-only, combined, widget failure, foreign-widget refusal and package failure. Only package management and shell registration are controlled endpoints; the installed Omarchy manifest validator runs on the private widget. The combined case checks enabled widget content through its manifest entry point, matching app/widget versions and the installed public launcher's released help. App-only removal runs the installed Bash uninstaller. A widget failure rolls back the widget while leaving a usable app; conflicting widget ownership refuses before package/app changes. An unrelated user file survives every case. Absolute-path interpreter traps have positive controls and must remain unused during all native operations.

The previous package's `install.sh` ELF alias fails the documented Bash invocation with status 126 instead of the reference's help status 0 (`tmp/setup-bash-regression.log`). Replacing it with the coordinator fixes that real entry-point regression. The valid reference capture is `tmp/setup-reference-valid.log`; an earlier capture omitted `cat` from the private PATH and was discarded. Two actual-package harness failures were also corrected: a missing private `omarchy-shell` endpoint, and an observer that assumed an unversioned widget filename instead of following its manifest. Neither changed the frozen reference or production widget ownership rules.

Run the checks with a built native bundle and the guarded runner:

```sh
bash dev/run-isolated-browser.sh tmp/native-setup -- bash -c '
  set -e
  cargo test --locked -p mluva-install --test package -- --ignored --nocapture
  export MLUVA_TEST_NATIVE_BUNDLE="$OFFSCREEN_SESSION_ROOT/native-package/app"
  cargo test --locked -p mluva-install --test setup -- --ignored --nocapture --test-threads=1
'
```

Accepted package evidence is `tmp/setup-package-final-evidence/session.fzzNng/native-package/`; the thirty setup comparisons and five actual workflows pass in `tmp/setup-accepted-evidence/session.3TpCL9/` with `tmp/setup-accepted.log`. All eight installation/migration tests pass through the new app-only wrapper, including interruption/ownership faults and self-upgrade. The installed combined payload passes the resident-startup comparison, and all three removal tests pass. `tmp/setup-workspace.log` records 137 ordinary tests passed, zero failed and 59 ignored environment checks across 93 suites. Strict all-target Clippy (`tmp/setup-clippy.log`), formatting, diff and ShellCheck pass.

These are private display/session/accessibility, network/PID, device and data boundaries. Nested mounts mask system unit/service paths; peers never invoke host package management or shell registration. This checkpoint does not prove interactive sudo, a clean distribution's ABI, actual Quickshell hot upgrade, physical microphone/keys, complete application parity or a performance improvement. Subsequent [source entry](source-entry-evidence.md), [combined setup](source-setup-evidence.md) and [archive/license](archive-evidence.md) evidence address their named boundaries; final Python removal and all whole-workflow acceptance rows remain Pending.
