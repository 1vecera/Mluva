"""Chunked previews for batch speech providers, with authoritative full-audio finalization."""

import tempfile
import threading
import time
import wave
from collections.abc import Callable
from pathlib import Path

from voice_scribe_linux.elevenlabs import TranscriptionResult
from voice_scribe_linux.realtime import RealtimePreview, RealtimeSessionResult
from voice_scribe_linux.workflow import TranscriptionClient

BYTES_PER_SECOND = 32_000
MAX_PREVIEW_BYTES = 30 * 60 * BYTES_PER_SECOND


class BatchPreviewClient:
    """Reuse one file transcription contract for local and cloud preview chunks."""

    def __init__(self, factory: Callable[[], TranscriptionClient], directory: Path, chunk_seconds: int) -> None:
        """Keep temporary audio in the app's private runtime directory."""
        self.factory = factory
        self.directory = directory
        self.chunk_seconds = chunk_seconds

    def start(self, language_code: str, on_preview=None, on_committed_segment=None):
        """Start a bounded preview worker without opening a device or network connection."""
        return BatchPreviewSession(self.factory, self.directory, language_code, self.chunk_seconds)


class BatchPreviewSession:
    """Never block microphone writes on inference or reuse preview chunks as final recognition."""

    def __init__(
        self,
        factory: Callable[[], TranscriptionClient],
        directory: Path,
        language: str,
        chunk_seconds: int,
    ) -> None:
        """Own all preview state for one capture and isolate each provider request."""
        self.factory = factory
        self.directory = directory
        self.language = language
        self.chunk_bytes = chunk_seconds * BYTES_PER_SECOND
        self.lock = threading.Lock()
        self.ready = threading.Event()
        self.cancelled = threading.Event()
        self.audio = bytearray()
        self.offset = 0
        self.text = ""
        self.healthy = True
        self.finishing = False
        self.active_clients = []
        self.worker = threading.Thread(target=self._run, name="speech-preview", daemon=True)
        self.worker.start()

    @property
    def bytes_sent(self) -> int:
        """Report audio accepted for provider work for existing cancellation disclosure."""
        return self.offset

    @property
    def is_healthy(self) -> bool:
        """Report whether previews remain available; final batch recognition can still recover."""
        return self.healthy and not self.cancelled.is_set()

    def snapshot(self) -> RealtimePreview:
        """Expose completed chunk text as provisional until full-audio finalization."""
        with self.lock:
            return RealtimePreview(self.text, "")

    def submit_audio(self, frames: bytes) -> None:
        """Accept bounded PCM without queuing concurrent inference calls."""
        with self.lock:
            if not self.is_healthy or self.finishing:
                return
            if len(self.audio) + len(frames) > MAX_PREVIEW_BYTES:
                self.healthy = False
                self.audio.clear()
                self.ready.set()
                return
            self.audio.extend(frames)
            if len(self.audio) - self.offset >= self.chunk_bytes:
                self.ready.set()

    def _transcribe(self, frames: bytes) -> TranscriptionResult:
        """Erase each owner-only temporary WAV even when a backend fails or capture is cancelled."""
        self.directory.mkdir(parents=True, mode=0o700, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="speech-", dir=self.directory) as temporary:
            path = Path(temporary) / "audio.wav"
            with wave.open(str(path), "wb") as writer:
                writer.setnchannels(1)
                writer.setsampwidth(2)
                writer.setframerate(16_000)
                writer.writeframes(frames)
            path.chmod(0o600)
            client = self.factory()
            with self.lock:
                if self.cancelled.is_set():
                    raise RuntimeError("Transcription cancelled.")
                self.active_clients.append(client)
            try:
                return client.transcribe(path, self.language, "")
            finally:
                with self.lock:
                    self.active_clients.remove(client)

    def _run(self) -> None:
        """Transcribe sequential chunks and discard output after stop or cancellation."""
        while True:
            self.ready.wait()
            self.ready.clear()
            with self.lock:
                if not self.is_healthy or self.finishing:
                    return
                if len(self.audio) - self.offset < self.chunk_bytes:
                    continue
                frames = bytes(self.audio[self.offset : self.offset + self.chunk_bytes])
                self.offset += len(frames)
            try:
                result = self._transcribe(frames)
            except Exception:
                if not self.finishing:
                    self.healthy = False
                return
            with self.lock:
                if self.cancelled.is_set() or self.finishing:
                    return
                self.text = (self.text + " " + result.text).strip()
                if len(self.audio) - self.offset >= self.chunk_bytes:
                    self.ready.set()

    def finish(self) -> RealtimeSessionResult:
        """Recognize the complete audio once so words at preview boundaries are reconciled."""
        started = time.monotonic()
        with self.lock:
            self.finishing = True
            self.ready.set()
            frames = bytes(self.audio)
            self.audio.clear()
            preview_clients = list(self.active_clients)
        for client in preview_clients:
            if hasattr(client, "cancel"):
                client.cancel()
        self.worker.join(timeout=2)
        if not self.is_healthy or not frames:
            raise RuntimeError("Preview unavailable; use the finalized recording.")
        result = self._transcribe(frames)
        if self.cancelled.is_set():
            raise RuntimeError("Transcription cancelled.")
        return RealtimeSessionResult(result, time.monotonic() - started)

    def cancel(self) -> None:
        """Drop queued audio and invalidate all late previews without blocking GTK."""
        self.cancelled.set()
        with self.lock:
            self.audio.clear()
            self.text = ""
            clients = list(self.active_clients)
        for client in clients:
            if hasattr(client, "cancel"):
                threading.Thread(target=client.cancel, name="cancel-speech", daemon=True).start()
        self.ready.set()
