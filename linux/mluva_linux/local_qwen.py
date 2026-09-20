"""Pinned, app-owned llama.cpp runtime for Qwen speech, downloaded only on request."""

import hashlib
import json
import os
import platform
import shutil
import tarfile
import urllib.request
from pathlib import Path

from mluva_linux.config import default_data_dir
from mluva_linux.local_gpu import disk_usage
from mluva_linux.local_gpu import runtime_path as onnx_runtime

MANIFEST = json.loads(Path(__file__).with_name("qwen_runtime.json").read_text())
RUNTIME_BUDGET = 250_000_000


def runtime_root():
    """Keep both CPU and GPU runtimes inside Mluva's storage budget."""
    return default_data_dir(os.environ) / "qwen-runtime"


def binary(device):
    """Resolve only the downloaded executable, never another application's runtime."""
    return runtime_root() / device / ("llama-" + MANIFEST["version"]) / "llama-server"


def ready(device):
    """Require a verified artifact and an executable before enabling Continue."""
    try:
        return (runtime_root() / device / ".ready").read_text() == MANIFEST["assets"][device]["sha256"] and os.access(
            binary(device), os.X_OK
        )
    except OSError:
        return False


def install(device, cancelled):
    """Verify the pinned release and extract only contained, bounded archive paths."""
    if ready(device):
        return
    if platform.machine() != "x86_64":
        raise RuntimeError("This Qwen runtime requires Linux x86-64. Choose another local model on this computer.")
    root = runtime_root()
    root.mkdir(parents=True, mode=0o700, exist_ok=True)
    used = (
        disk_usage(root)
        + disk_usage(onnx_runtime())
        + disk_usage(root.parent / "models")
        + disk_usage(root.parent / "qwen-cache")
    )
    if used + RUNTIME_BUDGET > 5_000_000_000 or shutil.disk_usage(root).free < RUNTIME_BUDGET:
        raise RuntimeError("Not enough local model storage. Remove unused downloads before continuing.")
    asset = MANIFEST["assets"][device]
    stage = root / (device + ".partial")
    shutil.rmtree(stage, ignore_errors=True)
    stage.mkdir(mode=0o700)
    archive = stage / "runtime.tar.gz"
    try:
        digest = hashlib.sha256()
        size = 0
        with urllib.request.urlopen(asset["url"], timeout=30) as source, archive.open("wb") as target:
            while block := source.read(1024 * 1024):
                if cancelled.is_set():
                    raise RuntimeError("Download cancelled.")
                size += len(block)
                if size > asset["size"]:
                    raise RuntimeError("Runtime exceeds its declared size.")
                digest.update(block)
                target.write(block)
        if size != asset["size"] or digest.hexdigest() != asset["sha256"]:
            raise RuntimeError("Runtime verification failed. Download again.")
        with tarfile.open(archive) as source:
            if sum(member.size for member in source.getmembers()) > RUNTIME_BUDGET:
                raise RuntimeError("Runtime exceeds its storage budget.")
            source.extractall(stage, filter="data")
        archive.unlink()
        if cancelled.is_set():
            raise RuntimeError("Download cancelled.")
        (stage / ".ready").write_text(asset["sha256"])
        shutil.rmtree(root / device, ignore_errors=True)
        stage.rename(root / device)
    except Exception:
        shutil.rmtree(stage, ignore_errors=True)
        raise
