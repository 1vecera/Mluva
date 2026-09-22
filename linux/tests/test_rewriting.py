"""Exercise shared rewrite policy against real JSONL and loopback HTTP transports."""

import json
import sys
from http.server import BaseHTTPRequestHandler
from pathlib import Path
from unittest.mock import patch

import pytest
from http_fixture import local_http_server

from mluva_linux.codex_client import CodexAppServerClient, CodexAppServerError
from mluva_linux.config import AppConfig
from mluva_linux.providers import ProviderError
from mluva_linux.rewriting import UnsupportedRewriteSpeed, UnsupportedThinkingLevel, rewrite_client, rewrite_text


@pytest.mark.parametrize(
    "requested,fast,expected",
    [(None, False, "gpt-5.4"), ("codex-explicit", False, "gpt-5.4-mini"), ("codex-default", True, "gpt-5.4")],
)
@pytest.mark.parametrize("stream", [False, True])
def test_native_rewrites_share_model_speed_and_streaming_policy(tmp_path, requested, fast, expected, stream):
    """Resolve aliases and advertised tiers identically for Live snapshots and streamed conversations."""
    client = CodexAppServerClient(
        command=(
            sys.executable,
            str(Path(__file__).with_name("fake_app_server.py")),
            "--expect-fast" if fast else "--expect-standard",
        )
    )
    deltas = []
    try:
        result = rewrite_text(
            client,
            AppConfig(rewrite_model=requested, rewrite_fast_mode=fast),
            "Synthetic source",
            tmp_path,
            on_delta=deltas.append if stream else None,
        )
        assert (result.text, result.model) == ("Clean text.", expected)
        assert deltas == (["Clean ", "text."] if stream else [])
    finally:
        client.close()


@pytest.mark.parametrize(
    "model,error", [("codex-explicit", UnsupportedRewriteSpeed), ("missing-model", CodexAppServerError)]
)
def test_unavailable_model_or_speed_never_starts_a_transform(tmp_path, model, error):
    """Keep explicit selections authoritative and avoid a silently changed model or service tier."""
    client = CodexAppServerClient(command=(sys.executable, str(Path(__file__).with_name("fake_app_server.py"))))
    try:
        with patch.object(CodexAppServerClient, "transform") as transform, pytest.raises(error):
            rewrite_text(client, AppConfig(rewrite_model=model, rewrite_fast_mode=True), "Synthetic source", tmp_path)
        transform.assert_not_called()
    finally:
        client.close()


@pytest.mark.parametrize("stream", [False, True])
def test_compatible_provider_needs_only_its_explicit_alias_and_chat_endpoint(tmp_path, stream):
    """A deployment without model discovery works in both rewrite paths and ignores stored native speed settings."""
    requests = []

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_args):
            """Keep test requests out of the console."""

        def do_POST(self):  # noqa: N802
            """Accept one model request and complete an independent SSE response."""
            requests.append((self.path, json.loads(self.rfile.read(int(self.headers["Content-Length"])))))
            self.send_response(200)
            self.end_headers()
            event = {"choices": [{"delta": {"content": "Čistý text."}, "finish_reason": "stop"}]}
            self.wfile.write(("data: " + json.dumps(event) + "\n\ndata: [DONE]\n\n").encode())

    with local_http_server(Handler) as server:
        config = AppConfig(
            rewrite_provider="litellm",
            litellm_base_url=f"http://127.0.0.1:{server.server_port}/v1",
            litellm_api_key_env="MLUVA_UNUSED_TEST_KEY",
            litellm_model="my-private-alias",
            rewrite_model="unrelated-native-model",
            rewrite_fast_mode=True,
        )
        with patch("mluva_linux.rewriting.CodexAppServerClient", side_effect=AssertionError("Wrong provider")):
            client = rewrite_client(config)
            deltas = []
            try:
                result = rewrite_text(
                    client, config, "Synthetic source", tmp_path, on_delta=deltas.append if stream else None
                )
            finally:
                client.close()
        assert (result.text, result.model) == ("Čistý text.", "my-private-alias")
        assert deltas == (["Čistý text."] if stream else [])
    assert requests == [
        (
            "/v1/chat/completions",
            {
                "model": "my-private-alias",
                "messages": [{"role": "user", "content": "Synthetic source"}],
                "stream": True,
            },
        )
    ]


def test_compatible_rewrite_without_alias_fails_before_network(tmp_path):
    """Use the same actionable missing-model error in Live and conversation workflows."""
    config = AppConfig(rewrite_provider="litellm")
    client = rewrite_client(config)
    with patch.object(client, "_open") as request, pytest.raises(ProviderError, match="Choose a LiteLLM model"):
        rewrite_text(client, config, "Synthetic source", tmp_path)
    request.assert_not_called()


def test_short_request_budgets_reach_the_selected_transport():
    """Catalog and title requests must not silently keep HTTP's longer default timeout."""
    config = AppConfig(rewrite_provider="litellm", litellm_model="alias")
    assert rewrite_client(config).timeout == 60
    assert rewrite_client(config, request_timeout_seconds=10).timeout == 10
    assert rewrite_client(config, request_timeout_seconds=10, turn_timeout_seconds=20).timeout == 20
    native = rewrite_client(AppConfig(), request_timeout_seconds=10, turn_timeout_seconds=20)
    assert (native.request_timeout_seconds, native.turn_timeout_seconds) == (10, 20)


def test_explicit_thinking_level_reaches_codex_and_stale_level_never_sends_text(tmp_path):
    """Use advertised levels on the wire and fail before inference when a model cannot serve one."""
    client = CodexAppServerClient(
        command=(sys.executable, str(Path(__file__).with_name("fake_app_server.py")), "--expect-high")
    )
    try:
        result = rewrite_text(client, AppConfig(rewrite_reasoning_effort="high"), "Synthetic source", tmp_path)
        assert result.text == "Clean text."
        with patch.object(CodexAppServerClient, "transform") as transform, pytest.raises(UnsupportedThinkingLevel):
            rewrite_text(
                client, AppConfig(rewrite_model="gpt-5.4-mini", rewrite_reasoning_effort="high"), "Source", tmp_path
            )
        transform.assert_not_called()
    finally:
        client.close()
