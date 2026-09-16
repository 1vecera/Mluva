"""Pinned, app-owned speech models; downloading never uploads audio or credentials."""

import fcntl
import hashlib
import json
import os
import shutil
import threading
import urllib.request
from pathlib import Path

from mluva_linux.config import default_data_dir

MODELS = tuple(json.loads(Path(__file__).with_name("local_models.json").read_text()))
MODEL_BY_ID = {model["id"]: model for model in MODELS}
STORAGE_LIMIT = 5_000_000_000


def model_root() -> Path:
    """Use Mluva's own model directory, independent of any other application."""
    return default_data_dir(os.environ) / "models"


def model_path(identifier: str) -> Path:
    """Resolve only a catalog identifier, never an arbitrary path or repository."""
    model = MODEL_BY_ID[identifier]
    return model_root() / (identifier + "-" + model["revision"][:12])


def ready(identifier: str) -> bool:
    """Require a completed verification marker and every expected file size."""
    model = MODEL_BY_ID[identifier]
    path = model_path(identifier)
    try:
        return (path / ".ready").read_text() == model["revision"] and all(
            (path / item["name"]).stat().st_size == item["size"] for item in model["files"]
        )
    except OSError:
        return False


def download(identifier: str, progress, cancelled: threading.Event, *, gpu: bool = False) -> None:
    """Download pinned files atomically, verify weight hashes and bound total storage."""
    model = MODEL_BY_ID[identifier]
    root = model_root()
    root.mkdir(parents=True, mode=0o700, exist_ok=True)
    with (root / ".download.lock").open("w") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise RuntimeError("Another model download is running. Try again when it finishes.") from None
        from mluva_linux import local_gpu

        if gpu:
            local_gpu.install(cancelled)
        if ready(identifier):
            progress(1.0)
            return
        path = model_path(identifier)
        total = sum(item["size"] for item in model["files"])
        used = local_gpu.disk_usage(root) + local_gpu.disk_usage(local_gpu.runtime_path())
        if used + total > STORAGE_LIMIT or shutil.disk_usage(root).free < total + 100_000_000:
            raise RuntimeError("Not enough model storage. Free disk space before downloading.")
        path.mkdir(mode=0o700, exist_ok=True)
        completed = 0
        try:
            for item in model["files"]:
                destination = path / item["name"]
                destination.parent.mkdir(parents=True, exist_ok=True)
                temporary = destination.with_suffix(destination.suffix + ".part")
                digest = hashlib.sha256()
                received = 0
                url = f"https://huggingface.co/{model['repo']}/resolve/{model['revision']}/{item['name']}"
                with urllib.request.urlopen(url, timeout=30) as response, temporary.open("wb") as target:
                    while block := response.read(1024 * 1024):
                        if cancelled.is_set():
                            raise RuntimeError("Download cancelled.")
                        received += len(block)
                        if received > item["size"]:
                            raise RuntimeError("The model download exceeded its expected size.")
                        digest.update(block)
                        target.write(block)
                        progress((completed + received) / total)
                if received != item["size"] or (item["sha256"] and digest.hexdigest() != item["sha256"]):
                    raise RuntimeError("Model verification failed. Please retry the download.")
                temporary.replace(destination)
                completed += received
            if cancelled.is_set():
                raise RuntimeError("Download cancelled.")
            (path / ".ready").write_text(model["revision"])
        except Exception:
            shutil.rmtree(path)
            raise
