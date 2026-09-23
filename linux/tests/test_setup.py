"""Exercise setup ordering without changing packages or the desktop."""

import shutil
import subprocess
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]


@pytest.fixture
def setup_environment(tmp_path: Path) -> tuple[Path, dict[str, str]]:
    """Run the actual setup script with isolated home and logged external commands."""
    source = tmp_path / "source"
    (source / "linux").mkdir(parents=True)
    shutil.copyfile(ROOT / "install.sh", source / "install.sh")
    (source / "linux/install.sh").write_text('#!/bin/bash\necho native >> "$SETUP_LOG"\nexit "${NATIVE_STATUS:-0}"\n')
    commands = tmp_path / "bin"
    commands.mkdir()
    for name in ("bash", "dirname"):
        executable = shutil.which(name)
        assert executable is not None
        (commands / name).symlink_to(executable)
    scripts = {
        "id": '#!/bin/bash\necho "${TEST_UID:-1000}"\n',
        "python3": (
            '#!/bin/bash\n[[ "$1" == */linux/install_widget.py ]] || exit 9\n'
            'if [[ "${2:-}" == "--check" ]]; then\n'
            '  echo widget-check >> "$SETUP_LOG"; exit "${PREFLIGHT_STATUS:-0}"\n'
            'fi\necho widget-install >> "$SETUP_LOG"\nexit "${PLUGIN_STATUS:-0}"\n'
        ),
        "omarchy": (
            '#!/bin/bash\nprintf "omarchy %s\\n" "$*" >> "$SETUP_LOG"\n'
            'if [[ "$1 $2" == "pkg add" ]]; then exit "${PACKAGE_STATUS:-0}"; fi\n'
        ),
        "sudo": '#!/bin/bash\nprintf "sudo %s\\n" "$*" >> "$SETUP_LOG"\nexit "${PACKAGE_STATUS:-0}"\n',
        "dnf": "#!/bin/bash\nexit 0\n",
    }
    for name, script in scripts.items():
        path = commands / name
        path.write_text(script)
        path.chmod(0o755)
    test_home = tmp_path / "home"
    test_home.mkdir()
    return source / "install.sh", {
        "PATH": str(commands),
        "HOME": str(test_home),
        "USER": "test-user",
        "SETUP_LOG": str(tmp_path / "commands.log"),
    }


def run_setup(
    setup: tuple[Path, dict[str, str]], *arguments: str
) -> tuple[subprocess.CompletedProcess[str], list[str]]:
    """Collect the script result and its ordered external operations."""
    script, environment = setup
    result = subprocess.run(
        ["bash", str(script), *arguments], env=environment, input="", text=True, capture_output=True, timeout=10
    )
    log = Path(environment["SETUP_LOG"])
    return result, log.read_text().splitlines() if log.exists() else []


def test_omarchy_setup_installs_dependencies_app_then_bundled_widget(setup_environment) -> None:
    """Preflight happens before packages; the widget comes from the same source tree as the app."""
    result, commands = run_setup(setup_environment, "--yes")
    assert result.returncode == 0, result.stderr
    assert commands[:2] == ["omarchy plugin enable --help", "widget-check"]
    assert commands[2].startswith("omarchy pkg add ")
    assert "pipewire-audio" in commands[2]
    assert "python-gobject" in commands[2]
    assert commands[3:] == ["native", "widget-install"]


def test_app_only_preserves_plugins(setup_environment) -> None:
    """Users can install the app without consulting or changing plugins."""
    result, commands = run_setup(setup_environment, "--yes", "--app-only")
    assert result.returncode == 0, result.stderr
    assert len(commands) == 2
    assert commands[0].startswith("omarchy pkg add ")
    assert commands[1] == "native"


def test_fedora_setup_skips_omarchy(setup_environment) -> None:
    """The compatibility route uses its own packages and advertises its test limit."""
    _, environment = setup_environment
    (Path(environment["PATH"]) / "omarchy").unlink()
    result, commands = run_setup(setup_environment, "--yes")
    assert result.returncode == 0, result.stderr
    assert commands[0].startswith("sudo dnf install -y ")
    assert "pipewire-utils" in commands[0]
    assert commands[1:] == ["native"]
    assert "not been tested" in result.stdout


@pytest.mark.parametrize("failure", ["PREFLIGHT_STATUS", "PACKAGE_STATUS", "NATIVE_STATUS", "PLUGIN_STATUS"])
def test_setup_stops_after_failure(setup_environment, failure: str) -> None:
    """Failures stop dependent operations and never report success."""
    _, environment = setup_environment
    environment[failure] = "7"
    result, commands = run_setup(setup_environment, "--yes")
    assert result.returncode == 7
    assert "Setup complete" not in result.stdout
    if failure in {"PREFLIGHT_STATUS", "PACKAGE_STATUS"}:
        assert "native" not in commands
    if failure == "PREFLIGHT_STATUS":
        assert not any("pkg add" in command for command in commands)
    if failure != "PLUGIN_STATUS":
        assert "widget-install" not in commands


@pytest.mark.parametrize("boundary", ["unconfirmed", "root", "staged", "unknown-platform"])
def test_setup_requires_supported_context(setup_environment, boundary: str) -> None:
    """Reject ambiguous or staged contexts before installation."""
    _, environment = setup_environment
    if boundary == "root":
        environment["TEST_UID"] = "0"
    elif boundary == "staged":
        environment["MLUVA_INSTALL_HOME"] = str(Path(environment["HOME"]) / "staged")
    elif boundary == "unknown-platform":
        for command in ("omarchy", "dnf"):
            (Path(environment["PATH"]) / command).unlink()
    result, commands = run_setup(setup_environment, *([] if boundary == "unconfirmed" else ["--yes"]))
    assert result.returncode != 0
    assert not any("pkg add" in command or "dnf install" in command or command == "native" for command in commands)
