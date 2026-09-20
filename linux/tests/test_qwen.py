"""Qwen's streaming, privacy and owned-runtime failure boundaries."""

import hashlib
import io
import json
import tarfile
import threading
from unittest.mock import patch

import pytest

from mluva_linux import local_qwen
from mluva_linux.qwen_asr import QwenSpeechClient
from mluva_linux.speech_languages import language_label, supports_language


def events(*contents):
    """Include ordinary SSE control and usage messages around text deltas."""
    chunks = [{"choices": [{"delta": {"content": value}}]} for value in contents]
    chunks.append({"choices": [], "usage": {}})
    return io.BytesIO(b"".join(b"data: " + json.dumps(c).encode() + b"\n\n" for c in chunks) + b"data: [DONE]\n")


def test_qwen_stream_ignores_control_messages_and_hides_language_prefix():
    """Nullable deltas are valid; only recognized words reach the provisional preview."""
    client = QwenSpeechClient()
    client.url, client.token = "http://127.0.0.1:9999", "synthetic-token"
    partials = []
    client.on_partial = partials.append
    with patch.object(
        client.opener, "open", return_value=events(None, "language English", "<asr_text>Hello", " there")
    ) as request:
        assert client._recognize(b"synthetic-audio", "auto", "") == "Hello there"
    assert partials == ["Hello", "Hello there"]
    sent = request.call_args.args[0]
    assert sent.full_url == "http://127.0.0.1:9999/v1/chat/completions"
    assert sent.get_header("Authorization") == "Bearer synthetic-token"


def test_qwen_forced_language_continues_assistant_without_new_generation_prompt():
    """The upstream ASR template requires both flags to recognize a forced language."""
    client = QwenSpeechClient()
    client.url = "http://127.0.0.1:9999"
    with patch.object(client.opener, "open", return_value=events(None, "Hello")) as request:
        assert client._recognize(b"audio", "en", "") == "Hello"
    body = json.loads(request.call_args.args[0].data)
    assert body["continue_final_message"] and not body["add_generation_prompt"]
    assert body["messages"][-1]["content"] == "language English<asr_text>"


def test_qwen_missing_download_and_unsupported_language_never_start_process(tmp_path):
    """Opening Mluva or selecting unavailable models never downloads or loads weights."""
    with patch("subprocess.Popen") as spawn, patch("mluva_linux.qwen_asr.ready", return_value=False):
        client = QwenSpeechClient()
        assert client.process is None
        with pytest.raises(RuntimeError, match="Download Qwen"):
            client.transcribe(tmp_path / "absent.wav", "eng")
        with pytest.raises(RuntimeError, match="not supported"):
            client.transcribe(tmp_path / "absent.wav", "slk")
    spawn.assert_not_called()


def test_qwen_ignores_proxy_environment(monkeypatch):
    """An inherited HTTP proxy cannot receive local audio or the ephemeral credential."""
    monkeypatch.setenv("http_proxy", "http://untrusted.invalid:8080")
    client = QwenSpeechClient()
    assert not any(getattr(handler, "proxies", {}) for handler in client.opener.handlers)


@pytest.mark.parametrize("code", ["eng", "en", "auto"])
def test_language_picker_accepts_both_iso_forms(code):
    """Existing two-letter settings remain usable and correctly labelled."""
    assert supports_language("qwen3-1.7b", code)
    assert language_label(code) != code
    assert not supports_language("qwen3-1.7b", "slk")
    assert supports_language("parakeet-v3", "slk")


@pytest.mark.parametrize("failure", ["checksum", "traversal", "cancel"])
def test_qwen_runtime_failed_install_cleans_stage(tmp_path, monkeypatch, failure):
    """Corrupted, escaped and cancelled archives never become executable app runtimes."""
    data = io.BytesIO()
    with tarfile.open(fileobj=data, mode="w:gz") as archive:
        entry = tarfile.TarInfo("../escaped" if failure == "traversal" else "llama-test/llama-server")
        entry.size = 4
        archive.addfile(entry, io.BytesIO(b"test"))
    payload = data.getvalue()
    monkeypatch.setattr(local_qwen, "runtime_root", lambda: tmp_path / "qwen-runtime")
    monkeypatch.setattr(local_qwen, "onnx_runtime", lambda: tmp_path / "gpu-runtime")
    monkeypatch.setattr(
        local_qwen,
        "MANIFEST",
        {
            "version": "test",
            "assets": {
                "cpu": {
                    "url": "https://example.invalid/runtime",
                    "size": len(payload),
                    "sha256": "bad" if failure == "checksum" else hashlib.sha256(payload).hexdigest(),
                }
            },
        },
    )
    monkeypatch.setattr(local_qwen.urllib.request, "urlopen", lambda *a, **kw: io.BytesIO(payload))
    cancelled = threading.Event()
    if failure == "cancel":
        cancelled.set()
    with pytest.raises((RuntimeError, tarfile.TarError)):
        local_qwen.install("cpu", cancelled)
    assert not local_qwen.ready("cpu")
    assert not (tmp_path / "escaped").exists()
    assert not (tmp_path / "qwen-runtime/cpu.partial").exists()


def test_interrupted_qwen_stream_never_becomes_authoritative_text():
    """Partial decoder words remain provisional if the local server disappears."""
    client = QwenSpeechClient()
    client.url = "http://127.0.0.1:9999"
    stream = events("Hello").getvalue().replace(b"data: [DONE]\n", b"")
    with patch.object(client.opener, "open", return_value=io.BytesIO(stream)):
        with pytest.raises(RuntimeError, match="ended early"):
            client._recognize(b"audio", "en", "")


def test_simultaneous_cancel_waits_for_owned_process_cleanup():
    """The UI and preview worker may both cancel; neither should return before cleanup."""
    entered, release = threading.Event(), threading.Event()

    class Process:
        def poll(self):
            return None

        def terminate(self):
            pass

        def wait(self, timeout=None):
            entered.set()
            assert release.wait(2)

    client = QwenSpeechClient()
    client.process = Process()
    first = threading.Thread(target=client.cancel)
    second = threading.Thread(target=client.cancel)
    first.start()
    assert entered.wait(1)
    second.start()
    second.join(timeout=0.03)
    assert second.is_alive(), "The second caller must wait for the first cleanup"
    release.set()
    first.join(timeout=1)
    second.join(timeout=1)
    assert not first.is_alive() and not second.is_alive()
    assert client.process is None
