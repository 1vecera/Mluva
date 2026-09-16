"""Chunked local previews with one on-demand worker per recording."""

from mluva_linux.batch_preview import BatchPreviewSession
from mluva_linux.local_asr import LocalSpeechClient


class LocalPreviewClient:
    """Keep local speech previews independent of optional text rewriting."""

    def __init__(self, model, directory, chunk_seconds=3):
        """Keep configuration only; no weights or process are loaded at launch."""
        self.model, self.directory, self.chunk_seconds = model, directory, chunk_seconds

    def start(self, language_code, on_preview=None, on_committed_segment=None):
        """Create one disposable inference session for this capture."""
        return LocalPreviewSession(self.model, self.directory, language_code, self.chunk_seconds)


class LocalPreviewSession(BatchPreviewSession):
    """Release model memory on Stop, Cancel and errors, keeping preview text provisional."""

    def __init__(self, model, directory, language, chunk_seconds):
        """Reuse a child between preview chunks, starting it only when audio arrives."""
        self.model = model
        self.local_client = None
        super().__init__(self._client, directory, language, chunk_seconds)

    def _client(self):
        if self.local_client is None or self.local_client.cancelled.is_set():
            self.local_client = LocalSpeechClient(self.model, keep_alive=True)
        return self.local_client

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
        if self.local_client:
            self.local_client.cancel()

    def _run(self):
        try:
            super()._run()
        finally:
            if not self.finishing and self.local_client:
                self.local_client.close()
