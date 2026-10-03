# Native application installation

`bash <bundle>/linux/install.sh` invokes the ninth production executable, `bin/mluva-install`; root `install.sh` now performs [combined dependency/app/widget setup](setup-evidence.md). The app installer installs a prepared native bundle into the existing user prefix, upgrades recognized Mluva Python or Rust applications, publishes the public launchers and desktop/icon files, and derives only the selected ElevenLabs reference when a scoped profile is needed. User stores, model/editor directories, optional integrations and nonempty profiles are preserved. Staged installs ignore live XDG/credential settings and skip desktop inspection. Running applications are checked before preparation and before publication.

The source and copied bundle must match their versioned SHA-256 inventory, exact relative aliases, file modes and native ELF architecture before any payload executes. Actual main-binary help loads linked dependencies without resolving managed credentials or initializing application stores. Each replacement uses a private same-filesystem backup and guarded rename. Failure restores the app, launchers, desktop files, scoped profile and changed directory modes. HUP/INT/TERM stop the owned child group and roll back before commit. Concurrent replacements are preserved, with previous objects retained in private recovery directories. This establishes process-failure recovery, not power-loss recovery or immunity to adversarial filesystem races.

The installer now incorporates [native VoiceScribe migration](legacy-evidence.md), including profiles under custom `DAS_CONF_DIR`, within the same transaction. Conflicting or foreign state is refused without mutation. Final source/download installation entry points remain unfinished; there is no Python fallback. The maintained root installer still belongs to the working 1.6.0 release. Daniel's installation has not been replaced.

## Independent observations

The immutable reference is v1.6.0 at `5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f`. Released `linux/install.sh` SHA-256 is `5833604b41f56d378bfe380e12df0531efe908636cb2a1ad22b241027a07ddb7`, checked against the Git blob. [The fixture](released-install.json), SHA-256 `b9c6324af3d95b3768f5b489238dc45c19f2847f147760cd4a7a02fb967110db`, records nineteen actual released processes, including its unchanged migration entry point. The temporary observer remains in ignored `tmp/export-install.py`; no maintained Python collector was added.

The shared Bash seeder supplies only inputs. Cases cover fresh/owned/Unicode/custom-XDG/staged installs, profile creation/preservation/empty replacement/CR bytes, GNOME extension/accessibility hints, foreign applications/commands, linked applications, invalid roots and dependency failure. Released provisioning uses an external `uv` endpoint and a native dependency-load peer; native transactions use a small fixture bundle with that peer. Neither peer implements installer ownership, publication or rollback. Actual production payload execution is checked separately below.

Observers compare status, public output, integration requests and selected home/data/config/foreign/unit-tree hashes, modes and link targets. Disposable paths and dependency-peer location are normalized. Implementation-specific application subtrees are represented as prepared applications; the old Bash main launcher and native symlink represent the same public command. Package hashes and execution are verified separately. Ownership diagnostics are compared by category. Dependency failure deliberately improves the released parent-directory mode leak: the native complete before/after tree must be identical.

Nine additional transactions verify complete preservation after cache failure, profile errors, conflicting legacy state and foreign/linked desktop or icon paths. Two more exercise real SIGTERM during a blocked cache command, including its descendant, and a foreign launcher appearing before rollback. Six admission cases reject changed payloads/links, malformed or mismatched inventory, wrong architecture and a running app before dependency execution. The former custom-directory legacy-profile refusal is superseded by the positive migration comparison. Opaque modern-install history/audio/model sentinels prove byte/mode preservation; the migration comparison separately opens real SQLite.

Every native case positively exercises an absolute `/usr/bin/python3` trap, then requires installation not to use it. Display/session/accessibility, network, PID and device boundaries isolate the run. Nested mounts replace system units and hide service sockets; GNOME discovery/settings use controlled read-only endpoints. No privileged service is enabled, device opened, credential resolved or host desktop operated.

## Complete package and regressions

The actual nine-binary package passes construction/inventory/mode/ownership checks and upgrades a disposable owned installation. The real desktop-cache utility runs only against that prefix. Public `mluva --help` matches the released CLI, and the app-only installer successfully reinstalls from its own bundle (now through `linux/install.sh`). The resident comparison then uses that installed payload: 38 public states, 13 actions, five CLI contracts, second-instance forwarding, cold synthetic-PCM recording/quit and three startup faults pass. Public native removal also removes a copy of the complete payload and preserves all other seeded state; its 39 reference cases and six additional guards still pass.

Accepted evidence is `tmp/install-package-evidence/session.fOTFA5/` with `tmp/install-package.log`: package check, all five installer tests, resident comparison and all three uninstaller tests pass. The full workspace records 137 ordinary tests passed, zero failed and 54 environment checks ignored across 92 suites in `tmp/install-workspace.log`; affected ignored checks were explicitly executed. Strict all-target Clippy, formatting, diff checks and ShellCheck pass. Accepted session logs contain no unexpected accessibility warning or critical.

The unchanged release fails complete rollback after cache failure in `tmp/install-before.log`: its app-only rollback leaves other installation paths changed. The first native guard missed a legacy profile under custom managed configuration (`tmp/install-custom-before.log`); the regression now passes without mutation. Early harness runs caught a misnamed interpreter trap and incorrect diagnostic-category mapping; those were observer issues. Frozen reference outcomes remain unchanged.

After building production binaries, repeat through the guarded runner:

```sh
bash dev/run-isolated-browser.sh tmp/native-installation -- bash -c '
  set -e
  cargo test --locked -p mluva-install --test package -- --ignored --nocapture
  MLUVA_TEST_NATIVE_BUNDLE="$OFFSCREEN_SESSION_ROOT/native-package/app" \
    cargo test --locked -p mluva-install --test install -- --ignored --nocapture --test-threads=1
  MLUVA_INSTALLED_BUNDLE=$(cat "$OFFSCREEN_SESSION_ROOT/install-actual/installed-bundle")
  MLUVA_TEST_NATIVE_BUNDLE="$MLUVA_INSTALLED_BUNDLE" \
    cargo test --locked -p mluva-gtk --test bootstrap -- --ignored --nocapture
  MLUVA_TEST_NATIVE_BUNDLE="$MLUVA_INSTALLED_BUNDLE" \
    cargo test --locked -p mluva-install --test uninstall -- --ignored --nocapture --test-threads=1
'
```

All whole-workflow rows remain Pending. Final archive/dependency/license review and maintained entry-point conversion, actual Omarchy hot upgrade, complete platform/target/device/UX acceptance, performance and maintained/shipped Python removal remain required. The installed app digest and widget version remain the verified 1.6.0 values. This work changes the draft task branch and disposable prefixes only.
