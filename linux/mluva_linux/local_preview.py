"""Chunked local previews with one on-demand worker per recording."""

from mluva_linux.batch_preview import BatchPreviewSession
from mluva_linux.local_asr import local_client
from mluva_linux.realtime import RealtimePreview


class LocalPreviewClient:
    """Keep local speech previews independent of optional text rewriting."""

    def __init__(self, model, directory, chunk_seconds=3, device="cpu"):
        """Keep configuration only; no weights or process are loaded at launch."""
        self.model, self.directory, self.chunk_seconds = model, directory, chunk_seconds
        self.device = device

    def start(self, language_code, on_preview=None, on_committed_segment=None):
        """Create one disposable inference session for this capture."""
        return LocalPreviewSession(self.model, self.directory, language_code, self.chunk_seconds, self.device)


class LocalPreviewSession(BatchPreviewSession):
    """Release model memory on Stop, Cancel and errors, keeping preview text provisional."""

    def __init__(self, model, directory, language, chunk_seconds, device="cpu"):
        """Reuse a child between preview chunks, starting it only when audio arrives."""
        self.model = model
        self.device = device
        self.local_client = None
        self.streaming_text = ""
        super().__init__(self._client, directory, language, chunk_seconds)

    def _client(self):
        if self.local_client is None or self.local_client.cancelled.is_set():
            self.local_client = local_client(self.model, keep_alive=True, device=self.device)
            if self.model == "qwen3-1.7b":
                self.local_client.on_partial = self._partial
        return self.local_client

    def _partial(self, text):
        with self.lock:
            if not self.finishing and not self.cancelled.is_set():
                self.streaming_text = (self.text + " " + text).strip()

    def snapshot(self):
        """Publish incremental decoder text as provisional, never final delivery."""
        with self.lock:
            return RealtimePreview(self.streaming_text or self.text, "")

    def set_preview_enabled(self, enabled):
        """Speech previews remain enabled even when text rewriting is switched off."""
        super().set_preview_enabled(True)

    def finish(self):
        """Reconcile the final recording and then unload all weights."""
        try:
            return super().finish()
        finally:
            if self.local_client:
                self.local_client.close()

    def cancel(self):
        """Stop the resident recording worker even between inference calls."""
        super().cancel()
        with self.lock:
            self.streaming_text = ""
        if self.local_client:
            self.local_client.cancel()

    def _run(self):
        try:
            super()._run()
        finally:
            if not self.finishing and self.local_client:
                self.local_client.close()
