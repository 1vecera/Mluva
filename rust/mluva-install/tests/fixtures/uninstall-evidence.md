# Native application removal

`mluva-uninstall` removes this user's recognized application and owned desktop integrations without Python. It accepts the released application marker and the native package receipt, retains exact managed helper links, handles the native main-command symlink, and preserves modified or foreign commands, desktop files and extensions. Settings, recovery/history bytes, audio, images, models, Tensaku preferences, migration backups and credential references remain outside the removed application directory. Input-service removal failure stops before application deletion; optional overlay failure warns and continues. Desktop-cache failures retain their exit status, including failed execution and signals.

The executable belongs at `<app>/bin/mluva-uninstall`, with `uninstall.sh -> bin/mluva-uninstall` retaining the existing target of the public `mluva-uninstall` link. It resolves resources from its actual executable location and has no source-checkout or interpreter fallback. The tested ELF links libc, libgcc and the native loader only. The current nine-binary package also has a [native installer](install-evidence.md), whose installed payload passes this removal comparison. Legacy migration and complete acceptance remain unfinished; this command has not replaced the installed 1.6.0 uninstaller.

Path handling preserves `MLUVA_INSTALL_HOME`, staged default directories and live custom XDG roots. Additional guards reject a staged prefix that resolves to the simulated live home, a staged root that escapes its prefix, a linked legacy ownership marker, and malformed or foreign native receipts. Running processes are identified through `/proc` executable or invocation identity, including the old venv/module arguments and the native public symlink. No process environment or user content is inspected. Missing access to an executable link must not permit removal of a process launched from the application directory.

## Independent observations

The reference is v1.6.0 at `5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f`. Released `linux/uninstall.sh` SHA-256 is `b5cfb0efe04f35f47420d10f22a7ec77d60fb8c590c169d8096b330b561bbf8b`, checked against the immutable Git blob. [The frozen fixture](released-uninstall.json), SHA-256 `8edc8e5a45450b5bcbc0fe48bae9880536b30b051c6f7ca28a4736e34f3c33c2`, contains 39 actual released command invocations. The first 35 observations retain their original SHA-256 `b7d1af89fd3b364fd5b01695d1bd96b74cadf0fcfb8a6e6ea2989f895456bd27`; four independently observed PATH/execution cases were appended without changing them. The temporary Python collector remains in ignored `tmp/`.

The shared Bash helper seeds only inputs: synthetic application markers, managed/foreign integrations and opaque user-store bytes. Released and native commands perform their own ownership checks and removal. The maintained comparison independently snapshots file hashes, permissions, directories and symbolic-link targets before and after each command. It compares every removed/changed path, stdout/stderr, status and external command receipt. Disposable root prefixes and the synthetic process-peer location are normalized. Native failed-execution and signal diagnostics are checked literally and compared to the corresponding released shell-error category, retaining status 126 or 143. Parser input is never exposed.

The cases cover normal and Unicode paths, absent/non-directory/foreign/symlinked applications, modified or linked launchers/helpers/desktop entries/icons, exact/modified/incomplete/linked extensions, invalid roots, custom XDG directories, staged XDG isolation, input removal and failure, optional overlay discovery/failure, desktop-cache failure/PATH fallback and a real running process. Service and GNOME endpoints are separate receipt-only peers; the actual packaged input/overlay scripts run. One PATH-fallback case executes the installed `update-desktop-database` solely on the private application directory; its SHA-256 is `6f0b785fa5188d6b3f3eccb8f47d3d5d6d1f3588e5b8658c1ef6a3650643a73c` and the resulting private cache bytes are included in the comparison.

The private runner isolates display, session/accessibility buses, network, PID and device access. Nested mounts mask service sockets, bind the disposable unit directory over `/etc/systemd/system`, and verify its device/inode before execution. The privileged-command peer permits actual removal only of the one bind-mounted test unit. An absolute-path Python trap is positively exercised in every native case and must remain unused by removal. These checks prove command/filesystem behavior, not privileged service availability or a live GNOME session.

Six further native guards verify unchanged file state and no external effects for prefix aliasing/escape, linked/invalid/foreign markers and an active native executable. A separate package test copies the actual complete bundle into a disposable user prefix and calls the public `mluva-uninstall` link. It verifies removal of the whole native payload, including the executing uninstaller, the independently specified integration set, and byte/mode preservation of all other seeded state, including a private synthetic credential reference. It does not interpret the opaque history/audio/model sentinels or establish their application-level readability.

## Validation

After building the native workspace binaries, run:

```sh
bash dev/run-isolated-browser.sh tmp/native-removal -- bash -c '
  set -e
  cargo test --locked -p mluva-install --test package -- --ignored --nocapture
  MLUVA_TEST_NATIVE_BUNDLE="$OFFSCREEN_SESSION_ROOT/native-package/app" \
    cargo test --locked -p mluva-install --test uninstall -- --ignored --nocapture
'
```

Accepted evidence is `tmp/uninstall-package-evidence/session.CbZLvl/` with `tmp/uninstall-package.log`: package construction/ownership checks, all 39 reference comparisons, six native guards and public packaged removal pass. The full workspace passes 137 ordinary tests with zero failures and 49 environment-dependent tests ignored across 90 suites in `tmp/uninstall-workspace.log`; the new ignored checks were explicitly executed. Strict all-target Clippy, formatting, diff checks and ShellCheck pass. Accepted private logs contain no accessibility critical/warning or product startup error.

The unchanged released uninstaller fails the additional alias-preservation check in `tmp/uninstall-before.log`, deleting the disposable simulated live installation. That is a deliberate native safety difference. The first Rust guard missed a real native process when its executable link was unreadable across the nested namespace (`tmp/uninstall-native.log`); resolving its absolute invocation repairs that failure. The expanded execution comparison then caught status 1 instead of 126 (`tmp/uninstall-exec-before.log`); native command lookup and status/signal handling now match the recorded outcomes. Raw reference evidence is retained in `tmp/uninstall-reference-evidence/`, `tmp/uninstall-executable-reference/` and `tmp/uninstall-helper-reference-evidence/`. Frozen expectations were not weakened to accommodate these repairs.

All complete-workflow parity rows remain Pending. Legacy VoiceScribe migration, final distribution entry points, full package/platform/device/user-experience acceptance, performance and maintained/shipped Python removal remain required. The installed application digest and widget version remain the verified 1.6.0 values; no live service, desktop or user state was changed.
