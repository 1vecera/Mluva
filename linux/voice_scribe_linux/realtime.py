"""Bounded ElevenLabs Scribe v2 Realtime transport for volatile Linux capture previews."""

from __future__ import annotations

import base64
import json
import queue
import threading
import time
from collections.abc import Callable
from dataclasses import dataclass, field
from typing import Protocol
from urllib.parse import parse_qsl, urlencode, urlsplit, urlunsplit

from websockets.sync.client import connect as websocket_connect

from voice_scribe_linux.audio import SAMPLE_RATE, SAMPLE_WIDTH_BYTES
from voice_scribe_linux.brand import LINUX_USER_AGENT
from voice_scribe_linux.elevenlabs import TranscriptionError, TranscriptionResult

SCRIBE_REALTIME_ENDPOINT = "wss://api.elevenlabs.io/v1/speech-to-text/realtime"
SCRIBE_REALTIME_MODEL = "scribe_v2_realtime"
DEFAULT_AUDIO_QUEUE_CHUNKS = 64
DEFAULT_SESSION_TIMEOUT_SECONDS = 10.0
DEFAULT_FINALIZATION_TIMEOUT_SECONDS = 8.0
MAX_EVENT_BYTES = 1_048_576
MANUAL_COMMIT_INTERVAL_BYTES = 25 * SAMPLE_RATE * SAMPLE_WIDTH_BYTES

PreviewCallback = Callable[["RealtimePreview"], None]
CommittedSegmentCallback = Callable[["RealtimeCommittedSegment"], None]


class WebSocketConnection(Protocol):
    """Describe the small synchronous WebSocket surface used by a realtime session."""

    def send(self, message: str) -> None:
        """Send one text message."""
        ...

    def recv(self, timeout: float | None = None) -> str | bytes:
        """Receive one complete message."""
        ...

    def close(self) -> None:
        """Close the connection and unblock active receives."""
        ...


class RealtimeTranscriptionError(TranscriptionError):
    """Report a controlled realtime failure that is safe to surface or batch-fallback."""


@dataclass(frozen=True, slots=True)
class RealtimePreview:
    """Separate committed text from the volatile segment that must never be delivered."""

    committed_text: str
    volatile_text: str

    @property
    def display_text(self) -> str:
        """Join stable and volatile segments for display only."""
        return _join_transcript_segments((self.committed_text, self.volatile_text))


@dataclass(frozen=True, slots=True)
class RealtimeSessionResult:
    """Return a committed transcript and stop-to-commit provider latency."""

    transcription: TranscriptionResult
    finalization_seconds: float


@dataclass(frozen=True, slots=True)
class RealtimeCommittedSegment:
    """Identify one immutable committed provider segment in capture order."""

    identifier: str
    sequence: int
    text: str


@dataclass(frozen=True, slots=True)
class ElevenLabsRealtimeClient:
    """Open authenticated Scribe v2 Realtime sessions without persisting the API key."""

    api_key: str = field(repr=False)
    endpoint: str = SCRIBE_REALTIME_ENDPOINT
    session_timeout_seconds: float = DEFAULT_SESSION_TIMEOUT_SECONDS
    finalization_timeout_seconds: float = DEFAULT_FINALIZATION_TIMEOUT_SECONDS
    maximum_queued_chunks: int = DEFAULT_AUDIO_QUEUE_CHUNKS

    def __post_init__(self) -> None:
        """Reject unsafe or unusable transport configuration before a connection starts."""
        if not self.api_key:
            raise ValueError("An ElevenLabs API key is required.")
        endpoint = urlsplit(self.endpoint)
        if endpoint.scheme not in {"ws", "wss"} or not endpoint.netloc:
            raise ValueError("The ElevenLabs realtime endpoint must be a ws:// or wss:// URL.")
        if endpoint.username is not None or endpoint.password is not None:
            raise ValueError("The ElevenLabs realtime endpoint cannot contain credentials.")
        if self.session_timeout_seconds <= 0 or self.finalization_timeout_seconds <= 0:
            raise ValueError("Realtime timeouts must be positive.")
        if self.maximum_queued_chunks <= 0:
            raise ValueError("The realtime audio queue must contain at least one chunk.")

    def start(
        self,
        language_code: str,
        on_preview: PreviewCallback | None = None,
        on_committed_segment: CommittedSegmentCallback | None = None,
    ) -> RealtimeTranscriptionSession:
        """Open a ready session before microphone capture is allowed to begin."""
        uri = _realtime_uri(self.endpoint, language_code)
        try:
            connection = websocket_connect(
                uri,
                additional_headers={
                    "xi-api-key": self.api_key,
                    "User-Agent": LINUX_USER_AGENT,
                },
                open_timeout=self.session_timeout_seconds,
                close_timeout=2,
                max_size=MAX_EVENT_BYTES,
                max_queue=16,
            )
        except Exception as error:
            raise RealtimeTranscriptionError(
                "ElevenLabs realtime transcription could not reach the service."
            ) from error
        try:
            event = _decode_event(connection.recv(timeout=self.session_timeout_seconds))
            if event.get("message_type") != "session_started":
                event_error = _event_error(event)
                raise RealtimeTranscriptionError(
                    event_error or "ElevenLabs did not confirm that realtime transcription was ready."
                )
            session_identifier = event.get("session_id")
            if not isinstance(session_identifier, str) or not session_identifier:
                raise RealtimeTranscriptionError("ElevenLabs returned an invalid realtime session response.")
        except RealtimeTranscriptionError:
            connection.close()
            raise
        except Exception as error:
            connection.close()
            raise RealtimeTranscriptionError(
                "ElevenLabs did not confirm that realtime transcription was ready."
            ) from error
        return RealtimeTranscriptionSession(
            connection=connection,
            configured_language_code=language_code,
            session_identifier=session_identifier,
            on_preview=on_preview,
            on_committed_segment=on_committed_segment,
            maximum_queued_chunks=self.maximum_queued_chunks,
            finalization_timeout_seconds=self.finalization_timeout_seconds,
        )


class RealtimeTranscriptionSession:
    """Send PCM on one worker, receive events on one worker, and deliver committed text only."""

    def __init__(
        self,
        connection: WebSocketConnection,
        configured_language_code: str,
        session_identifier: str,
        on_preview: PreviewCallback | None = None,
        on_committed_segment: CommittedSegmentCallback | None = None,
        maximum_queued_chunks: int = DEFAULT_AUDIO_QUEUE_CHUNKS,
        finalization_timeout_seconds: float = DEFAULT_FINALIZATION_TIMEOUT_SECONDS,
    ) -> None:
        """Start isolated sender and receiver workers around one confirmed connection."""
        if maximum_queued_chunks <= 0:
            raise ValueError("The realtime audio queue must contain at least one chunk.")
        if finalization_timeout_seconds <= 0:
            raise ValueError("The realtime finalization timeout must be positive.")
        self._connection = connection
        self._configured_language_code = configured_language_code
        self._session_identifier = session_identifier
        self._on_preview = on_preview
        self._on_committed_segment = on_committed_segment
        self._finalization_timeout_seconds = finalization_timeout_seconds
        self._audio_queue: queue.Queue[bytes | None] = queue.Queue(maxsize=maximum_queued_chunks)
        self._lock = threading.Lock()
        self._committed_segments: list[str] = []
        self._volatile_text = ""
        self._detected_language_code: str | None = None
        self._failure_message: str | None = None
        self._finishing = False
        self._cancelled = False
        self._closing = False
        self._bytes_sent = 0
        self._bytes_since_commit = 0
        self._commits_sent = 0
        self._commits_dispatched = 0
        self._final_commit_target: int | None = None
        self._sender_done = threading.Event()
        self._receiver_done = threading.Event()
        self._final_commit_received = threading.Event()
        self._sender_thread = threading.Thread(
            target=self._send_events,
            name="scribe-realtime-send",
            daemon=True,
        )
        self._receiver_thread = threading.Thread(
            target=self._receive_events,
            name="scribe-realtime-receive",
            daemon=True,
        )
        self._sender_thread.start()
        self._receiver_thread.start()

    @property
    def bytes_sent(self) -> int:
        """Return how much audio has actually crossed the WebSocket boundary."""
        with self._lock:
            return self._bytes_sent

    @property
    def is_healthy(self) -> bool:
        """Return whether the live route is still eligible to produce a final result."""
        with self._lock:
            return self._failure_message is None and not self._cancelled

    def snapshot(self) -> RealtimePreview:
        """Return a thread-safe display projection with volatile text explicitly separated."""
        with self._lock:
            return RealtimePreview(
                committed_text=_join_transcript_segments(tuple(self._committed_segments)),
                volatile_text=self._volatile_text,
            )

    def submit_audio(self, pcm_frames: bytes) -> bool:
        """Queue one non-empty PCM16 chunk without ever blocking the PipeWire drain."""
        if not pcm_frames:
            return True
        if len(pcm_frames) % SAMPLE_WIDTH_BYTES:
            raise ValueError("Realtime audio chunks must contain complete PCM16 samples.")
        with self._lock:
            if self._finishing or self._cancelled or self._failure_message is not None:
                return False
        try:
            self._audio_queue.put_nowait(bytes(pcm_frames))
        except queue.Full:
            self._fail("Realtime transcription could not keep up with microphone audio.")
            return False
        return True

    def finish(self) -> RealtimeSessionResult:
        """Flush queued audio, request a commit, and return only provider-committed text."""
        finalization_started_at = time.monotonic()
        with self._lock:
            if self._cancelled:
                raise RealtimeTranscriptionError("Realtime transcription was cancelled.")
            if self._finishing:
                raise RealtimeTranscriptionError("Realtime transcription is already finalizing.")
            self._finishing = True
            failure_message = self._failure_message
        if failure_message is not None:
            self.cancel()
            raise RealtimeTranscriptionError(failure_message)
        deadline = finalization_started_at + self._finalization_timeout_seconds
        try:
            self._audio_queue.put(None, timeout=self._remaining(deadline))
        except queue.Full:
            self._fail("Realtime transcription did not accept the final audio chunk.")
        if not self._wait_until(self._sender_done, deadline):
            self._fail("Realtime transcription did not finish sending microphone audio.")
        if self._current_failure() is None and not self._wait_until(self._final_commit_received, deadline):
            self._fail("ElevenLabs did not return a committed realtime transcript in time.")
        failure_message = self._current_failure()
        if failure_message is not None:
            self.cancel()
            raise RealtimeTranscriptionError(failure_message)
        with self._lock:
            text = _join_transcript_segments(tuple(self._committed_segments))
            language_code = self._detected_language_code or (
                self._configured_language_code if self._configured_language_code != "auto" else "und"
            )
            audio_duration_seconds = self._bytes_sent / (SAMPLE_RATE * SAMPLE_WIDTH_BYTES)
        if not text:
            self.cancel()
            raise RealtimeTranscriptionError("ElevenLabs returned an empty committed realtime transcript.")
        result = RealtimeSessionResult(
            transcription=TranscriptionResult(
                text=text,
                language_code=language_code,
                language_probability=None,
                transcription_id=self._session_identifier,
                audio_duration_seconds=audio_duration_seconds,
            ),
            finalization_seconds=time.monotonic() - finalization_started_at,
        )
        self._close()
        return result

    def cancel(self) -> None:
        """Close without a commit and discard every volatile transcript fragment."""
        with self._lock:
            self._cancelled = True
            self._finishing = True
            self._volatile_text = ""
            self._on_preview = None
            self._on_committed_segment = None
        while True:
            try:
                self._audio_queue.get_nowait()
            except queue.Empty:
                break
        try:
            self._audio_queue.put_nowait(None)
        except queue.Full:
            pass
        self._close()

    def _send_events(self) -> None:
        """Serialize queued chunks so final commit cannot overtake captured audio."""
        try:
            while True:
                chunk = self._audio_queue.get()
                if chunk is None:
                    with self._lock:
                        if self._cancelled:
                            return
                        if self._bytes_since_commit == 0 and self._commits_sent > 0:
                            self._final_commit_target = self._commits_sent
                            if self._commits_dispatched >= self._final_commit_target:
                                self._final_commit_received.set()
                            return
                        self._commits_sent += 1
                        self._final_commit_target = self._commits_sent
                    self._connection.send(_audio_event(b"", commit=True))
                    return
                with self._lock:
                    if self._cancelled:
                        return
                self._connection.send(_audio_event(chunk))
                with self._lock:
                    self._bytes_sent += len(chunk)
                    self._bytes_since_commit += len(chunk)
                    should_commit = self._bytes_since_commit >= MANUAL_COMMIT_INTERVAL_BYTES
                    if should_commit:
                        self._bytes_since_commit -= MANUAL_COMMIT_INTERVAL_BYTES
                        self._commits_sent += 1
                if should_commit:
                    self._connection.send(_audio_event(b"", commit=True))
        except Exception:
            self._fail("The ElevenLabs realtime connection closed while sending audio.")
        finally:
            self._sender_done.set()

    def _receive_events(self) -> None:
        """Own the connection's only receive loop and validate every content-bearing event."""
        try:
            while True:
                event = _decode_event(self._connection.recv())
                self._handle_event(event)
                with self._lock:
                    if self._cancelled or self._closing:
                        return
        except Exception:
            with self._lock:
                expected_close = self._cancelled or self._closing or self._final_commit_received.is_set()
            if not expected_close:
                self._fail("The ElevenLabs realtime connection closed before transcription completed.")
        finally:
            self._receiver_done.set()

    def _handle_event(self, event: dict[str, object]) -> None:
        """Apply one provider event while keeping interim text outside final delivery state."""
        message_type = event.get("message_type")
        if not isinstance(message_type, str):
            self._fail("ElevenLabs returned an invalid realtime transcription event.")
            return
        event_error = _event_error(event)
        if event_error is not None:
            self._fail(event_error)
            return
        if message_type in {"partial_transcript", "final_transcript"}:
            text = event.get("text")
            if not isinstance(text, str):
                self._fail("ElevenLabs returned an invalid realtime transcription event.")
                return
            with self._lock:
                self._volatile_text = text.strip()
            self._emit_preview()
            return
        if message_type == "committed_transcript":
            text = event.get("text")
            if not isinstance(text, str):
                self._fail("ElevenLabs returned an invalid realtime transcription event.")
                return
            committed_text = text.strip()
            committed_segment = None
            with self._lock:
                if committed_text:
                    sequence = len(self._committed_segments)
                    self._committed_segments.append(committed_text)
                    committed_segment = RealtimeCommittedSegment(
                        identifier=f"{self._session_identifier}-{sequence}",
                        sequence=sequence,
                        text=committed_text,
                    )
                self._volatile_text = ""
            if committed_segment is not None:
                self._emit_committed_segment(committed_segment)
            with self._lock:
                self._commits_dispatched += 1
                final_commit_target = self._final_commit_target
                if final_commit_target is not None and self._commits_dispatched >= final_commit_target:
                    self._final_commit_received.set()
            self._emit_preview()
            return
        if message_type in {
            "final_transcript_with_timestamps",
            "committed_transcript_with_timestamps",
        }:
            language_code = event.get("language_code")
            if isinstance(language_code, str) and language_code:
                with self._lock:
                    self._detected_language_code = language_code

    def _emit_preview(self) -> None:
        """Notify a display-only consumer without allowing it to break recognition."""
        with self._lock:
            callback = self._on_preview
            cancelled = self._cancelled
        if callback is None or cancelled:
            return
        try:
            callback(self.snapshot())
        except Exception:
            with self._lock:
                self._on_preview = None

    def _emit_committed_segment(self, segment: RealtimeCommittedSegment) -> None:
        """Offer immutable committed text to optional cleanup without risking recognition."""
        with self._lock:
            callback = self._on_committed_segment
            cancelled = self._cancelled
        if callback is None or cancelled:
            return
        try:
            callback(segment)
        except Exception:
            with self._lock:
                self._on_committed_segment = None

    def _fail(self, message: str) -> None:
        """Preserve the first controlled route failure for deterministic batch fallback."""
        with self._lock:
            if self._failure_message is None and not self._cancelled:
                self._failure_message = message

    def _current_failure(self) -> str | None:
        """Read the first controlled failure across sender and receiver workers."""
        with self._lock:
            return self._failure_message

    def _close(self) -> None:
        """Close the socket once and join workers within the configured bound."""
        with self._lock:
            if self._closing:
                return
            self._closing = True
        try:
            self._connection.close()
        except Exception:
            pass
        current_thread = threading.current_thread()
        for worker in (self._sender_thread, self._receiver_thread):
            if worker is not current_thread:
                worker.join(timeout=2)

    @staticmethod
    def _remaining(deadline: float) -> float:
        """Return a nonnegative timeout for one bounded finalization phase."""
        return max(0.0, deadline - time.monotonic())

    @classmethod
    def _wait_until(cls, event: threading.Event, deadline: float) -> bool:
        """Wait for an event without extending the shared finalization deadline."""
        return event.wait(timeout=cls._remaining(deadline))


def _realtime_uri(endpoint: str, language_code: str) -> str:
    """Add the reviewed Scribe v2 Realtime query without altering endpoint authority."""
    parsed = urlsplit(endpoint)
    query = dict(parse_qsl(parsed.query, keep_blank_values=True))
    query.update(
        {
            "model_id": SCRIBE_REALTIME_MODEL,
            "audio_format": f"pcm_{SAMPLE_RATE}",
            "commit_strategy": "manual",
        }
    )
    if language_code == "auto":
        query.pop("language_code", None)
        query["include_language_detection"] = "true"
    else:
        query["language_code"] = language_code
        query.pop("include_language_detection", None)
    return urlunsplit((parsed.scheme, parsed.netloc, parsed.path, urlencode(query), parsed.fragment))


def _audio_event(pcm_frames: bytes, commit: bool = False) -> str:
    """Encode one raw PCM chunk using the exact realtime event contract."""
    payload: dict[str, object] = {
        "message_type": "input_audio_chunk",
        "audio_base_64": base64.b64encode(pcm_frames).decode("ascii"),
    }
    if commit:
        payload["commit"] = True
    return json.dumps(payload, separators=(",", ":"))


def _decode_event(message: str | bytes) -> dict[str, object]:
    """Validate one bounded JSON object without exposing provider payloads in errors."""
    try:
        event = json.loads(message)
    except (json.JSONDecodeError, UnicodeDecodeError) as error:
        raise RealtimeTranscriptionError("ElevenLabs returned an invalid realtime transcription event.") from error
    if not isinstance(event, dict):
        raise RealtimeTranscriptionError("ElevenLabs returned an invalid realtime transcription event.")
    return event


def _event_error(event: dict[str, object]) -> str | None:
    """Map provider failures to controlled messages that cannot echo content or credentials."""
    message_type = event.get("message_type")
    if not isinstance(message_type, str):
        return None
    if message_type in {"auth_error", "scribe_auth_error"}:
        return "ElevenLabs rejected realtime transcription authentication."
    if message_type in {"rate_limited", "quota_exceeded"}:
        return "ElevenLabs realtime transcription is rate limited."
    if message_type.endswith("_error") or isinstance(event.get("error"), str):
        return "ElevenLabs realtime transcription failed."
    return None


def _join_transcript_segments(segments: tuple[str, ...]) -> str:
    """Join provider segments without allowing boundary whitespace to accumulate."""
    return " ".join(segment.strip() for segment in segments if segment.strip())
