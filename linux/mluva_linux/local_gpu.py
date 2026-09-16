"""Optional, app-owned NVIDIA runtime; no dependency on another application's models."""

import functools
import hashlib
import os
import platform
import shutil
import signal
import subprocess
import sys
from pathlib import Path

from mluva_linux.config import default_data_dir

RUNTIME_BUDGET = 3_500_000_000
STORAGE_LIMIT = 5_000_000_000
REQUIREMENTS = Path(__file__).with_name("gpu-requirements.txt")


@functools.lru_cache(maxsize=1)
def gpu_name() -> str:
    """Detect an NVIDIA device without loading weights or creating a GPU context."""
    if platform.machine() != "x86_64" or not shutil.which("uv"):
        return ""
    try:
        result = subprocess.run(
            ["nvidia-smi", "--query-gpu=name", "--format=csv,noheader", "--id=0"],
            capture_output=True,
            text=True,
            timeout=2,
            check=True,
        )
        return result.stdout.strip().splitlines()[0]
    except (OSError, subprocess.SubprocessError, IndexError):
        return ""


def runtime_path() -> Path:
    """Keep the optional runtime alongside Mluva's own weights."""
    return default_data_dir(os.environ) / "gpu-runtime"


def runtime_stamp() -> str:
    """Invalidate a runtime when its pinned packages or Python ABI change."""
    return hashlib.sha256(REQUIREMENTS.read_bytes()).hexdigest() + f":{sys.version_info.major}.{sys.version_info.minor}"


def ready() -> bool:
    """Require completed installation, not a half-populated environment."""
    try:
        return (runtime_path() / ".ready").read_text() == runtime_stamp() and (runtime_path() / "bin/python").exists()
    except OSError:
        return False


def disk_usage(path: Path) -> int:
    """Count owned regular files without following Python's interpreter symlink."""
    return sum(p.stat().st_size for p in path.rglob("*") if p.is_file() and not p.is_symlink())


def install(cancelled) -> None:
    """Install hash-locked wheels only after explicit download, releasing partial files on failure."""
    if ready():
        return
    if not gpu_name():
        raise RuntimeError("No supported NVIDIA GPU found. Use CPU on this computer.")
    root = runtime_path()
    root.parent.mkdir(parents=True, exist_ok=True)
    # The caller holds the model download lock across runtime and weight downloads.
    if disk_usage(root.parent / "models") + RUNTIME_BUDGET > STORAGE_LIMIT:
        raise RuntimeError("GPU support needs 3.5 GB of the 5 GB local storage budget. Remove unused models first.")
    if shutil.disk_usage(root.parent).free < 6_000_000_000:
        raise RuntimeError("GPU installation needs 6 GB free temporarily to unpack its wheels.")
    shutil.rmtree(root, ignore_errors=True)
    staging = root.with_name("gpu-runtime.partial")
    shutil.rmtree(staging, ignore_errors=True)

    def run(arguments):
        process = subprocess.Popen(
            arguments, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True
        )
        try:
            while process.poll() is None:
                if cancelled.wait(0.1):
                    raise RuntimeError("Download cancelled.")
            if process.returncode:
                raise RuntimeError("GPU runtime installation failed. Check connectivity and retry, or use CPU.")
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGTERM)
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()

    try:
        run(["uv", "venv", "--python", sys.executable, str(staging)])
        run(
            [
                "uv",
                "pip",
                "sync",
                "--python",
                str(staging / "bin/python"),
                "--require-hashes",
                "--only-binary",
                ":all:",
                "--no-cache",
                str(REQUIREMENTS),
            ]
        )
        if cancelled.is_set():
            raise RuntimeError("Download cancelled.")
        if disk_usage(staging) > RUNTIME_BUDGET:
            raise RuntimeError("GPU runtime exceeds the storage budget. Use CPU.")
        (staging / ".ready").write_text(runtime_stamp())
        staging.rename(root)
    except Exception:
        shutil.rmtree(staging, ignore_errors=True)
        raise
