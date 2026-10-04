"""Manual credential helper and remaining development lockfile contracts."""

import os
import subprocess
import tomllib
from pathlib import Path


def _write_executable(path: Path, content: str) -> None:
    """Create an owner-executable test double."""
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")
    path.chmod(0o700)


def test_uv_lock_does_not_inherit_a_contributors_global_release_cutoff() -> None:
    """Keep the checked lockfile valid on a clean machine regardless of user-level uv policy."""
    linux_root = Path(__file__).parents[1]
    project = tomllib.loads((linux_root / "pyproject.toml").read_text(encoding="utf-8"))
    lock = tomllib.loads((linux_root / "uv.lock").read_text(encoding="utf-8"))

    assert project["tool"]["uv"]["exclude-newer"] is False
    assert "exclude-newer" not in lock["options"]
    assert "exclude-newer-span" not in lock["options"]


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

    profile = environment_dir / "mluva.env"
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
    assert not (environment_dir / "mluva.env").exists()
