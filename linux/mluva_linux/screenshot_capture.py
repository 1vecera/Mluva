"""Invoke Omarchy's existing region picker without replacing the clipboard or sharing a destination."""

import os
import subprocess
import tempfile
import threading
from pathlib import Path

from mluva_linux.screenshots import MAX_IMAGE_BYTES, validate_png


class ScreenshotCapture:
    """Own exactly one cancellable picker process and its temporary screenshot directory."""

    def __init__(self, runtime_directory: Path) -> None:
        """Keep selected pixels in a private temporary directory until the application attaches them."""
        self.runtime_directory = runtime_directory
        self.cancelled = threading.Event()
        self.attached = threading.Event()
        self.process: subprocess.Popen | None = None
        self.lock = threading.Lock()

    def run(self) -> bytes | None:
        """Return a completed selection or cancellation while leaving the normal clipboard intact."""
        self.runtime_directory.mkdir(mode=0o700, parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="screenshot-", dir=self.runtime_directory) as directory:
            environment = dict(os.environ, OMARCHY_SCREENSHOT_DIR=directory)
            with self.lock:
                if self.cancelled.is_set():
                    return None
                self.process = subprocess.Popen(
                    ["omarchy", "screenshot", "region", "save"],
                    env=environment,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.DEVNULL,
                    start_new_session=True,
                )
            try:
                stdout, _stderr = self.process.communicate(timeout=180)
            except subprocess.TimeoutExpired:
                self.cancel()
                self.process.communicate()
                raise RuntimeError("Screenshot selection timed out. Press F10 to try again.") from None
            if self.cancelled.is_set() or self.process.returncode != 0 or not stdout.strip():
                return None
            if len(stdout) > 4096:
                raise ValueError("Screenshot picker returned an invalid file.")
            path = Path(os.fsdecode(stdout.strip()))
            if path.is_symlink() or path.resolve().parent != Path(directory).resolve():
                raise ValueError("Screenshot picker returned a file outside its capture directory.")
            with path.open("rb") as source:
                data = source.read(MAX_IMAGE_BYTES + 1)
            validate_png(data)
            return data

    def cancel(self) -> None:
        """Stop only this picker and its region-selection children after its owner was cancelled."""
        import signal

        self.cancelled.set()
        self.attached.set()
        with self.lock:
            if self.process is not None and self.process.poll() is None:
                try:
                    os.killpg(self.process.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
