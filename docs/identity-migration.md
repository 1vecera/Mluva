# Mluva identity migration

Mluva is the only current product name. VoiceScribe, Voice Scribe, voice-scribe and voice_scribe are retired identifiers, retained only where an explicit migration reads an existing installation or where historical evidence must remain exact.

| Surface | Previous identity | Current identity |
| --- | --- | --- |
| Python imports | `voice_scribe_linux` | `mluva_linux` |
| Python project | `voice-scribe-linux` (older releases) | `mluva-linux` |
| Swift package, executable and test target | `VoiceScribeMac`, `VoiceScribeMacTests` | `MluvaMac`, `MluvaMacTests` |
| macOS bundle identifier | `com.voicescribe.mac` | `com.mluva.mac` |
| macOS preference keys | `voiceScribe.*` | `mluva.*` |
| macOS Application Support | `VoiceScribe` | `Mluva` |
| macOS signing and notarization defaults | `Voice Scribe Local Signing`, `voice-scribe` | `Mluva Local Signing`, `mluva` |
| Linux app, desktop and D-Bus identity | `com.voicescribe.Linux`, `/com/voicescribe/Linux` | `com.mluva.Linux`, `/com/mluva/Linux` |
| Linux configuration, data and runtime roots | `voice-scribe` below each XDG root | `mluva` below each XDG root |
| GNOME display extension | `recording-status@voicescribe.local` | `recording-status@mluva.local` |
| System input service | `voice-scribe-input@.service` | `mluva-input@.service` |
| Scoped credential reference profile | `voice-scribe.env` | `mluva.env` |
| Commands | `voice-scribe`, `voice-scribe-input-helper`, `voice-scribe-overlay` | `mluva`, `mluva-input-helper`, `mluva-overlay` |
| Packaged Omarchy plugin | `voice-scribe/app/quickshell/mluva.dictation` | `mluva/app/quickshell/mluva.dictation` |

The published Omarchy plugin ID remains `mluva.dictation`; it already uses the current product identity. Its packaged bridge and D-Bus endpoints now use Mluva throughout. No retired command aliases or Python import shims are installed.

## Linux upgrades

Close the existing app, then run `bash linux/install.sh` from the new source. The installer invokes [the migration boundary](../linux/migrate_legacy.py) before publishing the current package. That file is not copied into the installed application. Development checkouts also require this explicit upgrade before using the renamed runtime against existing state.

The migration checks directory and launcher ownership before its first write. Two independently populated old and new state roots, unrecognized launchers, symlinked state roots, differing credential references, or locally modified system service templates stop the upgrade without adopting or overwriting them. Reconcile the reported conflict before retrying; no automatic history merge is attempted.

Each upgrade retains a private snapshot and path manifest under `$XDG_DATA_HOME/mluva-migration-backups/upgrade-*` (normally `~/.local/share/mluva-migration-backups`). Configuration and data directories move as a whole: history, conversations and rewrite versions, personalization, saved drafts, meeting records and retained audio remain available. Only managed absolute audio references in history and the saved draft are rebased. Recognized and edited text are never rewritten. The reference profile is moved as a reference, without resolving or printing its credential.

Existing managed launchers are replaced, GNOME favorites, custom shortcuts and overlay preferences are retargeted, known Hyprland executable bindings and owned autostart entries are updated, and a plugin symlink to the old packaged location is retargeted. The obsolete Right Alt helper is retired. An already installed system input helper is renamed only when its template is exact and no other user's instance depends on it; its enabled and active states are retained. This existing privileged integration requires `sudo`, as its installation did. Fresh installs never enable a new privileged helper through migration.

A dependency, package, state, or integration failure restores the backed-up installation and retains the snapshot. A successful rerun sees no legacy installation and creates no additional migration backup. Uninstall removes the current owned application and launch integrations while retaining settings, history, drafts, audio, migration backups, and unrelated files. Staged verification with `MLUVA_INSTALL_HOME` skips live Shell, systemd, accessibility and credential service actions.

The new application and extension identities require fresh desktop approvals. Approve shortcuts when prompted; a renamed Shell extension may require logout/login for discovery. Private portal and X11 checks cannot prove a physical shortcut or real Wayland permission flow.

## macOS upgrades

`make install` validates and replaces recognized application bundles, retaining previous apps under `~/Library/Application Support/Mluva-migration-backups/apps.*`. It refuses to install while either version is running. The primary bundle directory keeps its file identity during replacement and rename so existing aliases can follow it. The application runs [its one-time native migration](../Sources/Services/LegacyMigration.swift) before SwiftUI constructs any stores or controllers. It moves the Application Support directory and copies the old bundle/debug preference domains to the new keys, keeping current preferences when they already exist. A provider service-account path inside the moved directory is rebased; external account files remain at their configured locations.

Private data and preference snapshots remain in `~/Library/Application Support/Mluva-migration-backups/<UUID>`. Conflicting data directories or failed persistence keep the app closed so a fresh empty history cannot conceal an incomplete migration. The native upgrade tests use isolated directories and preference suites.

macOS permission grants are tied to bundle identity and signing: approve microphone, speech recognition and Accessibility again as needed. Developer signing uses the new certificate name and notarization profile; create/configure those through `make setup-signing` and the documented signing workflow. Credential values and private keys are not copied into the repository or application. Previously signed historical artifacts and keychain entries are not renamed or falsely relabeled.

## Archival exceptions and package scan

Historical media and receipts are unchanged under `docs/promotion/assets/`, `docs/promotion/evidence/`, `docs/reviews/s27-459/`, and `docs/verification/delight-launch/`. Their saved harnesses, runtime hashes and paths describe the software that actually produced the captures. These directories are excluded from the installed Linux payload and native app bundle. Git history is unchanged.

Current source may contain retired literals only in the two migration implementations, the macOS installer boundary, this document, migration fixtures, and the identity scanner. The Linux runtime payload contains none. The native executable necessarily retains its exact old storage literals for first-launch migration, alongside current targets and symbols.

Run the reproducible scans from the repository root:

```sh
uv run --no-project scripts/check_product_identity.py
uv run --no-project scripts/check_product_identity.py --package /absolute/staged/home/.local/share/mluva/app
uv run --no-project scripts/check_product_identity.py --package build
```

The report lists archival and migration exceptions separately and fails for unclassified matches. Inspect the actual install tree and signed bundle, including filenames and generated dependency metadata; a source-only scan is insufficient.
