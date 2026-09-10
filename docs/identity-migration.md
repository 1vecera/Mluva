# Mluva identity migration

Mluva is the only current product name. VoiceScribe, Voice Scribe, voice-scribe and voice_scribe are retired identifiers, retained only where an explicit migration reads an existing installation in the migration documentation.

| Surface | Previous identity | Current identity |
| --- | --- | --- |
| Python imports | `voice_scribe_linux` | `mluva_linux` |
| Python project | `voice-scribe-linux` (older releases) | `mluva-linux` |
| Linux app, desktop and D-Bus identity | `com.voicescribe.Linux`, `/com/voicescribe/Linux` | `com.mluva.Linux`, `/com/mluva/Linux` |
| Linux configuration, data and runtime roots | `voice-scribe` below each XDG root | `mluva` below each XDG root |
| GNOME display extension | `recording-status@voicescribe.local` | `recording-status@mluva.local` |
| System input service | `voice-scribe-input@.service` | `mluva-input@.service` |
| Scoped credential reference profile | `voice-scribe.env` | `mluva.env` |
| Commands | `voice-scribe`, `voice-scribe-input-helper`, `voice-scribe-overlay` | `mluva`, `mluva-input-helper`, `mluva-overlay` |
| Packaged Omarchy plugin | `voice-scribe/app/quickshell/mluva.dictation` | `mluva/app/quickshell/mluva.dictation` |

The published Omarchy plugin ID remains `mluva.dictation`; it already uses the current product identity. Its packaged bridge and D-Bus endpoints now use Mluva throughout. No retired command aliases or Python import shims are installed.

## Linux upgrades

Close the existing app, then run `bash install.sh` from the new source. Use `bash linux/install.sh` when dependencies and shell plugins are managed separately. The installer invokes [the migration boundary](../linux/migrate_legacy.py) before publishing the current package. That file is not copied into the installed application. Development checkouts also require this explicit upgrade before using the renamed runtime against existing state.

The migration checks directory and launcher ownership before its first write. Two independently populated old and new state roots, unrecognized launchers, symlinked state roots, differing credential references, or locally modified system service templates stop the upgrade without adopting or overwriting them. Reconcile the reported conflict before retrying; no automatic history merge is attempted.

Each upgrade retains a private snapshot and path manifest under `$XDG_DATA_HOME/mluva-migration-backups/upgrade-*` (normally `~/.local/share/mluva-migration-backups`). Configuration and data directories move as a whole: history, conversations and rewrite versions, personalization, saved drafts, meeting records and retained audio remain available. Only managed absolute audio references in history and the saved draft are rebased. Recognized and edited text are never rewritten. The reference profile is moved as a reference, without resolving or printing its credential.

Existing managed launchers are replaced, GNOME favorites, custom shortcuts and overlay preferences are retargeted, known Hyprland executable bindings and owned autostart entries are updated, and a plugin symlink to the old packaged location is retargeted. The obsolete Right Alt helper is retired. An already installed system input helper is renamed only when its template is exact and no other user's instance depends on it; its enabled and active states are retained. This existing privileged integration requires `sudo`, as its installation did. Fresh installs never enable a new privileged helper through migration.

A dependency, package, state, or integration failure restores the backed-up installation and retains the snapshot. A successful rerun sees no legacy installation and creates no additional migration backup. Uninstall removes the current owned application and launch integrations while retaining settings, history, drafts, audio, migration backups, and unrelated files. Staged verification with `MLUVA_INSTALL_HOME` skips live Shell, systemd, accessibility and credential service actions.

The new application and extension identities require fresh desktop approvals. Approve shortcuts when prompted; a renamed Shell extension may require logout/login for discovery. Private portal and X11 checks cannot prove a physical shortcut or real Wayland permission flow.

## Package identity checks

Retired names remain only in the migration reader, its tests, this guide and the identity scanner. The installed runtime contains none.

```sh
uv run --no-project scripts/check_product_identity.py
uv run --no-project scripts/check_product_identity.py --package /absolute/staged/home/.local/share/mluva/app
```

The scanner lists migration exceptions separately and fails for unclassified matches. Check both the source and the actual installation tree.
