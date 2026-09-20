"""Failure, privacy and lifecycle boundaries for self-contained local transcription."""

import hashlib
import io
import threading
from dataclasses import replace
from pathlib import Path
from unittest.mock import patch

import pytest

from mluva_linux import local_gpu, local_models
from mluva_linux.config import AppConfig
from mluva_linux.local_asr import LocalSpeechClient
from mluva_linux.rewriting import rewrite_client


@pytest.fixture
def model_fixture(tmp_path, monkeypatch):
    """Serve tiny synthetic weights through the production download/verification path."""
    payload = b"synthetic verified weights"
    model = dict(
        id="test",
        repo="fixture/model",
        revision="1234567890abcdef",
        files=[dict(name="weights.bin", size=len(payload), sha256=hashlib.sha256(payload).hexdigest())],
    )
    monkeypatch.setattr(local_models, "MODEL_BY_ID", {"test": model})
    monkeypatch.setattr(local_models, "model_root", lambda: tmp_path)
    monkeypatch.setattr(local_models.urllib.request, "urlopen", lambda *args, **kwargs: io.BytesIO(payload))
    return payload


def test_download_verified_before_ready_and_reused(model_fixture):
    """A completed download unlocks use; reopening never downloads it again."""
    assert not local_models.ready("test")
    local_models.download("test", lambda value: None, threading.Event())
    assert local_models.ready("test")
    with patch.object(local_models.urllib.request, "urlopen", side_effect=AssertionError("Unexpected request")):
        local_models.download("test", lambda value: None, threading.Event())


@pytest.mark.parametrize("failure", ["checksum", "cancel", "storage"])
def test_failed_download_cannot_unlock_model(model_fixture, monkeypatch, failure):
    """Corruption, cancellation and insufficient storage leave no ready or partial model."""
    cancelled = threading.Event()
    if failure == "checksum":
        monkeypatch.setattr(
            local_models.urllib.request, "urlopen", lambda *args, **kwargs: io.BytesIO(b"x" * len(model_fixture))
        )
    elif failure == "cancel":
        cancelled.set()
    else:
        monkeypatch.setattr(local_models, "STORAGE_LIMIT", 1)
    with pytest.raises(RuntimeError):
        local_models.download("test", lambda value: None, cancelled)
    assert not local_models.ready("test")
    assert not list(local_models.model_root().rglob("*.part"))


def test_missing_local_model_never_starts_inference_or_cloud(tmp_path):
    """Fail closed before starting a process when a selected model is absent."""
    with patch("mluva_linux.local_asr.ready", return_value=False), patch("subprocess.Popen") as spawn:
        with pytest.raises(RuntimeError, match="Download"):
            LocalSpeechClient("whisper-tiny").transcribe(tmp_path / "audio.wav", "eng")
    spawn.assert_not_called()


def test_skip_client_never_constructs_any_provider():
    """Even a programmatic rewrite path cannot bypass the user's Skip choice."""
    with (
        patch("mluva_linux.rewriting.CodexAppServerClient") as codex,
        patch("mluva_linux.rewriting.LiteLLMClient") as remote,
    ):
        client = rewrite_client(replace(AppConfig(), rewrite_provider="none"))
        assert client.list_models() == []
        with pytest.raises(RuntimeError, match="Choose a rewriting provider"):
            client.transform("private text", Path("."))
        client.close()
    codex.assert_not_called()
    remote.assert_not_called()


def test_keyring_save_never_places_secret_in_arguments():
    """Store only through stdin and leave the settings file out of the credential path."""
    from types import SimpleNamespace

    from mluva_linux.credentials import store_speech_key

    with patch("mluva_linux.credentials.subprocess.run", return_value=SimpleNamespace(returncode=0)) as run:
        store_speech_key("synthetic-test-key")
    assert "synthetic-test-key" not in str(run.call_args.args)
    assert run.call_args.kwargs["input"] == "synthetic-test-key"
    assert run.call_args.kwargs["capture_output"]


def test_unsupported_local_language_does_not_start_worker(tmp_path):
    """Do not send unsupported speech through a model that would guess the wrong language."""
    with patch("subprocess.Popen") as spawn:
        with pytest.raises(RuntimeError, match="does not support this language"):
            LocalSpeechClient("parakeet-v3").transcribe(tmp_path / "audio.wav", "jpn")
    spawn.assert_not_called()


def test_gpu_runtime_counts_towards_model_storage(model_fixture, monkeypatch, tmp_path):
    """A model cannot silently exceed the budget already occupied by CUDA libraries."""
    monkeypatch.setattr(local_models, "model_root", lambda: tmp_path / "models")
    gpu = tmp_path / "runtime"
    gpu.mkdir()
    (gpu / "library").write_bytes(b"x" * 100)
    monkeypatch.setattr(local_gpu, "runtime_path", lambda: gpu)
    monkeypatch.setattr(local_models, "STORAGE_LIMIT", 100)
    with pytest.raises(RuntimeError, match="storage"):
        local_models.download("test", lambda value: None, threading.Event())
    assert not local_models.ready("test")


def test_gpu_worker_requires_completed_runtime(tmp_path):
    """A selected GPU never silently runs on CPU or downloads on first audio."""
    with patch.object(local_gpu, "ready", return_value=False), patch("subprocess.Popen") as spawn:
        with pytest.raises(RuntimeError, match="GPU support"):
            LocalSpeechClient("parakeet-v3", device="cuda").transcribe(tmp_path / "voice.wav", "eng")
    spawn.assert_not_called()


def test_runtime_version_invalidates_readiness(tmp_path, monkeypatch):
    """A changed dependency lock must be installed before Continue can unlock."""
    monkeypatch.setattr(local_gpu, "runtime_path", lambda: tmp_path)
    (tmp_path / "bin").mkdir()
    (tmp_path / "bin/python").touch()
    (tmp_path / ".ready").write_text("old dependency lock")
    assert not local_gpu.ready()
    (tmp_path / ".ready").write_text(local_gpu.runtime_stamp())
    assert local_gpu.ready()


def test_gpu_install_failure_removes_partial_runtime(tmp_path, monkeypatch):
    """An installation failure leaves neither readiness nor a large partial download."""
    root = tmp_path / "gpu-runtime"
    monkeypatch.setattr(local_gpu, "runtime_path", lambda: root)
    monkeypatch.setattr(local_gpu, "gpu_name", lambda: "test NVIDIA")
    monkeypatch.setattr(local_gpu, "ready", lambda: False)

    class FailedProcess:
        returncode = 1

        def poll(self):
            staging = root.with_name("gpu-runtime.partial")
            staging.mkdir(exist_ok=True)
            (staging / "partial-library").write_text("partial")
            return self.returncode

    with patch.object(local_gpu.subprocess, "Popen", return_value=FailedProcess()):
        with pytest.raises(RuntimeError, match="installation failed"):
            local_gpu.install(threading.Event())
    assert not root.exists()
    assert not root.with_name("gpu-runtime.partial").exists()


def test_cpu_default_and_monotonic_slider_budgets():
    """Every slider stop increases both the displayed RAM budget and actual storage."""
    assert AppConfig().local_device == "cpu"
    models = local_models.MODELS
    assert len(models) == 5
    assert [m["ram_mb"] for m in models] == sorted(m["ram_mb"] for m in models)
    sizes = [sum(f["size"] for f in m["files"]) for m in models]
    assert sizes == sorted(sizes)


def test_explicit_saved_key_wins_over_inherited_environment(monkeypatch):
    """Pasting a new account key must not silently keep billing the launcher's account."""
    from mluva_linux.config import elevenlabs_api_key

    monkeypatch.setenv("ELEVENLABS_API_KEY", "synthetic-old-key")
    with patch("mluva_linux.credentials.stored_speech_key", return_value="synthetic-new-key"):
        assert elevenlabs_api_key() == "synthetic-new-key"
    with patch("mluva_linux.credentials.stored_speech_key", return_value=None):
        assert elevenlabs_api_key() == "synthetic-old-key"
