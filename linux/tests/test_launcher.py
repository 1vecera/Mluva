"""Contract tests for the installed Linux entrypoint."""

import os
import subprocess
import tomllib
from pathlib import Path

import pytest


def _write_executable(path: Path, content: str) -> None:
    """Create an owner-executable test double."""
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")
    path.chmod(0o700)


def _render_launcher(tmp_path: Path) -> tuple[Path, Path]:
    """Render the install-time template against an isolated application path."""
    application_dir = tmp_path / "application"
    _write_executable(
        application_dir / ".venv" / "bin" / "python",
        '#!/bin/sh\nprintf \'python:%s:%s\\n\' "$PYTHONPATH" "$*"\n',
    )
    template_path = Path(__file__).parents[1] / "resources" / "mluva.in"
    launcher_path = tmp_path / "mluva"
    launcher_path.write_text(
        template_path.read_text(encoding="utf-8").replace("@APPLICATION_DIR@", str(application_dir)),
        encoding="utf-8",
    )
    launcher_path.chmod(0o700)
    return launcher_path, application_dir


def _launcher_environment(tmp_path: Path) -> tuple[dict[str, str], Path]:
    """Build a deterministic environment for the rendered launcher."""
    home = tmp_path / "home"
    return {
        "HOME": str(home),
        "PATH": os.environ["PATH"],
    }, home


def _write_installer_command_doubles(fake_bin: Path, sync_status: int) -> None:
    """Provide deterministic installer dependencies and a controllable production sync result."""
    for command_name in ("pw-record", "pw-dump", "wl-copy", "update-desktop-database"):
        _write_executable(fake_bin / command_name, "#!/bin/sh\nexit 0\n")
    _write_executable(fake_bin / "pgrep", "#!/bin/sh\nexit 1\n")
    _write_executable(
        fake_bin / "uv",
        f"""#!/bin/sh
if test "$1" = venv; then
  for target in "$@"; do :; done
  mkdir -p "$target/bin"
  printf '#!/bin/sh\\nexit 0\\n' > "$target/bin/python"
  chmod 700 "$target/bin/python"
  exit 0
fi
if test "$1" = sync; then
  exit {sync_status}
fi
exit 92
""",
    )


def test_launcher_uses_scoped_managed_profile_when_credential_is_missing(tmp_path: Path) -> None:
    """Re-exec through the proven managed profile interface without putting a secret in arguments."""
    launcher_path, application_dir = _render_launcher(tmp_path)
    environ, home = _launcher_environment(tmp_path)
    config_dir = home / ".config" / "daniel-ai-skills"
    profile = config_dir / "env" / "voice-scribe.env"
    profile.parent.mkdir(parents=True)
    profile.write_text("ELEVENLABS_API_KEY=op://test/item/credential\n", encoding="utf-8")
    managed_launcher = config_dir / "bin" / "das-mcp-launch"
    _write_executable(
        managed_launcher,
        "#!/bin/sh\n"
        'test "$1" = voice-scribe\n'
        'test "$2" = --\n'
        "printf 'managed-profile:%s\\n' \"$1\"\n"
        "shift 2\n"
        "export ELEVENLABS_API_KEY=test-only\n"
        'exec "$@"\n',
    )

    result = subprocess.run(
        [str(launcher_path), "recording.wav"],
        env=environ,
        capture_output=True,
        text=True,
        check=False,
    )

    assert result.returncode == 0, result.stderr
    assert result.stdout.splitlines() == [
        "managed-profile:voice-scribe",
        f"python:{application_dir}:-m voice_scribe_linux.app recording.wav",
    ]


@pytest.mark.parametrize("profile_marker", ["MLUVA_SECRET_PROFILE", "VOICE_SCRIBE_SECRET_PROFILE"])
def test_launcher_does_not_reenter_an_active_managed_profile(tmp_path: Path, profile_marker: str) -> None:
    """Prevent a missing resolved field from causing an infinite launcher loop."""
    launcher_path, application_dir = _render_launcher(tmp_path)
    environ, home = _launcher_environment(tmp_path)
    environ[profile_marker] = "1"
    _write_executable(
        home / ".config" / "daniel-ai-skills" / "bin" / "das-mcp-launch",
        "#!/bin/sh\nexit 91\n",
    )

    result = subprocess.run(
        [str(launcher_path)],
        env=environ,
        capture_output=True,
        text=True,
        check=False,
    )

    assert result.returncode == 0, result.stderr
    assert result.stdout.strip() == f"python:{application_dir}:-m voice_scribe_linux.app"


def test_installer_exposes_mluva_commands_and_keeps_legacy_aliases() -> None:
    """Make Mluva canonical without breaking shortcuts that invoke the previous command names."""
    installer = (Path(__file__).parents[1] / "install.sh").read_text(encoding="utf-8")

    assert 'resources/mluva.in" > "${bin_dir}/mluva"' in installer
    assert 'ln -sfn "mluva" "${bin_dir}/voice-scribe"' in installer
    assert '"${bin_dir}/mluva-input-helper"' in installer
    assert 'ln -sfn "mluva-input-helper" "${bin_dir}/voice-scribe-input-helper"' in installer
    assert '"${bin_dir}/mluva-overlay"' in installer
    assert 'ln -sfn "mluva-overlay" "${bin_dir}/voice-scribe-overlay"' in installer
    assert '"${bin_dir}/mluva-uninstall"' in installer
    assert "s|@EXECUTABLE@|${bin_dir}/mluva|g" in installer


def test_installed_uninstaller_resolves_its_real_packaged_directory() -> None:
    """Ensure the launcher symlink can still locate packaged integration-removal helpers."""
    uninstaller = (Path(__file__).parents[1] / "uninstall.sh").read_text(encoding="utf-8")

    assert 'script_path="$(readlink -f -- "${BASH_SOURCE[0]}")"' in uninstaller
    assert 'source_dir="$(cd -- "$(dirname -- "${script_path}")" && pwd -P)"' in uninstaller


def test_uninstaller_removes_only_staged_application_files(tmp_path: Path) -> None:
    """Remove a staged install while preserving configuration, history, and unrelated executables."""
    install_home = tmp_path / "staged-home"
    data_home = install_home / ".local" / "share"
    application_dir = data_home / "voice-scribe" / "app"
    bin_dir = install_home / ".local" / "bin"
    desktop_entry = data_home / "applications" / "com.voicescribe.Linux.desktop"
    icon_path = data_home / "icons" / "hicolor" / "scalable" / "apps" / "com.voicescribe.Linux.svg"
    config_marker = install_home / ".config" / "voice-scribe" / "settings.json"
    history_marker = data_home / "voice-scribe" / "history.sqlite3"
    application_dir.mkdir(parents=True)
    bin_dir.mkdir(parents=True)
    desktop_entry.parent.mkdir(parents=True)
    icon_path.parent.mkdir(parents=True)
    config_marker.parent.mkdir(parents=True)
    (application_dir / "pyproject.toml").write_text('name = "mluva-linux"\n', encoding="utf-8")
    (application_dir / "configure-input-helper.sh").write_text("test helper\n", encoding="utf-8")
    (application_dir / "configure-recording-overlay.sh").write_text("test overlay\n", encoding="utf-8")
    (application_dir / "uninstall.sh").write_text("test uninstall\n", encoding="utf-8")
    (bin_dir / "mluva").write_text(
        f'#!/bin/sh\napplication_dir="{application_dir}"\nexec python -m voice_scribe_linux.app\n',
        encoding="utf-8",
    )
    (bin_dir / "unrelated-command").write_text("preserve\n", encoding="utf-8")
    (bin_dir / "voice-scribe").symlink_to("mluva")
    (bin_dir / "mluva-input-helper").symlink_to(application_dir / "configure-input-helper.sh")
    (bin_dir / "voice-scribe-input-helper").symlink_to("mluva-input-helper")
    (bin_dir / "mluva-overlay").symlink_to(application_dir / "configure-recording-overlay.sh")
    (bin_dir / "voice-scribe-overlay").symlink_to("mluva-overlay")
    (bin_dir / "mluva-uninstall").symlink_to(application_dir / "uninstall.sh")
    desktop_entry.write_text(f"[Desktop Entry]\nName=Mluva\nExec={bin_dir}/mluva\n", encoding="utf-8")
    icon_path.write_text('<svg><title id="title">Mluva</title></svg>\n', encoding="utf-8")
    config_marker.write_text("preserve\n", encoding="utf-8")
    history_marker.write_text("preserve\n", encoding="utf-8")

    script = Path(__file__).parents[1] / "uninstall.sh"
    result = subprocess.run(
        [str(script)],
        env={
            "HOME": str(tmp_path / "live-home"),
            "MLUVA_INSTALL_HOME": str(install_home),
            "PATH": os.environ["PATH"],
        },
        capture_output=True,
        text=True,
        check=False,
    )

    assert result.returncode == 0, result.stderr
    assert "Staged verification skipped live GNOME extension" in result.stdout
    assert not application_dir.exists()
    assert not desktop_entry.exists()
    assert not icon_path.exists()
    assert not (bin_dir / "mluva").exists()
    assert not (bin_dir / "voice-scribe").exists()
    assert not (bin_dir / "mluva-input-helper").exists()
    assert not (bin_dir / "voice-scribe-input-helper").exists()
    assert not (bin_dir / "mluva-overlay").exists()
    assert not (bin_dir / "voice-scribe-overlay").exists()
    assert not (bin_dir / "mluva-uninstall").exists()
    assert (bin_dir / "unrelated-command").read_text(encoding="utf-8") == "preserve\n"
    assert config_marker.read_text(encoding="utf-8") == "preserve\n"
    assert history_marker.read_text(encoding="utf-8") == "preserve\n"


def test_uninstaller_refuses_an_unrecognized_application_directory(tmp_path: Path) -> None:
    """Fail before deleting anything when the install path does not carry Mluva's package identity."""
    install_home = tmp_path / "staged-home"
    application_dir = install_home / ".local" / "share" / "voice-scribe" / "app"
    sentinel = application_dir / "do-not-delete"
    application_dir.mkdir(parents=True)
    sentinel.write_text("preserve\n", encoding="utf-8")

    script = Path(__file__).parents[1] / "uninstall.sh"
    result = subprocess.run(
        [str(script)],
        env={
            "HOME": str(tmp_path / "live-home"),
            "MLUVA_INSTALL_HOME": str(install_home),
            "PATH": os.environ["PATH"],
        },
        capture_output=True,
        text=True,
        check=False,
    )

    assert result.returncode != 0
    assert "unrecognized application directory" in result.stderr
    assert sentinel.read_text(encoding="utf-8") == "preserve\n"


def test_installer_has_an_isolated_staging_root() -> None:
    """Let package verification exercise the real installer without touching the active user prefix or Shell."""
    installer = (Path(__file__).parents[1] / "install.sh").read_text(encoding="utf-8")

    assert 'install_home="${MLUVA_INSTALL_HOME:-${HOME}}"' in installer
    assert '"${install_home}" == "/"' in installer
    assert 'data_home="${install_home}/.local/share"' in installer
    assert 'config_home="${install_home}/.config"' in installer
    assert 'data_home="${XDG_DATA_HOME:-${install_home}/.local/share}"' in installer
    assert 'config_home="${XDG_CONFIG_HOME:-${install_home}/.config}"' in installer
    assert 'bin_dir="${install_home}/.local/bin"' in installer
    assert 'secret_config_dir="${config_home}/daniel-ai-skills"' in installer
    assert 'if [[ "${staged_install}" == "false" ]]' in installer
    assert "Staged verification skipped live GNOME extension" in installer


def test_installer_rejects_an_unrecognized_application_directory(tmp_path: Path) -> None:
    """Fail before overwriting a staged path that does not carry a recognized package identity."""
    install_home = tmp_path / "staged-home"
    application_dir = install_home / ".local" / "share" / "voice-scribe" / "app"
    sentinel = application_dir / "do-not-overwrite"
    application_dir.mkdir(parents=True)
    sentinel.write_text("preserve\n", encoding="utf-8")

    script = Path(__file__).parents[1] / "install.sh"
    result = subprocess.run(
        [str(script)],
        env={
            "HOME": str(tmp_path / "live-home"),
            "MLUVA_INSTALL_HOME": str(install_home),
            "PATH": os.environ["PATH"],
        },
        capture_output=True,
        text=True,
        check=False,
    )

    assert result.returncode != 0
    assert "unrecognized application directory" in result.stderr
    assert sentinel.read_text(encoding="utf-8") == "preserve\n"


def test_installer_rejects_an_unsafe_live_xdg_root(tmp_path: Path) -> None:
    """Never derive the application directory directly below the filesystem root."""
    script = Path(__file__).parents[1] / "install.sh"
    result = subprocess.run(
        [str(script)],
        env={
            "HOME": str(tmp_path / "live-home"),
            "XDG_DATA_HOME": "/",
            "PATH": os.environ["PATH"],
        },
        capture_output=True,
        text=True,
        check=False,
    )

    assert result.returncode != 0
    assert "unsafe XDG root" in result.stderr


def test_installer_preserves_an_unrelated_canonical_command(tmp_path: Path) -> None:
    """Do not overwrite another executable that already owns the public command name."""
    install_home = tmp_path / "staged-home"
    command_path = install_home / ".local" / "bin" / "mluva"
    _write_executable(command_path, "#!/bin/sh\nprintf 'unrelated\\n'\n")

    script = Path(__file__).parents[1] / "install.sh"
    result = subprocess.run(
        [str(script)],
        env={
            "HOME": str(tmp_path / "live-home"),
            "MLUVA_INSTALL_HOME": str(install_home),
            "PATH": os.environ["PATH"],
        },
        capture_output=True,
        text=True,
        check=False,
    )

    assert result.returncode != 0
    assert "unrelated command" in result.stderr
    assert command_path.read_text(encoding="utf-8") == "#!/bin/sh\nprintf 'unrelated\\n'\n"


def test_installer_preserves_an_unrelated_helper_link(tmp_path: Path) -> None:
    """Do not retarget another tool's symlink when installing optional helper commands."""
    install_home = tmp_path / "staged-home"
    command_path = install_home / ".local" / "bin" / "mluva-input-helper"
    command_path.parent.mkdir(parents=True)
    command_path.symlink_to("unrelated-helper")

    script = Path(__file__).parents[1] / "install.sh"
    result = subprocess.run(
        [str(script)],
        env={
            "HOME": str(tmp_path / "live-home"),
            "MLUVA_INSTALL_HOME": str(install_home),
            "PATH": os.environ["PATH"],
        },
        capture_output=True,
        text=True,
        check=False,
    )

    assert result.returncode != 0
    assert "unrelated command" in result.stderr
    assert command_path.is_symlink()
    assert os.readlink(command_path) == "unrelated-helper"


def test_installer_rolls_back_a_failed_environment_sync(tmp_path: Path) -> None:
    """Restore the complete previous application when production dependency setup fails."""
    install_home = tmp_path / "staged-home"
    application_dir = install_home / ".local" / "share" / "voice-scribe" / "app"
    previous_marker = application_dir / "previous-version"
    application_dir.mkdir(parents=True)
    (application_dir / "pyproject.toml").write_text('name = "voice-scribe-linux"\n', encoding="utf-8")
    previous_marker.write_text("working\n", encoding="utf-8")

    fake_bin = tmp_path / "fake-bin"
    _write_installer_command_doubles(fake_bin, sync_status=73)

    script = Path(__file__).parents[1] / "install.sh"
    result = subprocess.run(
        [str(script)],
        env={
            "HOME": str(tmp_path / "live-home"),
            "MLUVA_INSTALL_HOME": str(install_home),
            "PATH": f"{fake_bin}:{os.environ['PATH']}",
        },
        capture_output=True,
        text=True,
        check=False,
    )

    assert result.returncode == 73
    assert previous_marker.read_text(encoding="utf-8") == "working\n"
    assert not (application_dir / "voice_scribe_linux").exists()
    assert list(application_dir.parent.glob(".app.previous.*")) == []


def test_installer_commits_a_complete_legacy_upgrade(tmp_path: Path) -> None:
    """Replace a recognized legacy app only after its new production environment succeeds."""
    install_home = tmp_path / "staged-home"
    application_dir = install_home / ".local" / "share" / "voice-scribe" / "app"
    previous_marker = application_dir / "previous-version"
    application_dir.mkdir(parents=True)
    (application_dir / "pyproject.toml").write_text('name = "voice-scribe-linux"\n', encoding="utf-8")
    previous_marker.write_text("legacy\n", encoding="utf-8")
    legacy_launcher = install_home / ".local" / "bin" / "voice-scribe"
    _write_executable(
        legacy_launcher,
        f'#!/bin/sh\napplication_dir="{application_dir}"\nexec python -m voice_scribe_linux.app "$@"\n',
    )

    fake_bin = tmp_path / "fake-bin"
    _write_installer_command_doubles(fake_bin, sync_status=0)
    script = Path(__file__).parents[1] / "install.sh"
    result = subprocess.run(
        [str(script)],
        env={
            "HOME": str(tmp_path / "live-home"),
            "MLUVA_INSTALL_HOME": str(install_home),
            "PATH": f"{fake_bin}:{os.environ['PATH']}",
        },
        capture_output=True,
        text=True,
        check=False,
    )

    assert result.returncode == 0, result.stderr
    assert not previous_marker.exists()
    assert 'name = "mluva-linux"' in (application_dir / "pyproject.toml").read_text(encoding="utf-8")
    assert (application_dir / ".venv" / "bin" / "python").is_file()
    assert list(application_dir.parent.glob(".app.previous.*")) == []
    assert (install_home / ".local" / "bin" / "mluva").is_file()
    assert legacy_launcher.is_symlink()
    assert os.readlink(legacy_launcher) == "mluva"


def test_installer_verifies_the_production_environment_without_resyncing_dev_dependencies() -> None:
    """Keep pytest and Ruff out of the installed application after the production-only frozen sync."""
    installer = (Path(__file__).parents[1] / "install.sh").read_text(encoding="utf-8")

    assert 'uv sync --project "${application_dir}" --no-dev --frozen' in installer
    assert '"${application_dir}/.venv/bin/python" -c' in installer
    assert 'gi.require_version("DBus", "1.0")' in installer
    assert 'gi.require_version("cairo", "1.0")' in installer
    assert 'uv run --project "${application_dir}"' not in installer


def test_makefile_prepares_system_gobject_before_running_linux_checks() -> None:
    """Keep clean checkouts from creating a virtual environment that cannot import distro PyGObject."""
    makefile = (Path(__file__).parents[2] / "Makefile").read_text(encoding="utf-8")

    assert "linux-test: linux-setup" in makefile
    assert "linux-text-target-test: linux-setup" in makefile
    assert "linux-run: linux-setup" in makefile
    assert "uv venv --clear --system-site-packages --python /usr/bin/python3" in makefile
    assert "uv sync --locked" in makefile
    assert "uv run --no-sync python -c" in makefile
    assert "cd linux && uv run --locked pytest -q" in makefile
    assert "uv run --locked ruff check ." in makefile
    assert "uv run --locked python -m voice_scribe_linux.app" in makefile
    assert 'gi.require_version("DBus", "1.0")' in makefile
    assert 'gi.require_version("cairo", "1.0")' in makefile


def test_uv_lock_does_not_inherit_a_contributors_global_release_cutoff() -> None:
    """Keep the checked lockfile valid on a clean machine regardless of user-level uv policy."""
    linux_root = Path(__file__).parents[1]
    project = tomllib.loads((linux_root / "pyproject.toml").read_text(encoding="utf-8"))
    lock = tomllib.loads((linux_root / "uv.lock").read_text(encoding="utf-8"))

    assert project["tool"]["uv"]["exclude-newer"] is False
    assert "exclude-newer" not in lock["options"]
    assert "exclude-newer-span" not in lock["options"]


def test_installer_keeps_codex_optional() -> None:
    """Allow basic dictation and Meeting installation without the optional enhancement client."""
    installer = (Path(__file__).parents[1] / "install.sh").read_text(encoding="utf-8")

    assert "require_command codex" not in installer


def test_installer_uses_generic_managed_credential_wording() -> None:
    """Describe the optional secret boundary without naming its backing store."""
    installer = (Path(__file__).parents[1] / "install.sh").read_text(encoding="utf-8")

    matching_lines = [line for line in installer.splitlines() if "reviewed ElevenLabs credential reference" in line]
    assert matching_lines == [
        '        echo "The launcher will resolve only the reviewed ElevenLabs credential reference at runtime."'
    ]


def test_launcher_runs_without_managed_launcher_when_direct_key_exists(tmp_path: Path) -> None:
    """Keep conventional environment injection working on other installations."""
    launcher_path, application_dir = _render_launcher(tmp_path)
    environ, home = _launcher_environment(tmp_path)
    environ["ELEVENLABS_API_KEY"] = "test-only"
    _write_executable(
        home / ".config" / "daniel-ai-skills" / "bin" / "das-mcp-launch",
        "#!/bin/sh\nexit 91\n",
    )

    result = subprocess.run(
        [str(launcher_path)],
        env=environ,
        capture_output=True,
        text=True,
        check=False,
    )

    assert result.returncode == 0, result.stderr
    assert result.stdout.strip() == f"python:{application_dir}:-m voice_scribe_linux.app"


def test_secret_profile_keeps_only_the_reviewed_credential_reference(tmp_path: Path) -> None:
    """Derive one scoped profile from the agent catalog without resolving or copying unrelated fields."""
    config_dir = tmp_path / "daniel-ai-skills"
    environment_dir = config_dir / "env"
    environment_dir.mkdir(parents=True)
    (environment_dir / "agent.env").write_text(
        "UNRELATED_TOKEN=op://test/unrelated/token\n"
        "DAS_ITEM_ELEVEN_LABS_API_KEY__CREDENTIAL=op://test/elevenlabs/credential\n"
        "DAS_ITEM_ELEVEN_LABS_API_KEY__USERNAME=op://test/elevenlabs/username\n",
        encoding="utf-8",
    )
    _write_executable(config_dir / "bin" / "das-mcp-launch", "#!/bin/sh\nexit 0\n")
    script = Path(__file__).parents[1] / "configure-secret-profile.sh"
    result = subprocess.run(
        [str(script)],
        env={"DAS_CONF_DIR": str(config_dir), "HOME": str(tmp_path), "PATH": os.environ["PATH"]},
        capture_output=True,
        text=True,
        check=False,
    )

    profile = environment_dir / "voice-scribe.env"
    assert result.returncode == 0, result.stderr
    assert "op://" not in result.stdout
    assert profile.read_text(encoding="utf-8") == "ELEVENLABS_API_KEY=op://test/elevenlabs/credential\n"
    assert profile.stat().st_mode & 0o777 == 0o600


def test_secret_profile_rejects_a_missing_reviewed_reference(tmp_path: Path) -> None:
    """Fail closed when the agent catalog cannot name exactly one reviewed credential field."""
    config_dir = tmp_path / "daniel-ai-skills"
    environment_dir = config_dir / "env"
    environment_dir.mkdir(parents=True)
    (environment_dir / "agent.env").write_text(
        "ELEVEN_LABS_STT_TOKEN=op://test/legacy/token\n",
        encoding="utf-8",
    )
    _write_executable(config_dir / "bin" / "das-mcp-launch", "#!/bin/sh\nexit 0\n")
    script = Path(__file__).parents[1] / "configure-secret-profile.sh"
    result = subprocess.run(
        [str(script)],
        env={"DAS_CONF_DIR": str(config_dir), "HOME": str(tmp_path), "PATH": os.environ["PATH"]},
        capture_output=True,
        text=True,
        check=False,
    )

    assert result.returncode != 0
    assert not (environment_dir / "voice-scribe.env").exists()
