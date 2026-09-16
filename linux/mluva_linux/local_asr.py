"""Cancellable local inference workers without any cloud fallback or resident daemon."""

import json
import os
import selectors
import subprocess
import sys
import threading
import time
from pathlib import Path

from mluva_linux import local_gpu
from mluva_linux.elevenlabs import TranscriptionResult
from mluva_linux.local_models import model_path, ready


class LocalSpeechClient:
    """Own one child process, reusing weights only inside an active capture."""

    def __init__(self, model: str, *, keep_alive: bool = False, device: str = "cpu") -> None:
        """Defer all weight loading until the first audio request."""
        self.model = model
        self.device = device
        self.keep_alive = keep_alive
        self.process = None
        self.cancelled = threading.Event()

    def transcribe(self, file_path: Path, language_code: str, model_id: str = "") -> TranscriptionResult:
        """Run only verified local weights; reject timeout, cancellation and malformed output."""
        from mluva_linux.providers import LANGUAGES

        language = LANGUAGES.get(language_code, language_code)
        if self.model == "parakeet-v3" and language not in {
            "auto",
            "bg",
            "hr",
            "cs",
            "da",
            "nl",
            "en",
            "et",
            "fi",
            "fr",
            "de",
            "el",
            "hu",
            "it",
            "lv",
            "lt",
            "mt",
            "pl",
            "pt",
            "ro",
            "sk",
            "sl",
            "es",
            "sv",
            "ru",
            "uk",
        }:
            raise RuntimeError("Parakeet does not support this language. Choose a multilingual Whisper model.")
        if self.device == "cuda" and not local_gpu.ready():
            raise RuntimeError("Download GPU support in Settings → Providers first, or choose CPU.")
        if not ready(self.model):
            raise RuntimeError("Download your local model in Settings → Providers first.")
        if self.cancelled.is_set():
            raise RuntimeError("Local transcription cancelled.")
        try:
            if self.process is None:
                environ = {key: os.environ[key] for key in ("PATH", "LANG", "SYSTEMROOT") if key in os.environ}
                environ.update(
                    PYTHONPATH=str(Path(__file__).resolve().parent.parent),
                    HF_HUB_OFFLINE="1",
                    TRANSFORMERS_OFFLINE="1",
                    OMP_NUM_THREADS="1",
                    OPENBLAS_NUM_THREADS="1",
                )
                self.process = subprocess.Popen(
                    [
                        str(local_gpu.runtime_path() / "bin/python") if self.device == "cuda" else sys.executable,
                        "-m",
                        "mluva_linux.local_asr_worker",
                        self.model,
                        str(model_path(self.model)),
                        self.device,
                    ],
                    stdin=subprocess.PIPE,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.DEVNULL,
                    text=True,
                    env=environ,
                )
            process = self.process
            if self.cancelled.is_set():
                raise RuntimeError("Local transcription cancelled.")
            process.stdin.write(
                json.dumps({"path": str(file_path.resolve()), "language": LANGUAGES.get(language_code, language_code)})
                + "\n"
            )
            process.stdin.flush()
            with selectors.DefaultSelector() as selector:
                selector.register(process.stdout, selectors.EVENT_READ)
                deadline = time.monotonic() + 180
                while True:
                    # CUDA reserves large virtual ranges, so bound resident RAM rather than address space.
                    try:
                        status = Path(f"/proc/{process.pid}/status").read_text()
                        rss = next(
                            int(line.split()[1]) * 1024 for line in status.splitlines() if line.startswith("VmRSS:")
                        )
                        if rss > 5_000_000_000:
                            raise RuntimeError("Local model exceeded 5 GB RAM. Choose a smaller model.")
                    except (OSError, StopIteration):
                        pass
                    if selector.select(timeout=0.1):
                        break
                    if self.cancelled.is_set():
                        raise RuntimeError("Local transcription cancelled.")
                    if time.monotonic() >= deadline:
                        raise RuntimeError("Local transcription timed out. Try a smaller model.")
            result = json.loads(process.stdout.readline(2_000_001))
            if self.cancelled.is_set() or not isinstance(result.get("text"), str):
                raise ValueError
            return TranscriptionResult(result["text"], language_code, None, None)
        except RuntimeError:
            self.close()
            raise
        except (OSError, ValueError):
            self.close()
            message = (
                "GPU transcription failed. Try CPU or reinstall GPU support."
                if self.device == "cuda"
                else ("Local transcription failed. Try a smaller model or download it again.")
            )
            raise RuntimeError(message) from None
        finally:
            if not self.keep_alive:
                self.close()

    def close(self) -> None:
        """Release all model memory by stopping only this client's worker."""
        process, self.process = self.process, None
        if process is not None:
            if process.poll() is None:
                process.kill()
            process.communicate()

    def cancel(self) -> None:
        """Invalidate late output and interrupt inference promptly."""
        self.cancelled.set()
        self.close()
