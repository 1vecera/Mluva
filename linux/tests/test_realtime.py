"""Wire-level and concurrency coverage for ElevenLabs Scribe v2 Realtime."""

import base64
import json
import queue
import threading
from collections.abc import Callable, Iterator
from dataclasses import dataclass, field
from urllib.parse import parse_qs, urlsplit

import pytest
from websockets.sync.server import Server, ServerConnection, serve

from voice_scribe_linux.realtime import (
    ElevenLabsRealtimeClient,
    RealtimeCommittedSegment,
    RealtimePreview,
    RealtimeTranscriptionError,
    RealtimeTranscriptionSession,
)


@dataclass(slots=True)
class RealtimeServerState:
    """Capture the authenticated handshake and decoded client events."""

    request_path: str | None = None
    api_key: str | None = None
    events: list[dict[str, object]] = field(default_factory=list)
    audio: bytearray = field(default_factory=bytearray)


@dataclass(frozen=True, slots=True)
class RealtimeServer:
    """Expose one isolated localhost endpoint and its captured protocol state."""

    endpoint: str
    state: RealtimeServerState


@pytest.fixture
def realtime_server() -> Iterator[RealtimeServer]:
    """Serve deterministic partial, final, language, and committed transcript events."""
    state = RealtimeServerState()

    def handler(connection: ServerConnection) -> None:
        state.request_path = connection.request.path
        state.api_key = connection.request.headers.get("xi-api-key")
        query = parse_qs(urlsplit(state.request_path).query)
        connection.send(
            json.dumps(
                {
                    "message_type": "session_started",
                    "session_id": "scribe-realtime-session",
                    "config": {
                        "model_id": "scribe_v2_realtime",
                        "audio_format": "pcm_16000",
                    },
                }
            )
        )
        for message in connection:
            event = json.loads(message)
            state.events.append(event)
            audio = base64.b64decode(event["audio_base_64"])
            state.audio.extend(audio)
            if event.get("commit") is True:
                if query.get("include_language_detection") == ["true"]:
                    connection.send(
                        json.dumps(
                            {
                                "message_type": "final_transcript_with_timestamps",
                                "text": "Hello world.",
                                "language_code": "eng",
                                "words": [],
                            }
                        )
                    )
                connection.send(json.dumps({"message_type": "committed_transcript", "text": "Hello world."}))
                return
            connection.send(json.dumps({"message_type": "partial_transcript", "text": "Hello wor"}))
            connection.send(json.dumps({"message_type": "final_transcript", "text": "Hello world"}))

    server, thread, endpoint = _start_server(handler)
    yield RealtimeServer(endpoint=endpoint, state=state)
    _stop_server(server, thread)


def test_realtime_session_uses_reviewed_wire_contract_and_committed_text_only(
    realtime_server: RealtimeServer,
) -> None:
    """Stream raw PCM, expose partial text, and return only the committed provider event."""
    previews: list[RealtimePreview] = []
    committed_segments: list[RealtimeCommittedSegment] = []
    client = ElevenLabsRealtimeClient(
        api_key="write-only-test-key",
        endpoint=realtime_server.endpoint,
        finalization_timeout_seconds=2,
    )
    assert "write-only-test-key" not in repr(client)
    session = client.start("auto", previews.append, committed_segments.append)
    pcm_frames = (1_000).to_bytes(2, "little", signed=True) * 1_600

    assert session.submit_audio(pcm_frames)
    result = session.finish()

    assert result.transcription.text == "Hello world."
    assert result.transcription.language_code == "eng"
    assert result.transcription.transcription_id == "scribe-realtime-session"
    assert result.transcription.audio_duration_seconds == pytest.approx(0.1)
    assert result.finalization_seconds >= 0
    assert any(preview.volatile_text == "Hello wor" for preview in previews)
    assert any(preview.volatile_text == "Hello world" for preview in previews)
    assert previews[-1].committed_text == "Hello world."
    assert previews[-1].volatile_text == ""
    assert committed_segments == [
        RealtimeCommittedSegment(
            identifier="scribe-realtime-session-0",
            sequence=0,
            text="Hello world.",
        )
    ]
    assert realtime_server.state.audio == pcm_frames
    assert realtime_server.state.api_key == "write-only-test-key"
    assert realtime_server.state.request_path is not None
    query = parse_qs(urlsplit(realtime_server.state.request_path).query)
    assert query == {
        "audio_format": ["pcm_16000"],
        "commit_strategy": ["manual"],
        "include_language_detection": ["true"],
        "model_id": ["scribe_v2_realtime"],
    }
    assert realtime_server.state.events[-1] == {
        "message_type": "input_audio_chunk",
        "audio_base_64": "",
        "commit": True,
    }


def test_configured_language_is_sent_without_detection_option(realtime_server: RealtimeServer) -> None:
    """Freeze a selected ISO language in the handshake instead of asking for auto-detection."""
    client = ElevenLabsRealtimeClient(
        api_key="write-only-test-key",
        endpoint=realtime_server.endpoint,
        finalization_timeout_seconds=2,
    )
    session = client.start("ces")
    session.submit_audio(bytes(3_200))
    result = session.finish()

    assert result.transcription.language_code == "ces"
    assert realtime_server.state.request_path is not None
    query = parse_qs(urlsplit(realtime_server.state.request_path).query)
    assert query["language_code"] == ["ces"]
    assert "include_language_detection" not in query


def test_finish_waits_until_final_committed_segment_callback_is_dispatched(
    realtime_server: RealtimeServer,
) -> None:
    """Prevent Stop from draining cleanup before its final immutable segment is accepted."""
    callback_started = threading.Event()
    release_callback = threading.Event()
    result_holder: list[object] = []

    def blocking_callback(_segment: RealtimeCommittedSegment) -> None:
        """Hold the receiver exactly at the cleanup handoff boundary."""
        callback_started.set()
        release_callback.wait(timeout=2)

    session = ElevenLabsRealtimeClient(
        api_key="write-only-test-key",
        endpoint=realtime_server.endpoint,
        finalization_timeout_seconds=2,
    ).start("eng", on_committed_segment=blocking_callback)
    assert session.submit_audio(bytes(3_200))

    def finish_session() -> None:
        """Record the terminal result after callback dispatch releases."""
        result_holder.append(session.finish())

    finisher = threading.Thread(target=finish_session, daemon=True)
    finisher.start()
    assert callback_started.wait(timeout=2)
    assert not session._final_commit_received.is_set()
    assert result_holder == []
    release_callback.set()
    finisher.join(timeout=2)
    assert not finisher.is_alive()
    assert len(result_holder) == 1


def test_provider_error_is_sanitized_for_batch_fallback() -> None:
    """Never echo arbitrary provider content when the live route becomes unusable."""

    def handler(connection: ServerConnection) -> None:
        connection.send(json.dumps({"message_type": "session_started", "session_id": "failed-session"}))
        connection.recv()
        connection.send(
            json.dumps(
                {
                    "message_type": "rate_limited",
                    "error": "arbitrary provider payload that must not reach the UI",
                }
            )
        )
        connection.recv()

    server, thread, endpoint = _start_server(handler)
    try:
        session = ElevenLabsRealtimeClient(
            api_key="write-only-test-key",
            endpoint=endpoint,
            finalization_timeout_seconds=2,
        ).start("eng")
        session.submit_audio(bytes(3_200))
        with pytest.raises(RealtimeTranscriptionError, match="rate limited") as error_info:
            session.finish()
        assert "arbitrary provider payload" not in str(error_info.value)
    finally:
        _stop_server(server, thread)


class BlockingConnection:
    """Block the sender deterministically while allowing cancellation to release both workers."""

    def __init__(self) -> None:
        """Create release events for one sender and one receiver."""
        self.send_started = threading.Event()
        self.closed = threading.Event()
        self.release_send = threading.Event()
        self.received: queue.Queue[str] = queue.Queue()

    def send(self, _message: str) -> None:
        """Hold the first send until the test fills the bounded client queue."""
        self.send_started.set()
        self.release_send.wait(timeout=2)

    def recv(self, timeout: float | None = None) -> str:
        """Block the only receive worker until close releases it."""
        if not self.closed.wait(timeout=timeout):
            return self.received.get(timeout=2)
        raise OSError("synthetic close")

    def close(self) -> None:
        """Release every synthetic blocking operation."""
        self.closed.set()
        self.release_send.set()


class CommitTrackingConnection:
    """Acknowledge every manual commit while recording ordered outbound events."""

    def __init__(self) -> None:
        """Create one synchronized inbound event queue."""
        self.events: list[dict[str, object]] = []
        self.inbound: queue.Queue[str | None] = queue.Queue()

    def send(self, message: str) -> None:
        """Record one event and acknowledge only explicit commit messages."""
        event = json.loads(message)
        self.events.append(event)
        if event.get("commit") is True:
            commit_number = sum(item.get("commit") is True for item in self.events)
            self.inbound.put(
                json.dumps(
                    {
                        "message_type": "committed_transcript",
                        "text": f"segment {commit_number}",
                    }
                )
            )

    def recv(self, timeout: float | None = None) -> str:
        """Return one synthetic provider event or terminate after close."""
        message = self.inbound.get(timeout=timeout)
        if message is None:
            raise OSError("synthetic close")
        return message

    def close(self) -> None:
        """Release the receive worker."""
        self.inbound.put(None)


class CloseAfterCommitConnection(CommitTrackingConnection):
    """Close immediately after acknowledging the final provider commit."""

    def __init__(self) -> None:
        """Expose when the receive worker has observed the provider close."""
        super().__init__()
        self.peer_close_observed = threading.Event()

    def send(self, message: str) -> None:
        """Acknowledge a commit, then close the provider side without another frame."""
        super().send(message)
        if json.loads(message).get("commit") is True:
            self.inbound.put(None)

    def recv(self, timeout: float | None = None) -> str:
        """Record the exact point at which the provider-side close is consumed."""
        try:
            return super().recv(timeout=timeout)
        except OSError:
            self.peer_close_observed.set()
            raise


def test_audio_queue_fails_to_batch_route_without_blocking_capture() -> None:
    """Reject overload immediately instead of blocking the thread that drains PipeWire."""
    connection = BlockingConnection()
    session = RealtimeTranscriptionSession(
        connection=connection,
        configured_language_code="eng",
        session_identifier="bounded-session",
        maximum_queued_chunks=1,
    )

    assert session.submit_audio(bytes(3_200))
    assert connection.send_started.wait(timeout=2)
    assert session.submit_audio(bytes(3_200))
    assert not session.submit_audio(bytes(3_200))
    assert not session.is_healthy
    session.cancel()


def test_long_capture_commits_every_twenty_five_seconds_before_final_commit() -> None:
    """Prevent provider auto-commit ambiguity while preserving every stable segment in order."""
    connection = CommitTrackingConnection()
    session = RealtimeTranscriptionSession(
        connection=connection,
        configured_language_code="eng",
        session_identifier="long-session",
        maximum_queued_chunks=512,
        finalization_timeout_seconds=2,
    )

    for _index in range(251):
        assert session.submit_audio(bytes(3_200))
    result = session.finish()

    commits = [event for event in connection.events if event.get("commit") is True]
    assert len(commits) == 2
    assert result.transcription.text == "segment 1 segment 2"
    assert result.transcription.audio_duration_seconds == pytest.approx(25.1)


def test_provider_close_after_final_commit_is_not_a_route_failure(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Accept a normal peer close after the final committed text was dispatched."""
    connection = CloseAfterCommitConnection()
    session = RealtimeTranscriptionSession(
        connection=connection,
        configured_language_code="eng",
        session_identifier="closing-session",
        finalization_timeout_seconds=2,
    )
    wait_until = session._wait_until

    def wait_until_peer_close(event: threading.Event, deadline: float) -> bool:
        """Force the receive-close branch to finish before Stop reads route state."""
        completed = wait_until(event, deadline)
        if event is session._final_commit_received:
            assert connection.peer_close_observed.wait(timeout=2)
        return completed

    monkeypatch.setattr(session, "_wait_until", wait_until_peer_close)
    assert session.submit_audio(bytes(3_200))

    result = session.finish()

    assert result.transcription.text == "segment 1"
    assert session.is_healthy


def test_cancel_closes_without_sending_a_commit() -> None:
    """Do not stabilize or deliver volatile text after an explicit user cancellation."""
    messages: list[dict[str, object]] = []

    def handler(connection: ServerConnection) -> None:
        connection.send(json.dumps({"message_type": "session_started", "session_id": "cancelled-session"}))
        for message in connection:
            messages.append(json.loads(message))

    server, thread, endpoint = _start_server(handler)
    try:
        session = ElevenLabsRealtimeClient(api_key="write-only-test-key", endpoint=endpoint).start("eng")
        session.submit_audio(bytes(3_200))
        session.cancel()
        assert all(event.get("commit") is not True for event in messages)
        assert session.snapshot().volatile_text == ""
    finally:
        _stop_server(server, thread)


def _start_server(handler: Callable[[ServerConnection], None]) -> tuple[Server, threading.Thread, str]:
    """Start one ephemeral synchronous WebSocket server for a focused contract test."""
    server = serve(handler, "127.0.0.1", 0)
    thread = threading.Thread(target=server.serve_forever, name="fake-scribe-realtime", daemon=True)
    thread.start()
    port = server.socket.getsockname()[1]
    return server, thread, f"ws://127.0.0.1:{port}/v1/speech-to-text/realtime"


def _stop_server(server: Server, thread: threading.Thread) -> None:
    """Stop one fake server and prove its serving thread exits."""
    server.shutdown()
    thread.join(timeout=2)
    assert not thread.is_alive()
