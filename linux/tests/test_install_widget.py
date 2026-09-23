"""Check bundled widget upgrades, migration, customization protection and rollback."""

import json
import shutil
import subprocess
from pathlib import Path

import pytest

import install_widget as widget


@pytest.fixture
def installation(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    """Isolate files and shell integration while exercising production installer logic."""
    source = tmp_path / "source"
    shutil.copytree(Path(widget.__file__).parent / "quickshell" / widget.PLUGIN_ID, source)
    home = tmp_path / "home"
    calls = []

    def run(command, **kwargs):
        """Record desktop commands without reaching the live shell."""
        calls.append(command)
        return subprocess.CompletedProcess(command, 0, stdout=json.dumps([{"id": widget.PLUGIN_ID}]))

    monkeypatch.setattr(widget.subprocess, "run", run)
    return source, home, calls


def test_install_and_upgrade_load_new_qml_and_preserve_previous_bundle(installation) -> None:
    """An archive-only install upgrades without Git or a separate repository."""
    source, home, calls = installation
    widget.install_widget(source, home)
    target = home / ".config/omarchy/plugins/mluva.dictation"
    old_manifest = json.loads((target / "manifest.json").read_text())
    old_entry = old_manifest["entryPoints"]["barWidget"]
    assert (target / old_entry).read_bytes() == (source / "Widget.qml").read_bytes()
    assert (target / Path(old_entry).parent / "fonts/JetBrainsMono-Regular.ttf").is_file()
    (source / "Widget.qml").write_text("// upgraded widget\n")
    widget.install_widget(source, home)
    new_entry = json.loads((target / "manifest.json").read_text())["entryPoints"]["barWidget"]
    assert old_entry != new_entry
    assert (target / new_entry).read_text() == "// upgraded widget\n"
    backups = list((home / ".config/omarchy/plugin-backups").glob("*/mluva.dictation"))
    assert len(backups) == 1
    assert (backups[0] / old_entry).exists()
    assert widget.existing_plugin(target.parent) == target
    widget.check_existing(target)
    assert calls[-3:] == [
        ["omarchy-shell", "shell", "rescanPlugins"],
        ["omarchy", "plugin", "list", "--json"],
        ["omarchy", "plugin", "enable", widget.PLUGIN_ID],
    ]


def test_check_is_read_only(installation) -> None:
    """Preflight validates source without creating any user configuration."""
    source, home, calls = installation
    widget.install_widget(source, home, check_only=True)
    assert not home.exists()
    assert calls == [["omarchy", "plugin", "validate", str(source)]]


@pytest.mark.parametrize("change", ["edit", "extra", "missing", "symlink", "unmanaged", "duplicate"])
def test_preserve_customized_or_ambiguous_installation(installation, change: str) -> None:
    """Updates reject edits, missing files, links, manual installs and duplicate identities."""
    source, home, calls = installation
    widget.install_widget(source, home)
    target = home / ".config/omarchy/plugins/mluva.dictation"
    entry = target / json.loads((target / "manifest.json").read_text())["entryPoints"]["barWidget"]
    if change == "edit":
        entry.write_text("// customization\n")
    elif change == "extra":
        (target / "custom.qml").write_text("// customization\n")
    elif change == "missing":
        entry.unlink()
    elif change == "symlink":
        entry.unlink()
        entry.symlink_to(source / "Widget.qml")
    elif change == "unmanaged":
        (target / widget.RECEIPT).unlink()
    else:
        shutil.copytree(target, target.with_name("mluva-old"))
    calls.clear()
    with pytest.raises(ValueError):
        widget.install_widget(source, home)
    assert calls == []
    assert target.exists()
    assert not (home / ".config/omarchy/plugin-backups").exists()


@pytest.mark.parametrize("failure", ["rescanPlugins", "enable"])
def test_failed_shell_update_restores_previous_files(installation, monkeypatch, failure: str) -> None:
    """A shell failure leaves the prior working widget in place."""
    source, home, _ = installation
    widget.install_widget(source, home)
    target = home / ".config/omarchy/plugins/mluva.dictation"
    before = widget.file_hashes(target)
    (source / "Widget.qml").write_text("// upgraded\n")

    def fail(command, **kwargs):
        """Fail the chosen shell operation once; allow best-effort rollback rescan."""
        if failure in command and kwargs.get("check"):
            raise subprocess.CalledProcessError(7, command)
        return subprocess.CompletedProcess(command, 0, stdout=json.dumps([{"id": widget.PLUGIN_ID}]))

    monkeypatch.setattr(widget.subprocess, "run", fail)
    with pytest.raises(subprocess.CalledProcessError):
        widget.install_widget(source, home)
    assert widget.file_hashes(target) == before


@pytest.mark.parametrize("state", ["clean", "dirty", "local-commit", "different-origin"])
def test_legacy_git_migration_without_network(tmp_path, monkeypatch, state: str) -> None:
    """Only unchanged published legacy checkouts can migrate after their remote is retired."""
    for variable in ("AUTHOR", "COMMITTER"):
        monkeypatch.setenv(f"GIT_{variable}_NAME", "Fixture")
        monkeypatch.setenv(f"GIT_{variable}_EMAIL", "fixture@example.invalid")
    monkeypatch.setenv("GIT_CONFIG_GLOBAL", "/dev/null")
    upstream = tmp_path / "upstream"
    target = tmp_path / "home/.config/omarchy/plugins/mluva.dictation-old"
    target.parent.mkdir(parents=True)
    source = Path(widget.__file__).parent / "quickshell" / widget.PLUGIN_ID
    shutil.copytree(source, upstream)
    for arguments in (
        ["init", str(upstream)],
        ["-C", str(upstream), "add", "."],
        ["-C", str(upstream), "commit", "-m", "Published widget"],
        ["clone", str(upstream), str(target)],
        ["-C", str(target), "remote", "set-url", "origin", sorted(widget.LEGACY_ORIGINS)[0]],
    ):
        subprocess.run(["git", *arguments], check=True, capture_output=True)
    if state == "dirty":
        (target / "Widget.qml").write_text("// local change\n")
    elif state == "local-commit":
        subprocess.run(
            ["git", "-C", str(target), "commit", "--allow-empty", "-m", "Local"], check=True, capture_output=True
        )
    elif state == "different-origin":
        subprocess.run(
            ["git", "-C", str(target), "remote", "set-url", "origin", "https://example.org/widget"], check=True
        )
    if state == "clean":
        real_run = subprocess.run

        def run(command, **kwargs):
            """Allow local Git reads while intercepting shell changes."""
            if command[0] == "git":
                return real_run(command, **kwargs)
            return subprocess.CompletedProcess(command, 0, stdout=json.dumps([{"id": widget.PLUGIN_ID}]))

        monkeypatch.setattr(widget.subprocess, "run", run)
        widget.install_widget(source, tmp_path / "home")
        assert not target.exists()
        assert (target.with_name(widget.PLUGIN_ID) / widget.RECEIPT).is_file()
        assert list((target.parent.parent / "plugin-backups").glob("*/mluva.dictation-old/.git"))
    else:
        with pytest.raises(ValueError, match="Preserved"):
            widget.check_existing(target)


@pytest.mark.parametrize("discovered", [True, False])
def test_wait_for_asynchronous_shell_discovery(installation, monkeypatch, discovered: bool) -> None:
    """Allow a delayed first scan and roll back if the shell never discovers the widget."""
    source, home, calls = installation
    scans = []

    def output(*args, **kwargs):
        """Return an initially empty registry to reproduce the asynchronous scan."""
        scans.append(1)
        return json.dumps([{"id": widget.PLUGIN_ID}] if discovered and len(scans) > 1 else [])

    monkeypatch.setattr(widget.subprocess, "check_output", output)
    monkeypatch.setattr(widget.time, "sleep", lambda _: None)
    if discovered:
        widget.install_widget(source, home)
        assert len(scans) == 2
        assert calls[-1] == ["omarchy", "plugin", "enable", widget.PLUGIN_ID]
    else:
        with pytest.raises(ValueError, match="did not discover"):
            widget.install_widget(source, home)
        assert not (home / ".config/omarchy/plugins/mluva.dictation").exists()
        assert not any("enable" in command for command in calls)
