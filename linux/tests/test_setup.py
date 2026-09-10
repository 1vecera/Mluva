"""Exercise setup ordering and ownership without changing packages or the desktop."""

import os
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
        "git": (
            '#!/bin/bash\nif [[ "${3:-}" == "fetch" ]]; then\n'
            '  [[ "${FETCH_STATUS:-0}" == 0 ]] || exit "$FETCH_STATUS"\n'
            '  exec /usr/bin/git -C "$2" fetch --quiet "$TEST_PLUGIN_REMOTE" HEAD\n'
            'fi\nexec /usr/bin/git "$@"\n'
        ),
        "id": '#!/bin/bash\necho "${TEST_UID:-1000}"\n',
        "omarchy": (
            '#!/bin/bash\nprintf "omarchy %s\\n" "$*" >> "$SETUP_LOG"\n'
            'if [[ "$1 $2" == "pkg add" ]]; then exit "${PACKAGE_STATUS:-0}"; fi\n'
            'if [[ "$1 $2" == "plugin add" && "$3" != "--help" ]]; then '
            'exit "${PLUGIN_STATUS:-0}"; fi\n'
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
    environment = {
        "PATH": str(commands),
        "HOME": str(test_home),
        "USER": "test-user",
        "SETUP_LOG": str(tmp_path / "commands.log"),
        "GIT_CONFIG_GLOBAL": os.devnull,
        "GIT_CONFIG_NOSYSTEM": "1",
        "GIT_AUTHOR_NAME": "Setup test",
        "GIT_AUTHOR_EMAIL": "setup@example.invalid",
        "GIT_COMMITTER_NAME": "Setup test",
        "GIT_COMMITTER_EMAIL": "setup@example.invalid",
    }
    return source / "install.sh", environment


def managed_plugin(environment: dict[str, str]) -> Path:
    """Provide real local Git history while redirecting fetch away from the network."""
    test_home = Path(environment["HOME"])
    upstream = test_home / "upstream"
    plugin = test_home / ".config/omarchy/plugins/mluva.dictation"
    plugin.parent.mkdir(parents=True)
    for arguments in (
        ["init", str(upstream)],
        ["-C", str(upstream), "commit", "--allow-empty", "-m", "Release fixture"],
        ["clone", str(upstream), str(plugin)],
        ["-C", str(plugin), "remote", "set-url", "origin", "https://github.com/1vecera/omarchy-mluva.git"],
    ):
        subprocess.run(["git", *arguments], env=environment, check=True, capture_output=True)
    environment["TEST_PLUGIN_REMOTE"] = str(upstream)
    return plugin


def run_setup(
    setup: tuple[Path, dict[str, str]], *arguments: str
) -> tuple[subprocess.CompletedProcess[str], list[str]]:
    """Collect the script result and the ordered external operations it requested."""
    script, environment = setup
    result = subprocess.run(
        ["bash", str(script), *arguments], env=environment, input="", text=True, capture_output=True, timeout=10
    )
    log = Path(environment["SETUP_LOG"])
    return result, log.read_text().splitlines() if log.exists() else []


def test_omarchy_setup_installs_dependencies_app_then_plugin(setup_environment: tuple[Path, dict[str, str]]) -> None:
    """A fresh setup supplies PipeWire capture before installing and enabling the widget."""
    result, commands = run_setup(setup_environment, "--yes")

    assert result.returncode == 0, result.stderr
    assert commands[0] == "omarchy plugin add --help"
    assert commands[1].startswith("omarchy pkg add ")
    assert "pipewire-audio" in commands[1]
    assert "python-gobject" in commands[1]
    assert commands[2:] == [
        "native",
        "omarchy plugin add https://github.com/1vecera/omarchy-mluva.git --enable --yes",
    ]


def test_app_only_preserves_plugins(setup_environment: tuple[Path, dict[str, str]]) -> None:
    """Users can install the app without consulting or changing their plugin installation."""
    result, commands = run_setup(setup_environment, "--yes", "--app-only")

    assert result.returncode == 0, result.stderr
    assert len(commands) == 2
    assert commands[0].startswith("omarchy pkg add ")
    assert commands[1] == "native"


def test_fedora_setup_skips_omarchy(setup_environment: tuple[Path, dict[str, str]]) -> None:
    """The retained compatibility route uses its own packages and advertises its test limit."""
    _, environment = setup_environment
    (Path(environment["PATH"]) / "omarchy").unlink()

    result, commands = run_setup(setup_environment, "--yes")

    assert result.returncode == 0, result.stderr
    assert commands[0].startswith("sudo dnf install -y ")
    assert "pipewire-utils" in commands[0]
    assert commands[1:] == ["native"]
    assert "not been tested" in result.stdout


@pytest.mark.parametrize("failure", ["PACKAGE_STATUS", "NATIVE_STATUS", "PLUGIN_STATUS"])
def test_setup_does_not_report_success_after_failure(
    setup_environment: tuple[Path, dict[str, str]], failure: str
) -> None:
    """Failed prerequisite stages prevent dependent operations and a false success message."""
    _, environment = setup_environment
    environment[failure] = "7"

    result, commands = run_setup(setup_environment, "--yes")

    assert result.returncode == 7
    assert "Setup complete" not in result.stdout
    if failure == "PACKAGE_STATUS":
        assert "native" not in commands
    if failure != "PLUGIN_STATUS":
        assert not any(command.endswith("--enable --yes") for command in commands)


@pytest.mark.parametrize("conflict", ["manual", "symlink", "dirty", "different-origin"])
def test_setup_preserves_plugin_customizations(setup_environment: tuple[Path, dict[str, str]], conflict: str) -> None:
    """An existing plugin is never silently adopted or overwritten by the combined setup."""
    _, environment = setup_environment
    plugin = Path(environment["HOME"]) / ".config/omarchy/plugins/mluva.dictation"
    plugin.mkdir(parents=True)
    sentinel = plugin / "custom.qml"
    sentinel.write_text("customized widget\n")
    if conflict in {"dirty", "different-origin"}:
        subprocess.run(["git", "init", str(plugin)], env=environment, check=True, capture_output=True)
        origin = (
            "https://example.com/custom-widget.git"
            if conflict == "different-origin"
            else "https://github.com/1vecera/omarchy-mluva.git"
        )
        subprocess.run(["git", "-C", str(plugin), "remote", "add", "origin", origin], env=environment, check=True)
    elif conflict == "symlink":
        target = plugin.with_name("custom-widget")
        plugin.rename(target)
        plugin.symlink_to(target, target_is_directory=True)

    result, commands = run_setup(setup_environment, "--yes")

    assert result.returncode != 0
    assert commands == ["omarchy plugin add --help"]
    assert sentinel.read_text() == "customized widget\n"


def test_setup_updates_a_clean_managed_plugin(setup_environment: tuple[Path, dict[str, str]]) -> None:
    """A rerun delegates update and enablement to Omarchy after installing the native app."""
    _, environment = setup_environment
    managed_plugin(environment)

    result, commands = run_setup(setup_environment, "--yes")

    assert result.returncode == 0, result.stderr
    assert commands[-3:] == [
        "native",
        "omarchy plugin update mluva.dictation --yes",
        "omarchy plugin enable mluva.dictation",
    ]


@pytest.mark.parametrize("failure", ["local-commits", "fetch-unavailable"])
def test_plugin_preflight_rejects_an_unavailable_update_before_app_install(
    setup_environment: tuple[Path, dict[str, str]], failure: str
) -> None:
    """A clean but customized history or unavailable upstream leaves the native app untouched."""
    _, environment = setup_environment
    plugin = managed_plugin(environment)
    if failure == "local-commits":
        subprocess.run(
            ["git", "-C", str(plugin), "commit", "--allow-empty", "-m", "Local customization"],
            env=environment,
            check=True,
            capture_output=True,
        )
    else:
        environment["FETCH_STATUS"] = "7"

    result, commands = run_setup(setup_environment, "--yes")

    assert result.returncode != 0
    assert commands == ["omarchy plugin add --help"]
    assert "No installation was started" in result.stderr


@pytest.mark.parametrize("boundary", ["unconfirmed", "root", "staged", "unknown-platform"])
def test_setup_requires_a_supported_install_context(
    setup_environment: tuple[Path, dict[str, str]], boundary: str
) -> None:
    """Reject ambiguous or staged contexts before any installation operation."""
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
