# Product identity migration verification

Verified on claw-mini on 2026-09-10, using the prepared S27-467 worktree at baseline `6022b05` and the separate `mluva-s27-467` Fedora 44 container. The film and cleanup worktrees were not changed. Current frontend code, artwork geometry and recordings were preserved.

| Check | Result |
| --- | --- |
| `make linux-test` | 391 passed; Ruff lint and format passed. One existing PyGObject deprecation warning. |
| `make linux-shortcut-test` | Production client/private portal peer passed request, session, binding and activation checks under the new app identity. |
| `make linux-text-target-test` | Real separate GTK target: focus capture, restoration and exact Unicode insertion passed on private X11/AT-SPI. |
| Shellcheck | All repository Linux and native shell scripts passed. |
| `swift test --disable-dependency-cache` | 295 tests in 39 suites passed, including native state migration and the corrected three-character protected-vocabulary expectation. |
| `CI=true SIGNING_MODE=ci scripts/build.sh` | Native arm64 release bundle built and passed strict codesign verification; executable and bundle identifiers are current. This is ad-hoc validation, not notarized distribution. |
| Native installation | Actual signed bundle replaced a legacy-identity fixture under an isolated Applications directory; old app retired, backup retained, primary bundle inode preserved. An unrelated bundle was refused untouched. |
| Fresh Linux install | Real distro Python environment and production dependencies installed from a clean copied payload. Installed import paths and filenames inspected. No development dependencies or retired identifiers. |
| Existing Linux installation | First installed the actual baseline source, seeded persistent history/draft/audio/settings/provider configuration, then upgraded through the new installer. State reopened successfully; text was preserved verbatim and managed audio paths rebased. |
| Rollback | Forced dependency sync failure after real venv creation. All prior files and symlink targets matched their pre-upgrade fingerprints; see [receipt](rollback.json). Unit tests also cover interruption and failed system-service migration. |
| Installed GUI | Fresh and upgraded launchers ran outside the checkout with inherited Python import paths cleared, on isolated displays, private buses and private state. [Fresh](fresh-entrypoint.json) and [upgrade](upgrade-entrypoint.json) receipts verify installed-module provenance, the new bus owner, absence of the retired owner, second-launch forwarding and clean shutdown. The upgraded bridge opened saved history. |
| Uninstall | Actual installed uninstallers removed owned packages, commands, desktop/autostart entries and the migrated overlay. All five upgraded state files and unrelated commands survived; see [receipt](uninstall.json). |
| Source/package inventory | [Source scan](source-scan.json): no unclassified matches. [Native bundle](macos-package-scan.json): only the exact first-launch migration storage literals. [Extracted GNOME archive](extension-package-scan.json) and both actual Linux payloads: zero retired identifiers. [Artifact hashes](artifact-hashes.json) identify the inspected outputs. |

The [upgraded UI screenshot](upgrade.png) deliberately shows the retired spelling inside user-dictated fixture text. Migration does not rewrite historical transcripts. Archived repository captures and their receipts also remain unchanged; the [migration guide](../../identity-migration.md) defines their exact source boundaries. None are included in the installed application payload.

The optional `make linux-overlay-test` packs the new extension successfully but cannot finish Shell startup in this disposable container: GNOME 50 cannot activate `org.freedesktop.login1` without a running systemd/logind environment. Running the unchanged baseline harness in the same container produces the same failure. No extension-specific regression was observed before that failure, but this is not a completed Shell runtime check.

Private X11 and synthetic portal evidence does not establish physical Wayland shortcuts, live permission consent, or macOS permission continuity. Approvals are expected again under the new identities. GUI checks used an unused synthetic credential and made no recording or provider requests. Native runtime microphone/delivery and notarization were not exercised. Hosted CI is on demand and was not started; all passing checks above ran locally. Detailed logs, the before-package inventory and repeatable staged-install scripts remain under this worktree's `tmp/s27-467/`.
