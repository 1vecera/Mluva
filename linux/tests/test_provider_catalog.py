"""Provider routing, discovery failures and secret-safe setup evidence."""

import json
import sys
from dataclasses import replace
from http.server import BaseHTTPRequestHandler

import pytest
from http_fixture import local_http_server

from voice_scribe_linux.config import AppConfig, load_config, save_config
from voice_scribe_linux.provider_catalog import CatalogRequest, VoxtypeCatalog, catalog_message, connection_hint
from voice_scribe_linux.providers import MAX_HTTP_BYTES, LiteLLMClient, ProviderError


@pytest.fixture
def catalog_server():
    """Exercise production HTTP parsing with an independent loopback service and no real account."""
    state = {"payload": {"data": [{"id": "deployment"}]}, "status": 200, "requests": []}

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_args):
            """Keep even synthetic key values out of logs."""

        def do_GET(self):
            """Return bounded or deliberately malformed model listings."""
            state["requests"].append((self.path, self.headers, self.headers.get("Content-Length")))
            self.send_response(state["status"])
            if state["status"] == 302:
                self.send_header("Location", "/redirect-target")
            self.end_headers()
            try:
                self.wfile.write(json.dumps(state["payload"]).encode())
            except (BrokenPipeError, ConnectionResetError):
                pass  # Bounded clients close deliberately oversized catalog responses early.

    with local_http_server(Handler) as server:
        yield f"http://127.0.0.1:{server.server_port}/v1", state


def test_models_are_content_free_deduplicated_and_capability_scoped(catalog_server, monkeypatch):
    """Keep deployment aliases intact; explicit task metadata excludes incompatible models."""
    url, state = catalog_server
    monkeypatch.setenv("CATALOG_TEST_KEY", "fixture-only-secret")
    state["payload"] = {
        "data": [
            {"id": "voice", "model_info": {"mode": "audio_transcription"}},
            {"id": "writer", "model_info": {"mode": "chat"}},
            {"id": "vector", "mode": "embedding"},
            {"id": "custom/unknown:alias"},
            {"id": "custom/unknown:alias"},
        ]
    }
    client = LiteLLMClient(url, "CATALOG_TEST_KEY", None)
    assert [m.identifier for m in client.list_models(capability="speech")] == ["voice", "custom/unknown:alias"]
    assert [m.identifier for m in client.list_models(capability="rewrite")] == ["writer", "custom/unknown:alias"]
    assert all(path == "/v1/models" and body_length is None for path, _headers, body_length in state["requests"])
    assert state["requests"][0][1]["Authorization"] == "Bearer fixture-only-secret"


@pytest.mark.parametrize(
    "payload",
    [
        {},
        {"data": {}},
        {"data": [None]},
        {"data": [{"id": "bad\ncontrol"}]},
        {"data": [{"id": ""}]},
        {"data": [{"id": "a" * 201}]},
        {"data": [{"id": "model", "model_info": "private response"}]},
        {"data": [{"id": "model", "mode": []}]},
        {"data": [{"id": "deployment"}] * 2001},
        {"data": [{"id": "deployment"}], "padding": "x" * MAX_HTTP_BYTES},
    ],
)
def test_invalid_catalog_is_bounded_and_not_exposed(catalog_server, payload):
    """A provider-controlled response must never become raw UI error text or an unbounded catalog."""
    url, state = catalog_server
    state["payload"] = payload
    with pytest.raises(ProviderError, match="invalid model catalog") as error:
        LiteLLMClient(url, "UNSET_CATALOG_TEST_KEY", None).list_models()
    assert str(error.value) == "The provider returned an invalid model catalog."


@pytest.mark.parametrize("status", [302, 401, 403, 404, 500])
def test_http_failures_do_not_follow_redirects_or_expose_server_body(catalog_server, status):
    """Authentication and unavailable listings remain recoverable without sharing response bodies."""
    url, state = catalog_server
    state.update(status=status, payload={"error": "private server body"})
    with pytest.raises(ProviderError) as error:
        LiteLLMClient(url, "UNSET_CATALOG_TEST_KEY", None).list_models()
    assert "private" not in str(error.value)
    assert len(state["requests"]) == 1
    assert "your choice is kept" in catalog_message(CatalogRequest("rewrite", "litellm", url), None).lower()


def test_empty_catalog_is_not_a_failure_and_does_not_invent_a_default(catalog_server):
    """A working listing can contain no deployments, and manual models remain a separate choice."""
    url, state = catalog_server
    state["payload"] = {"data": []}
    assert LiteLLMClient(url, "UNSET_CATALOG_TEST_KEY", None).list_models() == []
    assert "No matching models" in catalog_message(CatalogRequest("speech", "litellm", url), [])


def test_local_catalog_lists_only_installed_whisper_and_never_downloads(tmp_path):
    """Use a real subprocess with a strict command fixture and installed flags."""
    script = tmp_path / "voxtype.py"
    script.write_text(
        "import json, sys\n"
        "assert sys.argv[1:] == ['info', 'models', '--json', '--engine', 'whisper']\n"
        "print(json.dumps({'engines': {'whisper': {'models': ["
        "{'name': 'small', 'installed': True}, {'name': 'medium', 'installed': False}]}}}))\n"
    )
    client = VoxtypeCatalog((sys.executable, str(script)))
    assert [model.identifier for model in client.list_models()] == ["small"]
    assert client.process.poll() == 0
    cancelled = VoxtypeCatalog((sys.executable, str(script)))
    cancelled.cancel()
    with pytest.raises(ProviderError):
        cancelled.list_models()
    assert cancelled.process is None


def test_setup_hints_do_not_disclose_keys_or_claim_authentication():
    """Availability hints report only local evidence and useful next steps."""
    present = {"ELEVENLABS_API_KEY": "private-api-key", "CUSTOM_KEY": "private-api-key"}
    for provider in ("elevenlabs", "litellm"):
        hint = connection_hint(provider, "CUSTOM_KEY", present)
        assert "private-api-key" not in hint and "Key found" in hint and "Account access is checked" in hint
    assert "No key found" in connection_hint("litellm", "CUSTOM_KEY", {})
    assert "restart Mluva" in connection_hint("elevenlabs", "", {})
    assert "Sign-in is not checked" in connection_hint("codex", "", {}, lambda _name: "/fixture/codex")
    assert "Install Voxtype" in connection_hint("voxtype", "", {}, lambda _name: None)


def test_provider_switching_persists_independent_models_and_key_references(tmp_path):
    """Round-trip both routes while preserving native defaults and inactive provider choices."""
    path = tmp_path / "config.json"
    config = AppConfig(
        rewrite_model="native-model",
        rewrite_fast_mode=True,
        litellm_model="chat-alias",
        transcription_remote_model="audio-alias",
        voxtype_model="small",
        litellm_api_key_env="WRITER_KEY",
        transcription_api_key_env="SPEECH_KEY",
    )
    for speech, rewrite in (("voxtype", "litellm"), ("litellm", "codex"), ("elevenlabs", "codex")):
        config = replace(config, transcription_provider=speech, rewrite_provider=rewrite)
        save_config(config, path)
        assert load_config(path) == config
        assert config.litellm_model == "chat-alias" and config.rewrite_model == "native-model"
        assert config.transcription_remote_model == "audio-alias" and config.voxtype_model == "small"
        assert path.stat().st_mode & 0o777 == 0o600
