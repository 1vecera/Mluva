"""Wire-level contract coverage for ElevenLabs Scribe v2."""

import io
import json
import threading
import urllib.error
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

import pytest

import voice_scribe_linux.elevenlabs as elevenlabs_module
from voice_scribe_linux.elevenlabs import ElevenLabsClient, TranscriptionError


class ScribeHandler(BaseHTTPRequestHandler):
    """Inspect one real HTTP upload and return a representative Scribe response."""

    request_body = b""
    request_api_key = ""

    def do_POST(self) -> None:
        """Capture the multipart request before returning transcript metadata."""
        type(self).request_body = self.rfile.read(int(self.headers["Content-Length"]))
        type(self).request_api_key = self.headers["xi-api-key"]
        response = json.dumps(
            {
                "text": "Hello Linux.",
                "language_code": "eng",
                "language_probability": 0.99,
                "transcription_id": "scribe-test",
                "words": [],
            }
        ).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(response)))
        self.end_headers()
        self.wfile.write(response)

    def log_message(self, _format: str, *_args: object) -> None:
        """Keep successful contract-test HTTP traffic out of test output."""


class FailedScribeHandler(BaseHTTPRequestHandler):
    """Return provider text that must not cross Mluva's error boundary."""

    def do_POST(self) -> None:
        """Simulate a rejected credential with a sensitive diagnostic body."""
        response = b'{"detail":"sensitive provider diagnostic"}'
        self.send_response(401)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(response)))
        self.end_headers()
        self.wfile.write(response)

    def log_message(self, _format: str, *_args: object) -> None:
        """Keep expected contract-test HTTP traffic out of test output."""


class MeetingScribeHandler(BaseHTTPRequestHandler):
    """Return diarized word timing for the explicit Meeting request path."""

    request_body = b""

    def do_POST(self) -> None:
        """Capture the meeting request and return two contiguous speakers."""
        type(self).request_body = self.rfile.read(int(self.headers["Content-Length"]))
        response = json.dumps(
            {
                "text": "First point. I will send it.",
                "language_code": "eng",
                "language_probability": 0.98,
                "transcription_id": "meeting-test",
                "audio_duration_secs": 3.25,
                "words": [
                    {
                        "text": "First",
                        "start": 0.1,
                        "end": 0.4,
                        "type": "word",
                        "speaker_id": "speaker_0",
                    },
                    {
                        "text": " point.",
                        "start": 0.4,
                        "end": 0.9,
                        "type": "word",
                        "speaker_id": "speaker_0",
                    },
                    {
                        "text": " ",
                        "start": 0.9,
                        "end": 1.0,
                        "type": "spacing",
                    },
                    {
                        "text": "I",
                        "start": 1.1,
                        "end": 1.2,
                        "type": "word",
                        "speaker_id": "speaker_1",
                    },
                    {
                        "text": " will send it.",
                        "start": 1.2,
                        "end": 2.4,
                        "type": "word",
                        "speaker_id": "speaker_1",
                    },
                    {
                        "text": "[door slams]",
                        "start": 2.5,
                        "end": 2.8,
                        "type": "audio_event",
                        "speaker_id": "speaker_1",
                    },
                ],
            }
        ).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(response)))
        self.end_headers()
        self.wfile.write(response)

    def log_message(self, _format: str, *_args: object) -> None:
        """Keep successful contract-test HTTP traffic out of test output."""


class InvalidScribeHandler(BaseHTTPRequestHandler):
    """Return malformed success content that must become a controlled error."""

    def do_POST(self) -> None:
        """Consume the request and return non-JSON provider content."""
        self.rfile.read(int(self.headers["Content-Length"]))
        response = b"sensitive malformed provider response"
        self.send_response(200)
        self.send_header("Content-Type", "text/plain")
        self.send_header("Content-Length", str(len(response)))
        self.end_headers()
        self.wfile.write(response)

    def log_message(self, _format: str, *_args: object) -> None:
        """Keep expected malformed-response traffic out of test output."""


def test_transcribe_posts_scribe_v2_multipart(tmp_path: Path) -> None:
    """Prove model, language, audio, and credential placement on the HTTP wire."""
    audio_path = tmp_path / "capture.wav"
    audio_path.write_bytes(b"RIFF-test-audio")
    server = ThreadingHTTPServer(("127.0.0.1", 0), ScribeHandler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        endpoint = f"http://127.0.0.1:{server.server_port}/v1/speech-to-text"
        client = ElevenLabsClient(api_key="write-only-test-key", endpoint=endpoint)
        assert "write-only-test-key" not in repr(client)
        result = client.transcribe(audio_path, "eng")
    finally:
        server.shutdown()
        server.server_close()
        thread.join()
    assert result.text == "Hello Linux."
    assert result.transcription_id == "scribe-test"
    assert ScribeHandler.request_api_key == "write-only-test-key"
    assert b'name="model_id"\r\n\r\nscribe_v2' in ScribeHandler.request_body
    assert b'name="language_code"\r\n\r\neng' in ScribeHandler.request_body
    assert b'name="diarize"\r\n\r\nfalse' in ScribeHandler.request_body
    assert b'name="file"; filename="capture.wav"' in ScribeHandler.request_body
    assert b"RIFF-test-audio" in ScribeHandler.request_body


def test_transcribe_meeting_requests_and_groups_diarized_words(tmp_path: Path) -> None:
    """Request Scribe diarization only for Meeting and preserve speaker timing."""
    audio_path = tmp_path / "meeting.wav"
    audio_path.write_bytes(b"RIFF-test-meeting-audio")
    server = ThreadingHTTPServer(("127.0.0.1", 0), MeetingScribeHandler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        endpoint = f"http://127.0.0.1:{server.server_port}/v1/speech-to-text"
        result = ElevenLabsClient(api_key="write-only-test-key", endpoint=endpoint).transcribe_meeting(
            audio_path,
            "eng",
        )
    finally:
        server.shutdown()
        server.server_close()
        thread.join()

    assert result.text == "First point. I will send it."
    assert result.audio_duration_seconds == 3.25
    assert len(result.speaker_segments) == 2
    assert result.speaker_segments[0].speaker == "Speaker 1"
    assert result.speaker_segments[0].text == "First point."
    assert result.speaker_segments[0].started_at_seconds == 0.1
    assert result.speaker_segments[0].ended_at_seconds == 1.0
    assert result.speaker_segments[1].speaker == "Speaker 2"
    assert result.speaker_segments[1].text == "I will send it."
    assert result.speaker_segments[1].started_at_seconds == 1.1
    assert result.speaker_segments[1].ended_at_seconds == 2.4
    assert b'name="diarize"\r\n\r\ntrue' in MeetingScribeHandler.request_body
    assert b'name="timestamps_granularity"\r\n\r\nword' in MeetingScribeHandler.request_body


def test_transcribe_omits_language_for_documented_auto_detection(tmp_path: Path) -> None:
    """Leave the optional language field absent when the user selects Auto-detect."""
    audio_path = tmp_path / "automatic.wav"
    audio_path.write_bytes(b"RIFF-test-audio")
    server = ThreadingHTTPServer(("127.0.0.1", 0), ScribeHandler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        endpoint = f"http://127.0.0.1:{server.server_port}/v1/speech-to-text"
        result = ElevenLabsClient(api_key="write-only-test-key", endpoint=endpoint).transcribe(
            audio_path,
            "auto",
        )
    finally:
        server.shutdown()
        server.server_close()
        thread.join()

    assert result.language_code == "eng"
    assert b'name="language_code"' not in ScribeHandler.request_body


def test_transcribe_does_not_expose_provider_error_body(tmp_path: Path) -> None:
    """Keep provider diagnostics behind the application's write-only secret boundary."""
    audio_path = tmp_path / "capture.wav"
    audio_path.write_bytes(b"RIFF-test-audio")
    server = ThreadingHTTPServer(("127.0.0.1", 0), FailedScribeHandler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        endpoint = f"http://127.0.0.1:{server.server_port}/v1/speech-to-text"
        with pytest.raises(TranscriptionError, match="HTTP 401") as error_info:
            ElevenLabsClient(api_key="write-only-test-key", endpoint=endpoint).transcribe(audio_path, "eng")
    finally:
        server.shutdown()
        server.server_close()
        thread.join()
    assert "sensitive provider diagnostic" not in str(error_info.value)


def test_transcribe_closes_http_error_response(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """Release rejected provider bodies and sockets before returning a controlled error."""
    audio_path = tmp_path / "capture.wav"
    audio_path.write_bytes(b"RIFF-test-audio")
    response_body = io.BytesIO(b'{"detail":"sensitive provider diagnostic"}')
    provider_error = urllib.error.HTTPError(
        url="https://provider.invalid/v1/speech-to-text",
        code=401,
        msg="Unauthorized",
        hdrs={},
        fp=response_body,
    )

    def reject_request(_request: object, timeout: float) -> object:
        """Return the same observable rejected response for this one request."""
        assert timeout == 300
        raise provider_error

    monkeypatch.setattr(elevenlabs_module.urllib.request, "urlopen", reject_request)

    with pytest.raises(TranscriptionError, match="HTTP 401"):
        ElevenLabsClient(api_key="write-only-test-key").transcribe(audio_path, "eng")

    assert response_body.closed


def test_transcribe_sanitizes_malformed_success_response(tmp_path: Path) -> None:
    """Convert invalid success content without disclosing its provider body."""
    audio_path = tmp_path / "capture.wav"
    audio_path.write_bytes(b"RIFF-test-audio")
    server = ThreadingHTTPServer(("127.0.0.1", 0), InvalidScribeHandler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        endpoint = f"http://127.0.0.1:{server.server_port}/v1/speech-to-text"
        with pytest.raises(TranscriptionError, match="invalid transcription response") as error_info:
            ElevenLabsClient(api_key="write-only-test-key", endpoint=endpoint).transcribe(audio_path, "eng")
    finally:
        server.shutdown()
        server.server_close()
        thread.join()
    assert "sensitive malformed provider response" not in str(error_info.value)
