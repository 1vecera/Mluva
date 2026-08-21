"""Contract coverage for the Codex app-server subprocess client."""

import sys
from pathlib import Path

import pytest

from voice_scribe_linux.codex_client import CodexAppServerClient, CodexAppServerError


def test_transform_uses_current_app_server_protocol() -> None:
    """Prove the client handshake and turn lifecycle against an independent process."""
    fake_server = Path(__file__).with_name("fake_app_server.py")
    client = CodexAppServerClient(command=(sys.executable, str(fake_server)))
    try:
        assert client.resolve_model(None) == "gpt-5.4"
        assert client.resolve_model("codex-explicit") == "gpt-5.4-mini"
        assert client.transform("Clean this", cwd=Path.cwd(), model="gpt-5.4-mini") == "Clean text."
        assert client.last_model_identifier == "gpt-5.4-mini"
    finally:
        client.close()


def test_app_server_stderr_cannot_block_protocol_progress() -> None:
    """Discard untrusted diagnostic volume instead of leaving an unread child pipe."""
    fake_server = Path(__file__).with_name("fake_app_server.py")
    client = CodexAppServerClient(
        command=(sys.executable, str(fake_server), "--fill-stderr"),
        request_timeout_seconds=2,
    )
    try:
        assert client.transform("Clean this", cwd=Path.cwd()) == "Clean text."
    finally:
        client.close()


def test_exited_app_server_restarts_for_the_next_transformation() -> None:
    """Recover after an app-server process exits between independent captures."""
    fake_server = Path(__file__).with_name("fake_app_server.py")
    client = CodexAppServerClient(command=(sys.executable, str(fake_server), "--single-turn"))
    try:
        frozen_model = client.resolve_model(None)
        assert client.transform("First", cwd=Path.cwd(), model=frozen_model) == "Clean text."
        assert client.process is not None
        client.process.wait(timeout=2)
        assert client.transform("Second", cwd=Path.cwd(), model=frozen_model) == "Clean text."
    finally:
        client.close()


def test_app_server_exit_during_turn_fails_immediately_with_controlled_error() -> None:
    """Wake a pending transformation when the protocol reader reaches EOF."""
    fake_server = Path(__file__).with_name("fake_app_server.py")
    client = CodexAppServerClient(
        command=(sys.executable, str(fake_server), "--exit-during-turn"),
        turn_timeout_seconds=10,
    )
    try:
        with pytest.raises(CodexAppServerError, match="exited before completing"):
            client.transform("Clean this", cwd=Path.cwd())
    finally:
        client.close()


def test_model_resolution_rejects_unavailable_configuration_without_echoing_it() -> None:
    """Fail before a capture when a configured model is absent from the app-server catalog."""
    fake_server = Path(__file__).with_name("fake_app_server.py")
    client = CodexAppServerClient(command=(sys.executable, str(fake_server)))
    try:
        with pytest.raises(CodexAppServerError, match="configured Codex model is unavailable") as failure:
            client.resolve_model("private-model-alias")
        assert "private-model-alias" not in str(failure.value)
    finally:
        client.close()


def test_missing_app_server_command_returns_a_controlled_start_failure() -> None:
    """Keep local executable paths out of the preparation error shown by the application."""
    client = CodexAppServerClient(command=("mluva-command-that-does-not-exist",))

    with pytest.raises(CodexAppServerError, match="could not start") as failure:
        client.resolve_model(None)

    assert "mluva-command-that-does-not-exist" not in str(failure.value)


def test_spawn_copies_bounds_without_sharing_process_or_cancellation() -> None:
    """Give each concurrent segment an independent app-server lifecycle."""
    parent = CodexAppServerClient(
        command=("fake-codex", "app-server"),
        request_timeout_seconds=7,
        turn_timeout_seconds=11,
    )

    child = parent.spawn()
    child.cancel()

    assert child is not parent
    assert tuple(child.command) == tuple(parent.command)
    assert child.request_timeout_seconds == 7
    assert child.turn_timeout_seconds == 11
    assert child.process is None
    assert not parent._cancel_requested.is_set()


def test_cancel_before_start_prevents_subprocess_launch() -> None:
    """Close the race where a timed-out segment could start Codex after cancellation."""
    client = CodexAppServerClient(command=("mluva-command-that-does-not-exist",))
    client.cancel()

    with pytest.raises(CodexAppServerError, match="work was cancelled"):
        client.transform("must not start", cwd=Path.cwd(), model="gpt-5.4-test")

    assert client.process is None


def test_oversized_stream_is_stopped_at_the_client_boundary() -> None:
    """Bound accumulated app-server output before workflow validation or History can see it."""
    fake_server = Path(__file__).with_name("fake_app_server.py")
    client = CodexAppServerClient(command=(sys.executable, str(fake_server), "--oversized-output"))

    with pytest.raises(CodexAppServerError, match="exceeded the supported bound"):
        client.transform("Clean this", cwd=Path.cwd())

    assert client.process is None
