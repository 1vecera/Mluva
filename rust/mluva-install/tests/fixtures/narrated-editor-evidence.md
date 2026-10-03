# Narrated editor package activation

The native package now includes the existing `linux/install-narrated-editor.sh` and `linux/build-narrated-editor.sh`, plus the pinned Tensaku source patch, upstream commit, MPL license, notice and integration README. These tools already use Bash and Rust. Their contents and public behavior are unchanged; packaging now retains their documented relative paths, executable modes and release inventory. Integration directories have mode 0755 even under umask 077.

The immutable reference is v1.6.0 at `5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f`. The released install script SHA-256 is `8e452a11d2b24bd358eb05b94d8a951fb9b29f6f55c2c7115d76e26b2fffb157`; the build script SHA-256 is `97eb4bfba962002d4c1dab154e3b4bd75142416c6c65a35c3b21a9253b8d747a`. Both match the current scripts. The prebuilt Tensaku 0.29.0 used here has SHA-256 `79de63b635ec5a711bb250bb4fa7cd2dd69aeeb70dffe735a39c3669f7082831`; its four source metadata files match the included integration at upstream commit `05a199f345976c432fa1b4452e57a385e73d3a76`.

## Native checks and independent reference

`prebuilt_activation_preserves_metadata_and_unrelated_editors` runs eight actual installer processes with private prefixes and an observable synthetic editor: fresh/owned installation, mismatched patch, linked license, missing narrator, unrelated default editor, nonexecutable binary and an executable without narration support. It checks status/diagnostic categories, copied bytes/modes, exact default symlink, preservation of user content, admission before invoking the editor, and staged-home precedence over unrelated XDG state. The same assertions run against the extracted immutable release through the test-only `MLUVA_TEST_NARRATED_INSTALLER` selection, then against the maintained script. Neither run needs Python.

`native_package_can_install_and_upgrade_the_narrated_editor` installs the actual native app into a disposable prefix, then activates the real prebuilt Tensaku first from the package and again from the installed app. It checks the editor and source bytes, the default wrapper link, the real editor's narration option and preservation of an unrelated file across the second activation. The packaged source-build entry point must also return its existing usage refusal for a relative output path. An absolute-path Python trap has a positive control and must remain unused by all app/editor installation operations. PipeWire/clipboard names are inert preflight prerequisites; no input device or host desktop is used.

The pre-fix optimized package installs the app successfully but fails at the actual narration Bash command because `linux/install-narrated-editor.sh` is missing (`tmp/narrated-package-before.log`, private session `tmp/narrated-package-before-evidence/session.QsPiZv/`). This is the intended regression. The repaired package passes its actual builder inventory/mode/privacy/atomicity check and the real editor activation/upgrade in `tmp/narrated-package-evidence/session.zn8Y2i/`, recorded in `tmp/narrated-package.log`.

Run after building the nine production executables:

```sh
cargo test --locked -p mluva-install --test narrated_editor
bash dev/run-isolated-browser.sh tmp/narrated-package -- bash -c '
  set -e
  cargo test --locked -p mluva-install --test package -- --ignored --nocapture
  export MLUVA_TEST_NATIVE_BUNDLE="$OFFSCREEN_SESSION_ROOT/native-package/app"
  export MLUVA_TEST_TENSAKU_BUNDLE=/absolute/path/to/verified-prebuilt-editor
  cargo test --locked -p mluva-install --test narrated_editor native_package -- --ignored --nocapture
'
```

## Packaged source build

The packaged build script also runs unchanged from a private copy of the accepted native bundle. It clones a local Git mirror containing the exact pinned upstream commit/tree/blob objects, checks out the pin, checks/applies the packaged patch and completes `cargo build --locked --release` using the existing registry cache with networking disabled. Cargo's target directory is fresh, Rust is 1.95, and compilation uses two jobs. All required native development libraries are present. The absolute-path Python trap passes its positive control and remains unused during the complete build and actual editor `--help`. Metadata bytes match, the narration option is present, and the build's temporary source directory is removed.

The initial local mirror was a partial clone whose unrelated missing objects prevented cloning; `tmp/narrated-source-build.log` records that harness failure. The corrected mirror is a complete shallow snapshot of the original commit, validated by `git fsck`, with pack SHA-256 `5d7cb4b8fe41036f3271ce024098cfc6a37a4491f7b324d5d0095d41c4fdf44c`. No upstream or package source was changed. The successful run is `tmp/narrated-source-build-final.log` and `tmp/narrated-source-build-final-evidence/session.PwENzN/`; its Tensaku 0.29.0 binary has SHA-256 `85654b0ee78f881e33120330de8e1c8c239eeef5262df6d7399a62d0ca6e32bc`. The temporary Bash observer is `tmp/verify-packaged-editor-build.sh`, SHA-256 `0667e0e11113f63dd41260da20b8a91aea258b2cbe3b595649d4002ea914e437`.

The final ordinary workspace passes 138 tests, zero failures and 61 ignored environment checks across 95 suites (`tmp/narrated-workspace.log`). Strict all-target Clippy (`tmp/narrated-clippy.log`), formatting, diff and ShellCheck pass. The final eight-case released comparison is `tmp/narrated-installer-reference-final.log`; the same current-script check also runs in the workspace gate. Relevant ignored package/activation checks were executed explicitly above.

This proves prebuilt activation/upgrade and a packaged source build from the cached immutable Git snapshot. It does not test GitHub availability, a clean distribution's development packages, physical shortcut activation or another complete annotation/OCR workflow. The editor's earlier interaction evidence retains its separate scope. All complete-workflow acceptance rows remain Pending; installed Mluva/widget stay at 1.6.0.

## Retired Python test ownership

`linux/tests/test_narrated_editor_installer.py`, introduced by `8316d36` for the 1.6.0 editor release, contained `test_prebuilt_editor_activates_default_wrapper_without_building`, `test_mismatched_source_patch_does_not_activate_or_install` and `test_unrelated_default_editor_is_preserved`. They detect lost binary/source metadata, wrong default-editor activation, acceptance of another release's patch, and overwriting a user-owned editor. The README and screenshot setup invoke the same production Bash installer. Its primary proof is now the native ordinary installer test above; the distinct package test protects distribution layout and execution with actual app/editor binaries. The three old tests are removed together, preserving their assertions in the native owner and avoiding duplicate interpreter-based coverage. No production helper or compatibility seam is added or deleted. The risk is loss of a refusal/preservation assertion; focused reference/current/native-package checks and the native workspace gate cover that migration.
