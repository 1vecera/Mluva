"""Provider protocol, editable document and live scheduling regression coverage."""

import json
import sqlite3
import threading
from dataclasses import replace
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from types import SimpleNamespace
from unittest.mock import patch

import pytest

from voice_scribe_linux.batch_preview import BatchPreviewClient
from voice_scribe_linux.config import AppConfig, load_config, save_config
from voice_scribe_linux.conversation import ConversationStore, rewrite_prompt
from voice_scribe_linux.elevenlabs import TranscriptionResult
from voice_scribe_linux.history import HistoryStore
from voice_scribe_linux.live_rewrite import LiveRewriteSchedule, initial_draft, live_prompt
from voice_scribe_linux.providers import LiteLLMClient, ProviderError, VoxtypeClient
from voice_scribe_linux.workflow import DictationWorkflow


@pytest.fixture
def service():
    """Use an independent loopback HTTP service for real JSON, SSE and multipart transport."""
    requests = []
    options = {"finish": "stop", "text": "Hotový zápis.", "done": True, "status": 200}

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_args):
            """Keep fixture requests and credentials out of test logs."""

        def do_GET(self):
            """Serve the deployment catalog without reading conversation data."""
            requests.append((self.path, self.headers, b""))
            self.send_response(200)
            self.end_headers()
            self.wfile.write(json.dumps({"data": [{"id": "local-editor"}]}).encode())

        def do_POST(self):
            """Accept both APIs and send configurable terminal/error behavior."""
            body = self.rfile.read(int(self.headers["Content-Length"]))
            requests.append((self.path, self.headers, body))
            self.send_response(options["status"])
            self.end_headers()
            if self.path.endswith("/audio/transcriptions"):
                self.wfile.write(json.dumps({"text": options["text"]}).encode())
                return
            events = [{"choices": [{"index": 0, "delta": {"content": options["text"]}, "finish_reason": None}]}]
            if options["done"]:
                events.append({"choices": [{"index": 0, "delta": {}, "finish_reason": options["finish"]}]})
            for event in events:
                self.wfile.write(("data: " + json.dumps(event) + "\r\n\r\n").encode())
            self.wfile.write(b"data: [DONE]\n\n")

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    yield f"http://127.0.0.1:{server.server_port}/v1", requests, options
    server.shutdown()
    server.server_close()
    thread.join(timeout=2)


def test_remote_catalog_stream_and_audio(service, tmp_path, monkeypatch):
    """Exercise model selection, Unicode SSE and multipart language mapping over real HTTP."""
    url, requests, _options = service
    monkeypatch.setenv("MLUVA_TEST_KEY", "fixture-key")
    client = LiteLLMClient(url, "MLUVA_TEST_KEY", "local-editor")
    assert client.list_models()[0].identifier == "local-editor"
    deltas = []
    assert client.transform("Edit this", tmp_path, on_delta=deltas.append) == "Hotový zápis."
    assert deltas == ["Hotový zápis."]
    body = json.loads(requests[-1][2])
    assert body == {"model": "local-editor", "messages": [{"role": "user", "content": "Edit this"}], "stream": True}
    wav = tmp_path / "speech.wav"
    wav.write_bytes(b"RIFF-fixture-wave")
    assert client.transcribe(wav, "ces").text == "Hotový zápis."
    assert requests[-1][0] == "/v1/audio/transcriptions"
    assert b'name="language"\r\n\r\ncs' in requests[-1][2]
    assert b"RIFF-fixture-wave" in requests[-1][2]
    assert requests[-1][1]["Authorization"] == "Bearer fixture-key"


@pytest.mark.parametrize(
    "change", [{"finish": "length"}, {"finish": "tool_calls"}, {"done": False}, {"text": "x" * 101}]
)
def test_incomplete_or_oversized_rewrite_never_becomes_a_result(service, tmp_path, change):
    """Streaming text alone is insufficient evidence of a completed document."""
    url, _requests, options = service
    options.update(change)
    with pytest.raises(ProviderError):
        LiteLLMClient(url, "UNSET_FIXTURE_KEY", "local-editor").transform("Prompt", tmp_path, max_output_characters=100)


def test_provider_errors_and_cancel_do_not_expose_payload(service, tmp_path):
    """Expose a controlled failure and prohibit dispatch after cancellation."""
    url, requests, options = service
    options.update(status=401, text="private response body")
    client = LiteLLMClient(url, "UNSET_FIXTURE_KEY", "local-editor")
    with pytest.raises(ProviderError, match="HTTP 401") as error:
        client.transform("private prompt", tmp_path)
    assert "private" not in str(error.value)
    client.cancel()
    count = len(requests)
    with pytest.raises(ProviderError):
        client.transform("not sent", tmp_path)
    assert len(requests) == count


@pytest.mark.parametrize(
    "url", ["http://remote.example/v1", "https://user:secret@example.com", "https://example.com?token=x"]
)
def test_provider_urls_reject_credentials_and_cleartext_remote_hosts(url):
    """Keep secrets out of the dotfile and off unauthenticated remote HTTP links."""
    with pytest.raises(ValueError):
        AppConfig(litellm_base_url=url)


def test_local_voxtype_file_mode_ignores_cli_diagnostics(tmp_path):
    """Use the local engine, preserve Unicode and never invoke output or recording controls."""
    with patch("voice_scribe_linux.providers.subprocess.Popen") as run:
        output = 'Loading audio file: "fixture"\nAudio format: 16000 Hz\nProcessing samples...\n\nČistý text.\n'
        run.return_value.communicate.return_value = (output, None)
        run.return_value.returncode = 0
        assert VoxtypeClient("small").transcribe(tmp_path / "source.wav", "ces").text == "Čistý text."
    command = run.call_args.args[0]
    assert command == [
        "voxtype",
        "--quiet",
        "--engine",
        "whisper",
        "--whisper-mode",
        "local",
        "--language",
        "cs",
        "--model",
        "small",
        "transcribe",
        str(tmp_path / "source.wav"),
    ]


def test_saved_source_and_reply_edits_survive_restart_and_deletion(tmp_path):
    """Edited working documents drive rewriting without destroying raw recovery evidence."""
    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    store = ConversationStore(history)
    store.initialize()
    entry = history.add("Raw speech", "Raw speech", "dictation", "eng", None, "copied")
    store.save_text(entry.identifier, "Corrected source")
    reply = store.append(entry.identifier, "Polish", "Draft reply", "fixture")
    store.save_text(entry.identifier, "Edited reply", reply.identifier)
    restarted = ConversationStore(history)
    assert restarted.source_text(entry) == "Corrected source"
    assert history.find(entry.identifier).raw_text == "Raw speech"
    assert restarted.replies(entry.identifier)[0].text == "Edited reply"
    assert restarted.search("Corrected source")[0].identifier == entry.identifier
    prompt = rewrite_prompt(entry, restarted.replies(entry.identifier), "Shorten", restarted.source_text(entry))
    assert "Corrected source" in prompt and "Edited reply" in prompt
    history.delete(entry.identifier)
    assert restarted.replies(entry.identifier) == []
    with pytest.raises(sqlite3.IntegrityError):
        restarted.save_text(entry.identifier, "Must not resurrect")


def test_live_schedule_coalesces_and_marks_missing_information():
    """Test thresholds, in-flight coalescing, final tails and explicit template gaps."""
    schedule = LiveRewriteSchedule(40, 4)
    assert schedule.take("x", 0) is None
    assert schedule.take("x" * 40, 0) == "x" * 40
    assert schedule.take("x" * 90, 5) is None
    schedule.finish(True)
    assert schedule.take("x" * 90, 2) is None
    assert schedule.take("x" * 90, 5) == "x" * 90
    schedule.finish(True)
    assert schedule.take("x" * 95, 6, final=True) == "x" * 95
    config = AppConfig()
    assert "[Missing:" in initial_draft(config)
    assert "Never invent owners" in live_prompt(config, "Create a task", "My edited draft")
    schedule.finish(False)
    assert schedule.take("x" * 200, 30) is None


def test_batch_preview_finishes_from_full_audio_and_erases_temporary_files(tmp_path):
    """Provisional chunk text never substitutes for the final full-audio transcription."""
    seen = []
    chunk_done = threading.Event()

    class Speech:
        def transcribe(self, path, *_args):
            """Inspect private WAV payload lengths to distinguish preview and final requests."""
            seen.append(path.read_bytes())
            chunk_done.set()
            return TranscriptionResult("Final whole audio" if len(seen) > 1 else "Preview chunk", "eng", None, None)

    client = BatchPreviewClient(Speech, tmp_path, 3)
    session = client.start("eng")
    session.submit_audio(b"\0\0" * 48_000)
    assert chunk_done.wait(2)
    session.worker.join(timeout=0.05)
    session.submit_audio(b"\0\0" * 100)
    assert session.finish().transcription.text == "Final whole audio"
    session.cancel()
    session.worker.join(timeout=2)
    assert len(seen[1]) > len(seen[0]) and list(tmp_path.iterdir()) == []


def test_local_capture_without_cloud_credentials_or_clipboard(tmp_path):
    """Exercise provider metadata and disabled automatic copying through the complete workflow."""
    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    config = AppConfig(transcription_provider="voxtype", auto_copy_dictation=False)
    speech = SimpleNamespace(transcribe=lambda *args, **kwargs: TranscriptionResult("Hello", "eng", None, None))
    workflow = DictationWorkflow(config, speech, SimpleNamespace(), history, tmp_path)
    audio = tmp_path / "audio.wav"
    audio.write_bytes(b"fixture")
    with patch("voice_scribe_linux.workflow.deliver_text") as copy:
        result = workflow.complete(audio, "dictation", False, False)
    assert not copy.called and not result.delivery.copied
    assert result.history_entry.recognition_route == "voxtype-local"
    assert result.history_entry.delivery_outcome == "ready"
    path = tmp_path / "config.json"
    save_config(replace(config, live_rewrite_enabled=True), path)
    assert load_config(path).live_rewrite_enabled


@pytest.mark.parametrize("edited_source", [None, "My correction", "raw original"])
def test_widget_copy_matches_the_displayed_working_version(tmp_path, edited_source):
    """Keep cleaned dictation and deliberate source edits consistent between preview and Copy."""
    from gi.repository import GLib

    from voice_scribe_linux.app import MluvaApplication

    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    conversations = ConversationStore(history)
    conversations.initialize()
    entry = history.add("raw original", "Cleaned dictation", "dictation", "eng", None, "copied")
    if edited_source is not None:
        conversations.save_text(entry.identifier, edited_source)
    expected = edited_source if edited_source is not None else "Cleaned dictation"
    assert conversations.source_text(entry, delivered_fallback=True) == expected
    app = SimpleNamespace(
        shutting_down=False,
        config=AppConfig(),
        overlay_review_identifier=entry.identifier,
        history_store=history,
        conversation_store=conversations,
        rewrite_identifier=None,
        _publish_review=lambda *_args, **_kwargs: None,
    )
    with patch("voice_scribe_linux.app.deliver_text") as copy:
        MluvaApplication._review_action(app, None, GLib.Variant("(sss)", ("copy", entry.identifier, "")))
    copy.assert_called_once_with(expected, auto_paste=False)
