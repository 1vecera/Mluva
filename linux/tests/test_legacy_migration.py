"""Upgrade fixtures are the test-only boundary for retired installation names."""

import json
import os
import shutil
import signal
import sqlite3
import subprocess
import time
from contextlib import closing
from pathlib import Path

import pytest
from test_launcher import _write_executable, _write_installer_command_doubles

from migrate_legacy import LegacyUpgrade
from mluva_linux.config import load_config
from mluva_linux.history import HistoryStore
from mluva_linux.scratchpad import ScratchpadDraft, ScratchpadDraftStore

SOURCE = Path(__file__).parents[1]


def legacy_install(tmp_path: Path) -> tuple[Path, dict[str, str]]:
    """Seed real stores and managed integrations, with no live credentials or desktop calls."""
    home = tmp_path / "upgrade-home"
    data = home / ".local/share/voice-scribe"
    config = home / ".config/voice-scribe"
    app = data / "app"
    app.mkdir(parents=True)
    (app / "pyproject.toml").write_text('name = "voice-scribe-linux"\n')
    (app / "voice_scribe_linux").mkdir()
    (app / "voice_scribe_linux/__init__.py").write_text('"""Retired package fixture."""\n')
    config.mkdir(parents=True)
    (config / "config.json").write_text(
        json.dumps({"language_code": "ces", "transcription_provider": "voxtype", "voxtype_model": "base"})
    )
    (config / "personalization.json").write_text('{"custom_styles": [], "dictionary": []}\n')
    audio = data / "recordings/draft.wav"
    audio.parent.mkdir()
    audio.write_bytes(b"recoverable fixture")
    store = HistoryStore(data / "history.sqlite3")
    store.initialize()
    # Use the persisted schema so text, rows, absolute paths and audio can be checked independently.
    with closing(sqlite3.connect(store.path)) as connection, connection:
        connection.execute(
            "INSERT INTO transcription_history "
            "(identifier, created_at, raw_text, delivered_text, mode, language_code, "
            "delivery_outcome, retained_audio_path) "
            "VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            (
                "existing",
                "2026-09-10",
                "VoiceScribe mentioned verbatim",
                "Edited draft",
                "scratchpad",
                "ces",
                "copied",
                str(audio),
            ),
        )
    ScratchpadDraftStore(data / "scratchpad-draft.json").save(
        ScratchpadDraft("draft", "existing", "2026-09-10", "VoiceScribe mentioned verbatim", "Žluťoučký", str(audio))
    )
    bin_dir = home / ".local/bin"
    _write_executable(
        bin_dir / "mluva",
        f'#!/bin/sh\napplication_dir="{app}"\nexec python -m voice_scribe_linux.app "$@"\n',
    )
    (bin_dir / "voice-scribe").symlink_to("mluva")
    for command, script in (
        ("mluva-input-helper", "configure-input-helper.sh"),
        ("mluva-overlay", "configure-recording-overlay.sh"),
    ):
        (bin_dir / command).symlink_to(app / script)
        (bin_dir / command.replace("mluva", "voice-scribe")).symlink_to(command)
    _write_executable(bin_dir / "unrelated", "#!/bin/sh\nexit 0\n")
    desktop = home / ".local/share/applications/com.voicescribe.Linux.desktop"
    desktop.parent.mkdir()
    desktop.write_text(f"[Desktop Entry]\nName=Mluva\nExec={bin_dir}/mluva\n")
    extension = home / ".local/share/gnome-shell/extensions/recording-status@voicescribe.local"
    extension.mkdir(parents=True)
    (extension / "metadata.json").write_text(json.dumps({"uuid": extension.name}))
    profile = home / ".config/daniel-ai-skills/env/voice-scribe.env"
    profile.parent.mkdir(parents=True)
    profile.write_text("ELEVENLABS_API_KEY=op://test/elevenlabs/credential\n")
    binding = home / ".config/hypr/bindings.conf"
    binding.parent.mkdir()
    binding.write_text("# voice-scribe is an archival comment\nbind = SUPER, D, exec, voice-scribe\n")
    autostart = home / ".config/autostart/com.voicescribe.Linux.desktop"
    autostart.parent.mkdir()
    autostart.write_text("[Desktop Entry]\nName=Voice Scribe\nExec=voice-scribe\nX-GNOME-Autostart-enabled=false\n")
    plugin = home / ".config/omarchy/plugins/mluva.dictation"
    plugin.parent.mkdir(parents=True)
    plugin.symlink_to(app / "quickshell/mluva.dictation")
    fake_bin = tmp_path / "fake-bin"
    _write_installer_command_doubles(fake_bin, sync_status=0)
    environ = {
        "HOME": str(tmp_path / "live-home"),
        "MLUVA_INSTALL_HOME": str(home),
        "PATH": f"{fake_bin}:{os.environ['PATH']}",
    }
    return home, environ


def test_upgrade_preserves_state_and_retires_active_artifacts(tmp_path: Path) -> None:
    """Reopen upgraded production stores, retain private backups and remove all runtime aliases."""
    home, environ = legacy_install(tmp_path)
    result = subprocess.run(["bash", str(SOURCE / "install.sh")], env=environ, text=True, capture_output=True)
    assert result.returncode == 0, result.stderr
    data = home / ".local/share/mluva"
    config = home / ".config/mluva"
    assert load_config(config / "config.json").transcription_provider == "voxtype"
    assert load_config(config / "config.json").language_code == "ces"
    draft = ScratchpadDraftStore(data / "scratchpad-draft.json").draft
    assert draft is not None and draft.text == "Žluťoučký" and draft.raw_text == "VoiceScribe mentioned verbatim"
    assert draft.audio_path == str(data / "recordings/draft.wav")
    assert Path(draft.audio_path).read_bytes() == b"recoverable fixture"
    with closing(sqlite3.connect(data / "history.sqlite3")) as connection:
        assert connection.execute("SELECT raw_text, retained_audio_path FROM transcription_history").fetchone() == (
            "VoiceScribe mentioned verbatim",
            draft.audio_path,
        )
    assert (home / ".local/bin/unrelated").is_file()
    assert not (home / ".local/bin/voice-scribe").is_symlink()
    assert not (home / ".local/share/voice-scribe").exists()
    assert not (home / ".config/voice-scribe").exists()
    assert not (data / "app/voice_scribe_linux").exists()
    assert (data / "app/mluva_linux/app.py").is_file()
    assert not (data / "app/migrate_legacy.py").exists()
    assert not (home / ".local/share/applications/com.voicescribe.Linux.desktop").exists()
    assert (home / ".local/share/applications/com.mluva.Linux.desktop").exists()
    assert not (home / ".local/share/gnome-shell/extensions/recording-status@voicescribe.local").exists()
    assert (home / ".local/share/gnome-shell/extensions/recording-status@mluva.local/extension.js").exists()
    profile = home / ".config/daniel-ai-skills/env/mluva.env"
    assert profile.is_file() and profile.stat().st_mode & 0o777 == 0o600
    assert "ELEVENLABS_API_KEY=" not in result.stdout + result.stderr
    assert (home / ".config/hypr/bindings.conf").read_text().endswith("exec, mluva\n")
    assert not (home / ".config/autostart/com.voicescribe.Linux.desktop").exists()
    assert (home / ".config/autostart/com.mluva.Linux.desktop").read_text() == (
        "[Desktop Entry]\nName=Mluva\nExec=mluva\nX-GNOME-Autostart-enabled=false\n"
    )
    assert os.readlink(home / ".config/omarchy/plugins/mluva.dictation") == str(data / "app/quickshell/mluva.dictation")
    backups = list((home / ".local/share/mluva-migration-backups").iterdir())
    assert len(backups) == 1 and (backups[0] / "complete").is_file()
    assert backups[0].stat().st_mode & 0o777 == 0o700
    assert (backups[0] / "0/scratchpad-draft.json").is_file()
    assert "voice-scribe" in (backups[0] / "0/scratchpad-draft.json").read_text()
    second = subprocess.run(["bash", str(SOURCE / "install.sh")], env=environ, text=True, capture_output=True)
    assert second.returncode == 0, second.stderr
    assert list((home / ".local/share/mluva-migration-backups").iterdir()) == backups
    uninstall = subprocess.run([str(home / ".local/bin/mluva-uninstall")], env=environ, text=True, capture_output=True)
    assert uninstall.returncode == 0, uninstall.stderr
    assert not (data / "app").exists()
    assert not (home / ".config/autostart/com.mluva.Linux.desktop").exists()
    assert not (home / ".local/share/gnome-shell/extensions/recording-status@mluva.local").exists()
    assert (config / "config.json").is_file()
    assert ScratchpadDraftStore(data / "scratchpad-draft.json").draft == draft
    assert (home / ".local/bin/unrelated").is_file()
    assert backups[0].is_dir()


def test_failed_upgrade_restores_old_state_launchers_and_integrations(tmp_path: Path) -> None:
    """A production dependency failure rolls back state and every owned integration."""
    home, environ = legacy_install(tmp_path)
    old = home / ".local/share/voice-scribe"
    before = (old / "scratchpad-draft.json").read_bytes()
    _write_installer_command_doubles(tmp_path / "fake-bin", sync_status=73)
    result = subprocess.run(["bash", str(SOURCE / "install.sh")], env=environ, text=True, capture_output=True)
    assert result.returncode == 73
    assert (old / "scratchpad-draft.json").read_bytes() == before
    assert (old / "app/voice_scribe_linux/__init__.py").is_file()
    assert (home / ".local/bin/voice-scribe").is_symlink()
    assert "voice_scribe_linux" in (home / ".local/bin/mluva").read_text()
    assert not (home / ".local/share/mluva").exists()
    assert (home / ".config/daniel-ai-skills/env/voice-scribe.env").exists()
    assert (home / ".local/share/gnome-shell/extensions/recording-status@voicescribe.local").exists()


@pytest.mark.parametrize("conflict", ["data", "config", "command", "symlink"])
def test_upgrade_conflicts_fail_before_mutation(tmp_path: Path, conflict: str) -> None:
    """Never merge independent installations or adopt files owned by another program."""
    home, environ = legacy_install(tmp_path)
    if conflict == "data":
        (home / ".local/share/mluva").mkdir()
    elif conflict == "config":
        (home / ".config/mluva").mkdir()
    elif conflict == "command":
        (home / ".local/bin/mluva").write_text("unrelated executable\n")
    else:
        shutil.move(home / ".config/voice-scribe", home / "external-config")
        (home / ".config/voice-scribe").symlink_to(home / "external-config")
    result = subprocess.run(["bash", str(SOURCE / "install.sh")], env=environ, text=True, capture_output=True)
    assert result.returncode != 0
    assert (home / ".local/share/voice-scribe/app/voice_scribe_linux").is_dir()
    assert not (home / ".local/share/mluva-migration-backups").exists()


def test_gnome_preferences_keep_pins_and_drop_retired_altgr_helper(tmp_path: Path, monkeypatch) -> None:
    """Retarget desktop launch pins and the existing display overlay without enabling AltGr capture."""
    upgrade = LegacyUpgrade(SOURCE, tmp_path, tmp_path / "config", tmp_path / "data", staged=True)
    upgrade.gsettings = [
        ("org.gnome.shell", "favorite-apps", "['other.desktop', 'com.voicescribe.Linux.desktop']"),
        (
            "org.gnome.shell",
            "enabled-extensions",
            "['right-alt@voicescribe.local', 'recording-status@voicescribe.local']",
        ),
        ("org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:/test/", "command", "'voice-scribe'"),
    ]
    calls = []
    monkeypatch.setattr(subprocess, "run", lambda command, **kwargs: calls.append(command))
    upgrade.desktop_settings()
    assert calls[0][-1] == "['other.desktop', 'com.mluva.Linux.desktop']"
    assert calls[1][-1] == "['recording-status@mluva.local']"
    assert calls[2][-1] == "'mluva'"
    calls.clear()
    upgrade.desktop_settings(restore=True)
    assert "right-alt@voicescribe.local" in calls[1][-1]


def test_unrecognized_profile_is_never_printed(tmp_path: Path) -> None:
    """Do not expose a value when a user has replaced the reference profile with a secret."""
    home, environ = legacy_install(tmp_path)
    profile = home / ".config/daniel-ai-skills/env/voice-scribe.env"
    profile.write_text("ELEVENLABS_API_KEY=synthetic-do-not-print\n")
    result = subprocess.run(["bash", str(SOURCE / "install.sh")], env=environ, text=True, capture_output=True)
    assert result.returncode != 0
    assert "synthetic-do-not-print" not in result.stdout + result.stderr
    assert profile.read_text() == "ELEVENLABS_API_KEY=synthetic-do-not-print\n"


def test_interrupted_upgrade_restores_the_previous_install(tmp_path: Path) -> None:
    """Terminate a real installer subprocess mid-sync and recover the old runnable layout."""
    home, environ = legacy_install(tmp_path)
    marker = tmp_path / "sync-started"
    uv = tmp_path / "fake-bin/uv"
    text = uv.read_text().replace("  exit 0\nfi\nexit 92", f"  touch '{marker}'\n  sleep 30\n  exit 0\nfi\nexit 92")
    uv.write_text(text)
    process = subprocess.Popen(
        ["bash", str(SOURCE / "install.sh")], env=environ, stdout=subprocess.PIPE, stderr=subprocess.PIPE
    )
    try:
        for _ in range(100):
            if marker.exists():
                break
            assert process.poll() is None
            time.sleep(0.02)
        assert marker.exists()
        process.send_signal(signal.SIGTERM)
        process.communicate(timeout=5)
        assert process.returncode != 0
        assert (home / ".local/share/voice-scribe/app/voice_scribe_linux/__init__.py").exists()
        assert not (home / ".local/share/mluva").exists()
        assert (home / ".local/bin/voice-scribe").is_symlink()
    finally:
        if process.poll() is None:
            process.terminate()
            process.communicate(timeout=5)


def test_failed_system_service_start_restores_template_and_prior_state(tmp_path: Path, monkeypatch) -> None:
    """Prove service migration rollback without systemd, sudo or input access."""
    upgrade = LegacyUpgrade(SOURCE, tmp_path, tmp_path / "config", tmp_path / "data", staged=True)
    upgrade.unit = tmp_path / "voice-scribe-input@.service"
    original = (SOURCE / "resources/mluva-input@.service").read_bytes()
    upgrade.unit.write_bytes(original)
    backup = tmp_path / "backup"
    backup.mkdir()
    state = {"old_enabled": True, "old_active": True, "new_enabled": False, "new_active": False}

    def command(args, **kwargs):
        if args[:2] == ["sudo", "systemctl"]:
            role = "new" if args[-1].startswith("mluva-input@") else "old"
            if args[2] == "disable":
                state[f"{role}_enabled"] = state[f"{role}_active"] = False
            elif args[2] == "enable":
                state[f"{role}_enabled"] = True
            elif args[2] == "start":
                if role == "new":
                    raise subprocess.CalledProcessError(1, "synthetic service start")
                state["old_active"] = True
        elif args[:2] == ["sudo", "mv"]:
            shutil.move(args[2], args[3])
        elif args[:2] == ["sudo", "rm"]:
            Path(args[-1]).unlink(missing_ok=True)
        elif args[:2] == ["sudo", "install"]:
            shutil.copy2(args[-2], args[-1])
        return subprocess.CompletedProcess(args, 0)

    monkeypatch.setattr(subprocess, "run", command)
    with pytest.raises(subprocess.CalledProcessError):
        upgrade._migrate_system_helper(backup)
    assert upgrade.unit.read_bytes() == original
    assert not upgrade.unit.with_name("mluva-input@.service").exists()
    assert state == {"old_enabled": True, "old_active": True, "new_enabled": False, "new_active": False}
