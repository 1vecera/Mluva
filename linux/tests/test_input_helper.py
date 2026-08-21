"""Static contract checks for the explicit keyboard-only paste helper."""

import os
import shutil
import subprocess
from pathlib import Path

LINUX_ROOT = Path(__file__).resolve().parents[1]


def test_input_helper_service_is_owner_only_keyboard_only_and_session_scoped() -> None:
    """Keep the privileged daemon narrower than the distribution's general default service."""
    service = (LINUX_ROOT / "resources" / "voice-scribe-input@.service").read_text()

    assert "--socket-path=/run/user/%i/.ydotool_socket" in service
    assert "--socket-perm=0600" in service
    assert "--socket-own=%i:%i" in service
    assert "--mouse-off" in service
    assert "PartOf=user@%i.service" in service
    assert "RestrictAddressFamilies=AF_UNIX" in service
    assert "DevicePolicy=closed" in service
    assert "DeviceAllow=/dev/uinput rw" in service
    assert "ReadWritePaths=/run/user/%i" in service
    assert "TemporaryFileSystem=/run/user:ro" in service
    assert "BindPaths=/run/user/%i:/run/user/%i" in service
    assert "ProtectHome=no" in service
    assert "InaccessiblePaths=/home /root" in service
    assert "ProtectHome=yes" not in service
    assert "--touch-on" not in service
    assert "--keyboard-off" not in service


def test_input_helper_script_is_valid_and_not_automatic() -> None:
    """Require an explicit operator command instead of enabling privileged input during app install."""
    helper = LINUX_ROOT / "configure-input-helper.sh"
    subprocess.run(["bash", "-n", str(helper)], check=True)
    installer = (LINUX_ROOT / "install.sh").read_text()

    assert "mluva-input-helper install" in installer
    assert "configure-input-helper.sh install" not in installer


def test_installed_input_helper_symlink_resolves_its_packaged_unit(tmp_path: Path) -> None:
    """Find application resources when the public command is the installer's symlink."""
    application_dir = tmp_path / "application"
    resources_dir = application_dir / "resources"
    resources_dir.mkdir(parents=True)
    helper = application_dir / "configure-input-helper.sh"
    shutil.copy2(LINUX_ROOT / "configure-input-helper.sh", helper)
    shutil.copy2(LINUX_ROOT / "resources" / "voice-scribe-input@.service", resources_dir)
    public_command = tmp_path / "bin" / "mluva-input-helper"
    public_command.parent.mkdir()
    public_command.symlink_to(helper)
    fake_systemctl = tmp_path / "fake-bin" / "systemctl"
    fake_systemctl.parent.mkdir()
    fake_systemctl.write_text("#!/usr/bin/env bash\nexit 1\n", encoding="utf-8")
    fake_systemctl.chmod(0o700)
    fake_id = fake_systemctl.parent / "id"
    fake_id.write_text("#!/usr/bin/env bash\nprintf '12345\\n'\n", encoding="utf-8")
    fake_id.chmod(0o700)

    result = subprocess.run(
        [str(public_command), "status"],
        env=os.environ | {"PATH": f"{fake_systemctl.parent}:{os.environ['PATH']}"},
        capture_output=True,
        text=True,
        check=False,
    )

    assert result.returncode == 1
    assert "not ready" in result.stderr
    assert "Missing packaged service template" not in result.stderr
