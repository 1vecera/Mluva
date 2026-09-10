"""Prove scoped managed-agent fallback without contacting a credential service."""

import os
import subprocess
from pathlib import Path


def test_agent_fallback_requests_only_elevenlabs_and_preserves_arguments(tmp_path: Path) -> None:
    """Use the agent route only when the older scoped profile is absent."""
    config = tmp_path / "managed"
    (config / "bin").mkdir(parents=True)
    (config / "env").mkdir()
    (config / "env" / "agent.env").write_text("synthetic reference marker\n")
    agent = config / "bin" / "das-agent-launch"
    agent.write_text('#!/bin/sh\nprintf "%s\\n" "$@"\n')
    agent.chmod(0o700)
    launcher = tmp_path / "mluva"
    template = Path(__file__).parents[1] / "resources" / "mluva.in"
    launcher.write_text(template.read_text().replace("@APPLICATION_DIR@", str(tmp_path / "application")))
    launcher.chmod(0o700)
    result = subprocess.run(
        [str(launcher), "argument with spaces"],
        env={"PATH": os.environ["PATH"], "HOME": str(tmp_path), "DAS_CONF_DIR": str(config)},
        capture_output=True,
        text=True,
        check=True,
    )
    assert result.stdout.splitlines() == [
        "--only",
        "DAS_ITEM_ELEVEN_LABS_API_KEY__CREDENTIAL",
        "--",
        "env",
        "MLUVA_SECRET_PROFILE=1",
        str(launcher),
        "argument with spaces",
    ]


def test_direct_credential_skips_managed_agent_resolution(tmp_path: Path) -> None:
    """Preserve direct environment injection and avoid a second credential hop."""
    application = tmp_path / "application"
    interpreter = application / ".venv" / "bin" / "python"
    interpreter.parent.mkdir(parents=True)
    interpreter.write_text('#!/bin/sh\nprintf "application-started\\n"\n')
    interpreter.chmod(0o700)
    launcher = tmp_path / "mluva"
    template = Path(__file__).parents[1] / "resources" / "mluva.in"
    launcher.write_text(template.read_text().replace("@APPLICATION_DIR@", str(application)))
    launcher.chmod(0o700)
    result = subprocess.run(
        [str(launcher)],
        env={"PATH": os.environ["PATH"], "HOME": str(tmp_path), "ELEVENLABS_API_KEY": "synthetic"},
        capture_output=True,
        text=True,
        check=True,
    )
    assert result.stdout == "application-started\n"
