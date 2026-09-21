"""Crash-cleaned, memory-backed staging for Incognito recordings."""

import atexit
import os
import selectors
import shutil
import subprocess
import sys
import tempfile
import threading
from pathlib import Path

_CLEANUP_SCRIPT = r"""
import shutil, sys
sys.stdout.write('ready\n')
sys.stdout.flush()
sys.stdin.buffer.read()
shutil.rmtree(sys.argv[1], ignore_errors=True)
"""


def _memory_backed(path: Path) -> bool:
    """Check the actual mount rather than trusting a runtime-directory environment variable."""
    selected: tuple[int, str] = (0, "")
    for line in Path("/proc/self/mountinfo").read_text(encoding="utf-8").splitlines():
        fields, filesystem = line.split(" - ", 1)
        mount = Path(fields.split()[4].replace("\\040", " ").replace("\\134", "\\"))
        if path.is_relative_to(mount) and len(str(mount)) > selected[0]:
            selected = (len(str(mount)), filesystem.split()[0])
    return selected[1] in {"tmpfs", "ramfs"}


class VolatileAudioStore:
    """Give an independent cleanup process ownership of one private volatile directory."""

    def __init__(self) -> None:
        """Fail before recording if memory-backed storage or crash cleanup is unavailable."""
        root = Path("/dev/shm").resolve(strict=True)
        if not _memory_backed(root):
            raise OSError("Incognito needs memory-backed /dev/shm; recording was not started.")
        self.path = Path(tempfile.mkdtemp(prefix=f"mluva-audio-{os.getuid()}-", dir=root))
        self.process = None
        try:
            self.process = subprocess.Popen(
                [sys.executable, "-I", "-c", _CLEANUP_SCRIPT, str(self.path)],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL,
                env={},
                start_new_session=True,
            )
            with selectors.DefaultSelector() as selector:
                selector.register(self.process.stdout, selectors.EVENT_READ)
                if not selector.select(timeout=5) or self.process.stdout.readline() != b"ready\n":
                    raise OSError("Incognito crash cleanup could not start.")
            self.process.stdout.close()
        except Exception:
            self.close()
            raise

    def close(self) -> None:
        """Close the lifetime pipe; EOF also occurs automatically if Mluva is killed."""
        if self.process is not None:
            if self.process.stdin is not None:
                self.process.stdin.close()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()
            if self.process.stdout is not None:
                self.process.stdout.close()
        shutil.rmtree(self.path, ignore_errors=True)


_store: VolatileAudioStore | None = None
_lock = threading.Lock()


def volatile_audio_directory() -> Path:
    """Share one staging area per app process without any durable fallback."""
    global _store
    with _lock:
        if _store is None:
            _store = VolatileAudioStore()
            atexit.register(_store.close)
        if _store.process.poll() is not None:
            raise OSError("Incognito crash cleanup stopped; restart Mluva before recording.")
        return _store.path
