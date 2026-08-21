"""Contract checks for explicit GNOME recording-overlay installation."""

import os
import subprocess
from pathlib import Path

LINUX_ROOT = Path(__file__).resolve().parents[1]


def test_recording_overlay_helper_is_valid_and_not_automatic() -> None:
    """Package the extension for explicit review without mutating Shell during app install."""
    helper = LINUX_ROOT / "configure-recording-overlay.sh"
    subprocess.run(["bash", "-n", str(helper)], check=True)
    helper_source = helper.read_text()
    installer = (LINUX_ROOT / "install.sh").read_text()

    assert "gnome-extensions pack" in helper_source
    assert "--extra-source recordingOverlay.js" in helper_source
    assert "gnome-extensions install --force" in helper_source
    assert 'gnome-extensions enable "${extension_uuid}"' in helper_source
    assert "mluva-overlay install" in installer
    assert "configure-recording-overlay.sh install" not in installer
    assert 'ln -sfn "${application_dir}/configure-recording-overlay.sh"' in installer


def test_recording_overlay_install_packs_installs_and_enables_with_fake_shell(tmp_path: Path) -> None:
    """Exercise the explicit helper without changing the active GNOME Shell session."""
    fake_bin = tmp_path / "bin"
    fake_bin.mkdir()
    command_log = tmp_path / "commands.log"
    fake_gnome_extensions = fake_bin / "gnome-extensions"
    fake_gnome_extensions.write_text(
        """#!/usr/bin/env bash
set -euo pipefail
printf '%s\\n' "$*" >> "${VOICE_SCRIBE_OVERLAY_COMMAND_LOG}"
if [[ "$1" == "pack" ]]; then
    shift
    output_dir=""
    while (($#)); do
        if [[ "$1" == "--out-dir" ]]; then
            output_dir=$2
            break
        fi
        shift
    done
    touch "${output_dir}/recording-status@voicescribe.local.shell-extension.zip"
fi
"""
    )
    fake_gnome_extensions.chmod(0o755)
    environment = os.environ | {
        "PATH": f"{fake_bin}:{os.environ['PATH']}",
        "VOICE_SCRIBE_OVERLAY_COMMAND_LOG": str(command_log),
    }

    public_command = tmp_path / "mluva-overlay"
    public_command.symlink_to(LINUX_ROOT / "configure-recording-overlay.sh")
    subprocess.run(
        [str(public_command), "install"],
        check=True,
        env=environment,
        capture_output=True,
        text=True,
    )

    commands = command_log.read_text().splitlines()
    assert commands[0].startswith("pack --force --extra-source recordingOverlay.js --out-dir ")
    assert commands[0].endswith(str(LINUX_ROOT / "gnome-extension" / "recording-status@voicescribe.local"))
    assert commands[1].startswith("install --force /tmp/mluva-overlay.")
    assert commands[2] == "info recording-status@voicescribe.local"
    assert commands[3] == "enable recording-status@voicescribe.local"
