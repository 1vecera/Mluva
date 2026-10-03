# Native runtime bundle and editor launcher

`mluva-package SOURCE_DIRECTORY BINARY_DIRECTORY OUTPUT_DIRECTORY` assembles the native runtime without installing or starting it. The current bundle contains nine production executables: `mluva`, `mluva-shell`, `mluva-narrate`, `mluva-asr-worker`, `mluva-audio-cleanup`, `mluva-install-widget`, `mluva-screenshot-editor`, `mluva-uninstall` and `mluva-install`. The builder and comparison peers are excluded. GTK resources, fonts, icons, Mermaid resources/licenses, GNOME extension, service/desktop templates and the two non-Python desktop helpers are selected explicitly. The widget command stages the included Quickshell bundle. Historical root helpers and `uninstall.sh` are relative aliases into `bin/`; root `install.sh` and `linux/install.sh`/`linux/uninstall.sh` are real Bash entry points with separate [combined setup evidence](setup-evidence.md). The expanded bundle's [installation](install-evidence.md) and [removal](uninstall-evidence.md) evidence is separate from the original seven-binary package/editor run below.

The builder accepts executable ELF inputs for its own architecture, requires matching builder/manifest versions, rejects asset links and unsupported file types, and records every payload file's SHA-256 plus exact managed links in `.mluva-native.json`. It creates private staging, normalizes modes independently of umask, and publishes with `RENAME_NOREPLACE`. Existing or concurrently created destinations survive refusal. The bundle also contains the unchanged [narrated-editor build/activation scripts and pinned source metadata](narrated-editor-evidence.md); actual prebuilt activation and upgrade are checked from the bundle and installed app. The native installer has separate disposable-prefix [installation](install-evidence.md) and [legacy migration](legacy-evidence.md) evidence. Final dependency/license inventory, portable archive selection and maintained entry-point conversion remain required.

After building the production executables, assemble with:

```sh
cargo build --locked --workspace --bins --target-dir tmp/native-build
tmp/native-build/debug/mluva-package . tmp/native-build/debug tmp/native-bundle
```

The native screenshot editor preserves the released one-image interface, managed Tensaku selection, stock `/usr/bin/tensaku-edit` fallback, argument bytes, process identity, signals, streams and child status. Its narration command resolves beside the actual executable, so a relocated native bundle uses its own Rust helper. It preserves 127 for an absent editor and 126 for a present but unexecutable editor, including a script whose interpreter is missing. Failed exec diagnostics are bounded native text. The existing input helper uses `cmp` for byte-exact unit ownership instead of invoking Python; low-level comparison errors retain the public ownership-refusal message.

## Independent reference and process checks

The immutable reference is v1.6.0, commit `5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f`. The released editor-template SHA-256 is `10c64cdb8c3861c59ebeabdced845cf09892f03999b2c02fa9e482b13d39b672`; the released input-helper SHA-256 is `5ebaa94ac878b29089f597bc86caaea6374d6ae0c33f93e30ab5ed89ad3c1839`. Both match the immutable Git blobs. Temporary reference observers run the actual released commands and remain in ignored `tmp/`; no Python observer is added to the maintained Rust tree.

[The editor fixture](released-editor-launcher.json), SHA-256 `e9b97f0faa582cdcd1c59c1fa577b1f232dc471e866dc38390c287e09343396c`, records 23 released processes. A separate native editor peer observes exact argument bytes, inherited PID, narration availability, stdout/stderr, chosen exit status and SIGTERM. Cases include argument counts, empty/relative/Unicode/newline filenames, literal `--help`, managed links/effective execute permissions, missing/dangling/invalid editors, stock fallback and absent narration. A native-only invalid-UTF-8 filename case preserves its bytes. Normalization removes disposable paths and maps the old global narration command and the native sibling to the same logical command. Failed-exec diagnostics are compared as a category with exact statuses; the native bounded message is checked literally. This process comparison does not substitute for the separately recorded real Tensaku PNG/OCR and interaction evidence.

[The input-ownership fixture](released-input-ownership.json), SHA-256 `9e2cf5be967a1fb53ec7b9e3c0180ff166d1c85bc708f65e4d84b6ca6e35111b`, records eight released removal outcomes: identical, changed, extra-newline and NUL-containing service files, valid/dangling links, a directory and an absent unit. The native comparison calls the public symlink with the exact packaged helper. A nested mount replaces only the unit directory, checks its device/inode before execution and masks system service sockets. Separate command peers record service requests; actual removal is restricted to the one private unit path. An absolute-path Python trap has a positive control in every case. The released helper fails this test by invoking Python; the packaged helper passes without invoking it. The reference's directory traceback is normalized to its final public refusal; native comparison errors are suppressed. This proves ownership/removal behavior, not privileged service startup or keyboard injection.

The package test runs the actual builder against the actual production ELF files under umask 077. It independently walks the result, verifies all hashes/modes/links and the production binary set, preserves resource bytes, checks the existing frozen CLI help, and rejects unexpected Python assets, links, version mismatch, recursive output and destination collisions. Home remains empty. The existing resident-process comparison also runs against a relocated copy of the complete assembled bundle: 38 public states, 13 actions, five CLI cases, second-instance forwarding and the first cold synthetic-PCM Record/quit cleanup match. Delayed-startup Cancel/repeated Record and missing-resource faults pass. Its external provider/audio/desktop peers retain their documented scope; physical devices are inaccessible.

## Verification and limits

Run the explicit tests only through the guarded runner with its documented prerequisites:

```sh
bash dev/run-isolated-browser.sh tmp/native-package-check -- bash -c '
  set -e
  cargo test --locked -p mluva-install --test editor -- --ignored --nocapture
  cargo test --locked -p mluva-install --test package -- --ignored --nocapture
  MLUVA_TEST_INPUT_HELPER="$OFFSCREEN_SESSION_ROOT/native-package/app/configure-input-helper.sh" \
    cargo test --locked -p mluva-install --test input -- --ignored --nocapture
  MLUVA_TEST_NATIVE_BUNDLE="$OFFSCREEN_SESSION_ROOT/native-package/app" \
    cargo test --locked -p mluva-gtk --test bootstrap -- --ignored --nocapture
'
```

Accepted editor/package/resident evidence is in `tmp/package-editor-evidence/` and `tmp/package-editor.log`. The final package/input-helper evidence is in `tmp/package-input-final-evidence/` and `tmp/package-input-final.log`; packaged helper bytes match the source. The final ordinary workspace passes 137 tests with zero failures and 46 environment-dependent tests ignored across 88 suites in `tmp/package-workspace.log`; the new ignored tests were explicitly executed above. Strict all-target Clippy passes in `tmp/package-clippy.log`. Formatting, diff and ShellCheck pass, as do the two existing input-helper tests in `tmp/package-legacy-helper.log`. No accessibility critical/warning or product startup error appears in the accepted isolated logs.

Recorded failures remain useful evidence: the editor initially returned 127 instead of 126 for a missing interpreter (`tmp/editor-launcher-before.log`); package assembly initially admitted VCS metadata into asset validation and used 0600 for generated metadata under umask 077 (`tmp/package-first.log`, `tmp/package-modes.log`); the first help assertion guessed a heading and was replaced by the unchanged independent release output (`tmp/package-acceptance.log`). An invalid reference sandbox made `/dev/null` unwritable and was discarded; the corrected collector produced `tmp/input-ownership-reference-valid.log`. The old helper then fails the Python trap in `tmp/input-ownership-before.log`, and the native directory case exposed an unwanted `cmp` diagnostic in `tmp/input-ownership-native.log`; both have the recorded passing final comparison. Frozen valid reference expectations were not changed for these repairs.

This evidence does not close any whole-workflow acceptance row. Final distribution entry points, joined screenshot/editor/application workflows, actual widget hot reload, physical keys/microphone, complete platform/target/scale acceptance, performance and maintained/shipped Python removal remain pending. The installed app and widget remain 1.6.0; no live desktop, service, user setting or credential was changed.
