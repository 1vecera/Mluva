"""Keep provider input out of Docker arguments and inherited service environments."""

import json
import sys
from pathlib import Path
from unittest.mock import Mock

import mac_capture
import pytest


@pytest.fixture
def capture_guest(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> tuple[Path, Mock, Mock]:
    """Provide a fake owned guest and synthetic credentials without opening a provider."""
    root = tmp_path / "checkout"
    (root / "dev").mkdir(parents=True)
    (root / "tmp/real-source").mkdir(parents=True)
    host_home = tmp_path / "host"
    (host_home / ".codex").mkdir(parents=True)
    (host_home / ".codex/auth.json").write_text(json.dumps({"tokens": {"access_token": "synthetic-codex"}}))
    (root / "tmp/installation.json").write_text(
        json.dumps(
            {
                "host": "claw-mini-capture",
                "source_revision": "6022b05e4258768e7aa70305febf7dad0835b060",
                "installed_runtime": "/home/developer/.local/share/voice-scribe/app",
            }
        )
    )
    monkeypatch.setattr(mac_capture, "__file__", str(root / "dev/mac_capture.py"))
    monkeypatch.setattr(mac_capture.socket, "gethostname", lambda: "claw-mini")
    monkeypatch.setenv("HOME", str(host_home))
    monkeypatch.setenv("DAS_ITEM_ELEVEN_LABS_API_KEY__CREDENTIAL", "synthetic-scribe")
    monkeypatch.setenv("UNRELATED_PRIVATE_VALUE", "synthetic-unrelated")
    inspect = Mock(
        return_value=json.dumps(
            [
                {
                    "Config": {
                        "Labels": {"dev.mluva.capture-profile": "claw-mini-film"},
                        "Hostname": "claw-mini-capture",
                    },
                    "State": {"Running": True},
                    "Mounts": [{"Name": "mluva-film-home"}],
                }
            ]
        )
    )
    execute = Mock()
    monkeypatch.setattr(mac_capture.subprocess, "check_output", inspect)
    monkeypatch.setattr(mac_capture.subprocess, "run", execute)
    return root, inspect, execute


@pytest.mark.parametrize("kind", ["preflight", "features", "real"])
def test_credentials_only_travel_on_real_stdin(capture_guest, monkeypatch: pytest.MonkeyPatch, kind: str) -> None:
    """Exercise every adapter route and challenge both inspection and execution boundaries."""
    root, inspect, execute = capture_guest
    arguments = [
        "mac_capture.py",
        kind,
        str(root / "tmp/new-take"),
        "--installation-receipt",
        str(root / "tmp/installation.json"),
    ]
    if kind == "features":
        arguments += ["--source-take", str(root / "tmp/real-source")]
    monkeypatch.setattr(sys, "argv", arguments)
    mac_capture.main()
    for command in (inspect.call_args, execute.call_args):
        assert set(command.kwargs["env"]) <= {"HOME", "PATH", "LANG"}
        assert "synthetic-" not in json.dumps([command.args, command.kwargs["env"]])
    assert execute.call_args.args[0][4:7] == ["-i", "--workdir", str(root)]
    provider_input = execute.call_args.kwargs["input"]
    if kind == "real":
        assert json.loads(provider_input) == {
            "ELEVENLABS_API_KEY": "synthetic-scribe",
            "_codex_auth": {"tokens": {"access_token": "synthetic-codex"}},
        }
    else:
        assert provider_input is None


def test_unowned_guest_stops_before_execution(capture_guest, monkeypatch: pytest.MonkeyPatch) -> None:
    """Reject a wrong volume before sending provider input or starting a capture."""
    root, inspect, execute = capture_guest
    guest = json.loads(inspect.return_value)
    guest[0]["Mounts"] = [{"Name": "unrelated-home"}]
    inspect.return_value = json.dumps(guest)
    monkeypatch.setattr(
        sys,
        "argv",
        [
            "mac_capture.py",
            "real",
            str(root / "tmp/new-take"),
            "--installation-receipt",
            str(root / "tmp/installation.json"),
        ],
    )
    with pytest.raises(RuntimeError, match="home volume changed"):
        mac_capture.main()
    execute.assert_not_called()
