"""Exercise downloaded editor activation through the real installer and filesystem boundary."""

import os
import subprocess
from pathlib import Path

import pytest

ROOT = Path(__file__).parents[2]


@pytest.fixture
def installation(tmp_path):
    """Provide a native-app layout and an independently observable editor executable."""
    home = tmp_path / "home"
    application = home / ".local/share/mluva/app"
    commands = home / ".local/bin"
    application.mkdir(parents=True)
    commands.mkdir(parents=True)
    for path in (application / "mluva-screenshot-editor", commands / "mluva-narrate"):
        path.write_text("#!/bin/sh\nexit 0\n")
        path.chmod(0o755)
    editor = tmp_path / "editor"
    editor.mkdir()
    for name in ("LICENSE", "NOTICE", "upstream-commit", "narration.patch"):
        (editor / name).write_bytes((ROOT / "linux/integrations/tensaku" / name).read_bytes())
    binary = editor / "tensaku"
    binary.write_text("#!/bin/sh\nprintf '%s\\n' --narration-command\n")
    binary.chmod(0o755)

    def activate():
        """Run the packaged shell entry point without a real user home or editor window."""
        return subprocess.run(
            ["bash", str(ROOT / "linux/install-narrated-editor.sh"), "--prebuilt-dir", str(editor)],
            env=dict(os.environ, MLUVA_INSTALL_HOME=str(home)),
            capture_output=True,
            text=True,
        )

    return home, editor, activate


def test_prebuilt_editor_activates_default_wrapper_without_building(installation):
    """Preserve source notices and install the requested binary before enabling the default editor."""
    home, editor, activate = installation
    assert activate().returncode == 0
    installed = home / ".local/share/mluva/tensaku"
    for source in editor.iterdir():
        assert (installed / source.name).read_bytes() == source.read_bytes()
    assert os.access(installed / "tensaku", os.X_OK)
    assert (home / ".local/bin/tensaku-edit").resolve() == home / ".local/share/mluva/app/mluva-screenshot-editor"


def test_mismatched_source_patch_does_not_activate_or_install(installation):
    """Reject an editor from another source release before invoking or replacing its binary."""
    home, editor, activate = installation
    (editor / "narration.patch").write_text("different release\n")
    assert activate().returncode == 5
    assert not (home / ".local/share/mluva/tensaku").exists()
    assert not (home / ".local/bin/tensaku-edit").exists()


def test_unrelated_default_editor_is_preserved(installation):
    """Leave user-owned editor customizations intact and report the conflict."""
    home, _editor, activate = installation
    existing = home / ".local/bin/tensaku-edit"
    existing.write_text("user-owned editor\n")
    assert activate().returncode == 4
    assert existing.read_text() == "user-owned editor\n"
    assert not (home / ".local/share/mluva/tensaku").exists()
