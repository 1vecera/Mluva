"""One-time installer boundary for the retired VoiceScribe identities.

This file is deliberately not installed or imported by the application. Back up
the entire old installation before changing state, and restore it if installation
fails. Never merge two independently populated state roots or inspect key values.
"""

import ast
import json
import os
import re
import shutil
import signal
import sqlite3
import subprocess
import sys
import tempfile
from contextlib import closing
from pathlib import Path

LEGACY_NAME = "voice-scribe"
LEGACY_MODULE = "voice_scribe_linux"
LEGACY_DESKTOP = "com.voicescribe.Linux"
LEGACY_EXTENSIONS = ("recording-status@voicescribe.local", "right-alt@voicescribe.local")
EXTENSION = "recording-status@mluva.local"


class MigrationConflict(Exception):
    """Report an actionable ownership conflict without including file contents."""


def present(path: Path) -> bool:
    """Include dangling symlinks in ownership and rollback checks."""
    return path.exists() or path.is_symlink()


def remove(path: Path) -> None:
    """Remove a previously checked owned path without following links."""
    if path.is_dir() and not path.is_symlink():
        shutil.rmtree(path)
    else:
        path.unlink(missing_ok=True)


def copy(source: Path, target: Path) -> None:
    """Snapshot regular files and trees without dereferencing links."""
    target.parent.mkdir(parents=True, exist_ok=True)
    if source.is_dir() and not source.is_symlink():
        shutil.copytree(source, target, symlinks=True)
    else:
        shutil.copy2(source, target, follow_symlinks=False)


def require_regular(path: Path) -> None:
    """Refuse symlinked files before reading or replacing user configuration."""
    if present(path) and (path.is_symlink() or not path.is_file()):
        raise MigrationConflict(f"Refusing an unexpected file: {path}")


class LegacyUpgrade:
    """Hold the small, explicit set of paths owned by the previous installer."""

    def __init__(self, source: Path, home: Path, config: Path, data: Path, staged: bool) -> None:
        """Resolve every migration path from the installer's validated roots."""
        self.source, self.home, self.config, self.data, self.staged = source, home, config, data, staged
        self.old_data, self.new_data = data / LEGACY_NAME, data / "mluva"
        self.old_config, self.new_config = config / LEGACY_NAME, config / "mluva"
        self.bin = home / ".local/bin"
        self.secret_dir = (
            config / "daniel-ai-skills" if staged else Path(os.environ.get("DAS_CONF_DIR", config / "daniel-ai-skills"))
        ) / "env"
        self.commands = {
            "mluva": None,
            "mluva-input-helper": "configure-input-helper.sh",
            "mluva-overlay": "configure-recording-overlay.sh",
            "mluva-uninstall": "uninstall.sh",
            "mluva-shell": "mluva-shell",
        }
        self.old_commands = [LEGACY_NAME, f"{LEGACY_NAME}-input-helper", f"{LEGACY_NAME}-overlay"]
        self.extensions = data / "gnome-shell/extensions"
        self.desktop_dir = data / "applications"
        self.icons = data / "icons/hicolor/scalable/apps"
        self.binding = config / "hypr/bindings.conf"
        self.autostart = config / "autostart" / f"{LEGACY_DESKTOP}.desktop"
        self.plugin = config / "omarchy/plugins/mluva.dictation"
        self.unit = Path(f"/etc/systemd/system/{LEGACY_NAME}-input@.service")
        self.gsettings: list[tuple[str, str, str]] = []
        self.system_helper = False

    def needed(self) -> bool:
        """Detect a legacy installation without scanning unrelated user data."""
        return any(
            present(path)
            for path in [
                self.old_data,
                self.old_config,
                self.secret_dir / f"{LEGACY_NAME}.env",
                self.desktop_dir / f"{LEGACY_DESKTOP}.desktop",
                self.icons / f"{LEGACY_DESKTOP}.svg",
                self.autostart,
                *(self.bin / name for name in self.old_commands),
                *(self.extensions / name for name in LEGACY_EXTENSIONS),
                *([] if self.staged else [self.unit]),
            ]
        )

    def preflight(self) -> None:
        """Reject conflicts, live processes and foreign owners before the first write."""
        if self.staged:
            if self.home.resolve() == Path(os.environ["HOME"]).resolve():
                raise MigrationConflict("The staged installation resolves to the live home directory.")
            for root in (self.config, self.data, self.bin, self.secret_dir):
                if not root.resolve().is_relative_to(self.home.resolve()):
                    raise MigrationConflict(f"A staged directory resolves outside the installation home: {root}")
        for root in (self.old_data, self.new_data, self.old_config, self.new_config):
            if present(root) and (root.is_symlink() or not root.is_dir()):
                raise MigrationConflict(f"Refusing an unexpected state directory: {root}")
        for old, new in ((self.old_data, self.new_data), (self.old_config, self.new_config)):
            if present(old) and present(new):
                raise MigrationConflict(
                    f"Both legacy and Mluva state exist; reconcile them before upgrading: {old}, {new}"
                )
        for root in (self.old_data, self.new_data):
            app = root / "app"
            if present(app):
                marker = app / "pyproject.toml"
                require_regular(marker)
                if (
                    app.is_symlink()
                    or not marker.is_file()
                    or not any(
                        f'name = "{name}"' in marker.read_text().splitlines()
                        for name in ("mluva-linux", "voice-scribe-linux")
                    )
                ):
                    raise MigrationConflict(f"Refusing an unrecognized application directory: {app}")
                for module in (LEGACY_MODULE, "mluva_linux"):
                    # An escaped, absolute executable match cannot stop or match another checkout.
                    pattern = re.escape(f"{app}/.venv/bin/python -m {module}") + r"($| )"
                    if subprocess.run(["pgrep", "-f", "--", pattern], capture_output=True).returncode == 0:
                        raise MigrationConflict(f"Mluva is running from {app}. Close it before installation.")
        for name in [*self.commands, *self.old_commands]:
            path = self.bin / name
            if not present(path):
                continue
            canonical = name.replace(LEGACY_NAME, "mluva")
            helper = self.commands[canonical]
            if path.is_symlink():
                allowed = {str(root / "app" / helper) for root in (self.old_data, self.new_data)} if helper else set()
                if name in self.old_commands:
                    allowed.add(canonical)
                owned = os.readlink(path) in allowed
            elif helper is None and path.is_file():
                text = path.read_text()
                owned = any(
                    f'application_dir="{root}/app"' in text.splitlines() and f"-m {module}.app" in text
                    for root, module in ((self.old_data, LEGACY_MODULE), (self.new_data, "mluva_linux"))
                )
            else:
                owned = False
            if not owned:
                raise MigrationConflict(f"Refusing to replace an unrelated command: {path}")
        for identity in (LEGACY_DESKTOP, "com.mluva.Linux"):
            desktop = self.desktop_dir / f"{identity}.desktop"
            icon = self.icons / f"{identity}.svg"
            require_regular(desktop)
            require_regular(icon)
            if desktop.exists() and not any(
                f"Exec={self.bin / command}" in desktop.read_text().splitlines() for command in (LEGACY_NAME, "mluva")
            ):
                raise MigrationConflict(f"Refusing an unrelated desktop entry: {desktop}")
            if icon.exists() and not any(
                f'<title id="title">{name}</title>' in icon.read_text() for name in ("Mluva", "Voice Scribe")
            ):
                raise MigrationConflict(f"Refusing an unrelated application icon: {icon}")
        for uuid in LEGACY_EXTENSIONS:
            extension = self.extensions / uuid
            if present(extension):
                metadata = extension / "metadata.json"
                require_regular(metadata)
                if extension.is_symlink() or json.loads(metadata.read_text())["uuid"] != uuid:
                    raise MigrationConflict(f"Refusing an unrelated extension: {extension}")
                if uuid == LEGACY_EXTENSIONS[0] and present(self.extensions / EXTENSION):
                    raise MigrationConflict("Both recording overlays are installed; reconcile them before upgrading.")
        for name in (f"{LEGACY_NAME}.env", "mluva.env"):
            require_regular(self.secret_dir / name)
        old_profile, new_profile = self.secret_dir / f"{LEGACY_NAME}.env", self.secret_dir / "mluva.env"
        if old_profile.exists():
            if not re.fullmatch(r"ELEVENLABS_API_KEY=op://[^/\n]+/[^/\n]+/[^\n]+\n?", old_profile.read_text()):
                raise MigrationConflict(
                    "The legacy credential profile is not a single reviewed reference; it was left untouched."
                )
            if new_profile.exists() and new_profile.read_bytes() != old_profile.read_bytes():
                raise MigrationConflict("The credential reference profiles differ; they were left untouched.")
        require_regular(self.binding)
        require_regular(self.autostart)
        if self.autostart.exists():
            if present(self.autostart.with_name("com.mluva.Linux.desktop")):
                raise MigrationConflict("Both autostart entries exist; reconcile them before upgrading.")
            if not any(
                f"Exec={command}" in self.autostart.read_text().splitlines()
                for command in (LEGACY_NAME, "mluva", str(self.bin / LEGACY_NAME), str(self.bin / "mluva"))
            ):
                raise MigrationConflict("The legacy autostart command was customized; it was left untouched.")
        if self.plugin.is_symlink() and os.readlink(self.plugin) not in {
            str(root / "app/quickshell/mluva.dictation") for root in (self.old_data, self.new_data)
        }:
            raise MigrationConflict(f"Refusing an unrelated plugin symlink: {self.plugin}")
        if not self.staged and self.needed():
            self._preflight_desktop()

    def _preflight_desktop(self) -> None:
        """Snapshot only desktop settings and the optional service this migration changes."""
        if shutil.which("gsettings"):
            for schema, key in (
                ("org.gnome.shell", "favorite-apps"),
                ("org.gnome.shell", "enabled-extensions"),
                ("org.gnome.shell", "disabled-extensions"),
            ):
                result = subprocess.run(["gsettings", "get", schema, key], capture_output=True, text=True)
                if result.returncode == 0:
                    self.gsettings.append((schema, key, result.stdout.strip()))
            bindings = subprocess.run(
                ["gsettings", "get", "org.gnome.settings-daemon.plugins.media-keys", "custom-keybindings"],
                capture_output=True,
                text=True,
            )
            if bindings.returncode == 0:
                for path in ast.literal_eval(bindings.stdout.removeprefix("@as ").strip()):
                    schema = f"org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:{path}"
                    command = subprocess.run(["gsettings", "get", schema, "command"], capture_output=True, text=True)
                    if command.returncode == 0 and LEGACY_NAME in command.stdout:
                        self.gsettings.append((schema, "command", command.stdout.strip()))
        if self.unit.exists():
            require_regular(self.unit)
            if self.unit.read_bytes() != (self.source / "resources/mluva-input@.service").read_bytes():
                raise MigrationConflict("The legacy system input service has local changes; it was left untouched.")
            target = self.unit.with_name("mluva-input@.service")
            if present(target):
                raise MigrationConflict("Both input service templates exist; reconcile them before upgrading.")
            # The system template is shared: never remove an instance belonging to another user.
            result = subprocess.run(
                ["systemctl", "list-units", "--all", "--plain", "--no-legend", f"{LEGACY_NAME}-input@*.service"],
                check=True,
                capture_output=True,
                text=True,
            )
            instances = re.findall(r"voice-scribe-input@\S+\.service", result.stdout)
            instances += [p.name for p in self.unit.parent.glob(f"*.wants/{LEGACY_NAME}-input@*.service")]
            own_unit = f"{LEGACY_NAME}-input@{os.getuid()}.service"
            if set(instances) - {own_unit}:
                raise MigrationConflict(
                    "The legacy input template has other users' instances; migrate those together first."
                )
            subprocess.run(["sudo", "-v"], check=True)
            self.system_helper = True

    def paths(self) -> list[Path]:
        """Enumerate transaction snapshots; the backup itself lives outside these roots."""
        return [
            self.old_data,
            self.new_data,
            self.old_config,
            self.new_config,
            *(self.bin / name for name in [*self.commands, *self.old_commands]),
            *(self.desktop_dir / f"{name}.desktop" for name in (LEGACY_DESKTOP, "com.mluva.Linux")),
            *(self.icons / f"{name}.svg" for name in (LEGACY_DESKTOP, "com.mluva.Linux")),
            *(self.extensions / name for name in (*LEGACY_EXTENSIONS, EXTENSION)),
            self.secret_dir / f"{LEGACY_NAME}.env",
            self.secret_dir / "mluva.env",
            *([self.binding] if self.binding.exists() else []),
            *([self.autostart, self.autostart.with_name("com.mluva.Linux.desktop")] if self.autostart.exists() else []),
            *([self.plugin] if self.plugin.is_symlink() else []),
        ]

    def migrate(self) -> None:
        """Move state and retire old launch artifacts inside the backed-up transaction."""
        for old, new in ((self.old_data, self.new_data), (self.old_config, self.new_config)):
            if old.exists():
                old.rename(new)
        app = self.new_data / "app"
        if app.exists():
            marker = app / "pyproject.toml"
            marker.write_text(marker.read_text().replace('name = "voice-scribe-linux"', 'name = "mluva-linux"'))
        self._rebase_audio()
        for name in self.old_commands:
            remove(self.bin / name)
        for name, helper in self.commands.items():
            path = self.bin / name
            if not present(path):
                continue
            if helper:
                path.unlink()
                path.symlink_to(app / helper)
            else:
                path.write_text((self.source / "resources/mluva.in").read_text().replace("@APPLICATION_DIR@", str(app)))
        for path in (self.desktop_dir / f"{LEGACY_DESKTOP}.desktop", self.icons / f"{LEGACY_DESKTOP}.svg"):
            remove(path)
        old_profile = self.secret_dir / f"{LEGACY_NAME}.env"
        if old_profile.exists():
            old_profile.replace(self.secret_dir / "mluva.env")
            (self.secret_dir / "mluva.env").chmod(0o600)
        for uuid in LEGACY_EXTENSIONS:
            extension = self.extensions / uuid
            if extension.exists():
                remove(extension)
                if uuid == LEGACY_EXTENSIONS[0]:
                    copy(self.source / "gnome-extension" / EXTENSION, self.extensions / EXTENSION)
        if self.binding.exists():
            lines = self.binding.read_text().splitlines(keepends=True)
            self.binding.write_text(
                "".join(
                    re.sub(r"(?<![\w-])voice-scribe(?=-input-helper\b|-overlay\b|\b)", "mluva", line)
                    if re.match(r"\s*bind\w*\s*=.*\bexec\s*,", line)
                    else line
                    for line in lines
                )
            )
        if self.autostart.exists():
            content = self.autostart.read_text().replace(LEGACY_DESKTOP, "com.mluva.Linux")
            content = (
                content.replace(LEGACY_NAME, "mluva").replace("VoiceScribe", "Mluva").replace("Voice Scribe", "Mluva")
            )
            self.autostart.write_text(content)
            self.autostart.rename(self.autostart.with_name("com.mluva.Linux.desktop"))
        if self.plugin.is_symlink():
            self.plugin.unlink()
            self.plugin.symlink_to(app / "quickshell/mluva.dictation")

    def _rebase_audio(self) -> None:
        """Change managed absolute audio paths, never transcript or draft text."""
        old_prefix, new_prefix = f"{self.old_data}/", f"{self.new_data}/"
        database = self.new_data / "history.sqlite3"
        require_regular(database)
        if database.exists():
            with closing(sqlite3.connect(database)) as connection, connection:
                columns = {row[1] for row in connection.execute("PRAGMA table_info(transcription_history)")}
                if "retained_audio_path" in columns:
                    connection.execute(
                        "UPDATE transcription_history SET retained_audio_path = ? || substr(retained_audio_path, ?) "
                        "WHERE substr(retained_audio_path, 1, ?) = ?",
                        (new_prefix, len(old_prefix) + 1, len(old_prefix), old_prefix),
                    )
        draft = self.new_data / "scratchpad-draft.json"
        require_regular(draft)
        if draft.exists():
            content = json.loads(draft.read_text())
            audio = content.get("audio_path")
            if isinstance(audio, str) and audio.startswith(old_prefix):
                content["audio_path"] = new_prefix + audio[len(old_prefix) :]
                draft.write_text(json.dumps(content, ensure_ascii=False, indent=2) + "\n")

    def desktop_settings(self, restore: bool = False) -> None:
        """Retarget existing GNOME pins and overlay preferences without enabling new access."""
        for schema, key, old in self.gsettings:
            value = old
            if not restore:
                value = value.replace(LEGACY_DESKTOP, "com.mluva.Linux").replace(LEGACY_EXTENSIONS[0], EXTENSION)
                if key == "command":
                    value = re.sub(r"(?<![\w-])voice-scribe(?=-input-helper\b|-overlay\b|\b)", "mluva", value)
                # Retire the obsolete AltGr-intercepting extension instead of reviving it.
                value = re.sub(r"'right-alt@voicescribe\.local',?\s*", "", value).replace(", ]", "]")
            subprocess.run(["gsettings", "set", schema, key, value], check=True)

    def install(self) -> int:
        """Run the current installer with no compatibility branches in its runtime payload."""
        child = subprocess.Popen(
            ["bash", str(self.source / "install.sh")],
            env={**os.environ, "MLUVA_MIGRATION_ACTIVE": "1"},
            start_new_session=True,
        )
        try:
            return child.wait()
        except BaseException:
            os.killpg(child.pid, signal.SIGTERM)
            child.wait()
            raise

    def run(self) -> int:
        """Back up once, migrate, and roll back the complete installation on failure."""
        self.preflight()
        if not self.needed():
            return self.install()
        backup_root = self.data / "mluva-migration-backups"
        if backup_root.is_symlink():
            raise MigrationConflict("Refusing a symlinked migration backup directory.")
        backup_root.mkdir(mode=0o700, parents=True, exist_ok=True)
        backup_root.chmod(0o700)
        backup = Path(tempfile.mkdtemp(prefix="upgrade-", dir=backup_root))
        paths = self.paths()
        for index, path in enumerate(paths):
            if present(path):
                copy(path, backup / str(index))
        (backup / "manifest.json").write_text(json.dumps([str(path) for path in paths], indent=2) + "\n")
        (backup / "desktop-settings.json").write_text(json.dumps(self.gsettings, indent=2) + "\n")
        # No secret contents are written to output, including in exception messages.
        print(f"Mluva migration backup: {backup}", flush=True)

        def interrupted(signum, _frame):
            raise InterruptedError(f"Installation interrupted by signal {signum}")

        handlers = {
            number: signal.signal(number, interrupted) for number in (signal.SIGHUP, signal.SIGINT, signal.SIGTERM)
        }
        try:
            self.migrate()
            result = self.install()
            if result:
                raise subprocess.CalledProcessError(result, "Mluva installer")
            self.desktop_settings()
            (backup / "complete").touch(mode=0o600)
            if self.system_helper:
                self._migrate_system_helper(backup)
        except BaseException:
            (backup / "complete").unlink(missing_ok=True)
            for number in handlers:
                signal.signal(number, signal.SIG_IGN)
            for index, path in reversed(list(enumerate(paths))):
                remove(path)
                if present(backup / str(index)):
                    copy(backup / str(index), path)
            self.desktop_settings(restore=True)
            print(f"Mluva upgrade rolled back; backup retained at {backup}.", file=sys.stderr)
            raise
        finally:
            for number, handler in handlers.items():
                signal.signal(number, handler)
        print("Mluva identity migration completed. Desktop permissions may need approval under the new identity.")
        return 0

    def _migrate_system_helper(self, backup: Path) -> None:
        """Rename only this user's exact service, restoring it if systemd setup fails."""
        old = f"{LEGACY_NAME}-input@{os.getuid()}.service"
        new = f"mluva-input@{os.getuid()}.service"
        target = self.unit.with_name("mluva-input@.service")
        enabled = subprocess.run(["systemctl", "is-enabled", "--quiet", old]).returncode == 0
        active = subprocess.run(["systemctl", "is-active", "--quiet", old]).returncode == 0
        copy(self.unit, backup / "input-service")
        try:
            subprocess.run(["sudo", "systemctl", "disable", "--now", old], check=True)
            subprocess.run(["sudo", "mv", str(self.unit), str(target)], check=True)
            subprocess.run(["sudo", "systemctl", "daemon-reload"], check=True)
            if enabled:
                subprocess.run(["sudo", "systemctl", "enable", new], check=True)
            if active:
                subprocess.run(["sudo", "systemctl", "start", new], check=True)
        except BaseException:
            subprocess.run(["sudo", "systemctl", "disable", "--now", new], check=False)
            subprocess.run(["sudo", "rm", "-f", str(target)], check=True)
            subprocess.run(["sudo", "install", "-m", "0644", str(backup / "input-service"), str(self.unit)], check=True)
            subprocess.run(["sudo", "systemctl", "daemon-reload"], check=True)
            if enabled:
                subprocess.run(["sudo", "systemctl", "enable", old], check=True)
            if active:
                subprocess.run(["sudo", "systemctl", "start", old], check=True)
            raise


if __name__ == "__main__":
    try:
        source, home, config, data, staged = sys.argv[1:]
        raise SystemExit(LegacyUpgrade(Path(source), Path(home), Path(config), Path(data), staged == "true").run())
    except subprocess.CalledProcessError as error:
        raise SystemExit(error.returncode) from None
    except MigrationConflict as error:
        print(str(error), file=sys.stderr)
        raise SystemExit(1) from None
    except (OSError, ValueError, sqlite3.Error):
        # A malformed JSON document can contain a credential; do not echo parser input.
        print("Mluva migration failed its state/ownership checks. Existing data was preserved.", file=sys.stderr)
        raise SystemExit(1) from None
