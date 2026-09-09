"""Provider-neutral text and speech transports through LiteLLM and local Voxtype."""

import json
import os
import subprocess
import threading
import time
import urllib.error
import urllib.request
import uuid
from collections.abc import Callable
from pathlib import Path

from voice_scribe_linux.codex_client import CodexModel
from voice_scribe_linux.config import AppConfig, elevenlabs_api_key, validate_provider_url
from voice_scribe_linux.elevenlabs import ElevenLabsClient, TranscriptionResult, encode_multipart

LANGUAGES: dict[str, str] = dict(
    zip(
        ("eng", "ces", "spa", "fra", "deu", "ita", "por", "nld", "jpn", "zho", "kor", "pol", "rus", "slk", "ukr"),
        ("en", "cs", "es", "fr", "de", "it", "pt", "nl", "ja", "zh", "ko", "pl", "ru", "sk", "uk"),
        strict=True,
    )
)
MAX_HTTP_BYTES = 2_000_000


class ProviderError(RuntimeError):
    """Expose a controlled error without provider response bodies or credentials."""


class _NoRedirect(urllib.request.HTTPRedirectHandler):
    """Keep credentials and user content on the explicitly configured endpoint."""

    def redirect_request(self, *_args: object, **_kwargs: object) -> None:
        """Reject redirects instead of forwarding authenticated requests elsewhere."""
        return None


class LiteLLMClient:
    """Use LiteLLM's OpenAI-compatible API without importing its server dependencies."""

    def __init__(self, base_url: str, key_env: str, model: str | None, timeout: float = 60) -> None:
        """Freeze endpoint and credential reference for one isolated request lifecycle."""
        validate_provider_url(base_url)
        self.base_url = base_url.rstrip("/")
        self.key_env = key_env
        self.model = model
        self.timeout = timeout
        self.cancelled = threading.Event()
        self.response = None

    def _open(self, path: str, body: bytes | None = None, content_type: str = "application/json"):
        """Send only the selected endpoint a bounded request with an optional environment key."""
        if self.cancelled.is_set():
            raise ProviderError("Request cancelled.")
        headers = {"Content-Type": content_type, "Accept": "application/json"}
        key = os.environ.get(self.key_env, "")
        if key:
            headers["Authorization"] = f"Bearer {key}"
        request = urllib.request.Request(self.base_url + path, data=body, headers=headers)
        try:
            self.response = urllib.request.build_opener(_NoRedirect()).open(request, timeout=self.timeout)
            if self.cancelled.is_set():
                self.response.close()
                raise ProviderError("Request cancelled.")
            return self.response
        except urllib.error.HTTPError as error:
            raise ProviderError(f"Provider request failed (HTTP {error.code}). Check model and credentials.") from None
        except (OSError, ValueError):
            raise ProviderError("Could not connect to the configured provider.") from None

    def list_models(self, *, capability: str | None = None) -> list[CodexModel]:
        """Read bounded identifiers, filtering explicit task metadata without guessing from aliases."""
        try:
            with self._open("/models") as response:
                raw = response.read(MAX_HTTP_BYTES + 1)
            if len(raw) > MAX_HTTP_BYTES:
                raise ValueError
            rows = json.loads(raw)["data"]
            if not isinstance(rows, list) or len(rows) > 2000:
                raise ValueError
            identifiers = []
            for row in rows:
                if not isinstance(row, dict) or not valid_model_id(row["id"]):
                    raise ValueError
                info = row.get("model_info", {})
                if not isinstance(info, dict):
                    raise ValueError
                mode = info.get("mode", row.get("mode"))
                if mode is not None and not isinstance(mode, str):
                    raise ValueError
                allowed = {"speech": {"audio_transcription", "transcription", "stt"}, "rewrite": {"chat"}}
                if capability in allowed and mode and mode not in allowed[capability]:
                    continue
                identifiers.append(row["id"])
            return [CodexModel(value, value, value, value == self.model) for value in dict.fromkeys(identifiers)]
        except (OSError, ValueError, KeyError, TypeError):
            raise ProviderError("The provider returned an invalid model catalog.") from None

    def resolve_model(self, requested_model: str | None) -> str:
        """Require an explicit deployment alias; catalog discovery is optional for compatible servers."""
        model = self.model or requested_model
        if not model:
            raise ProviderError("Choose a LiteLLM model in Settings → Providers.")
        return model

    def transform(
        self,
        prompt: str,
        cwd: Path,
        model: str | None = None,
        *,
        max_output_characters: int = 8000,
        on_delta: Callable[[str], None] | None = None,
        effort: str | None = None,
        service_tier: str | None = None,
    ) -> str:
        """Stream plain text, rejecting incomplete, oversized, tool-call and cancelled results."""
        model = self.resolve_model(model)
        body = json.dumps(
            {
                "model": model,
                "messages": [{"role": "user", "content": prompt}],
                "stream": True,
            }
        ).encode()
        parts: list[str] = []
        size = 0
        completed = False
        started = time.monotonic()
        try:
            with self._open("/chat/completions", body) as response:
                event_lines: list[str] = []
                received_bytes = 0
                while raw := response.readline(MAX_HTTP_BYTES + 1):
                    received_bytes += len(raw)
                    if received_bytes > MAX_HTTP_BYTES:
                        raise ProviderError("The provider stream exceeded the response size limit.")
                    if self.cancelled.is_set() or time.monotonic() - started > self.timeout:
                        raise ProviderError("Rewrite cancelled or timed out.")
                    if len(raw) > MAX_HTTP_BYTES:
                        raise ProviderError("The provider returned an oversized event.")
                    line = raw.decode("utf-8").rstrip("\r\n")
                    if line.startswith("data:"):
                        event_lines.append(line[5:].lstrip())
                    if line or not event_lines:
                        continue
                    data = "\n".join(event_lines)
                    event_lines.clear()
                    if data == "[DONE]":
                        break
                    event = json.loads(data)
                    if "error" in event:
                        raise ProviderError("The rewrite provider reported a failure.")
                    for choice in event["choices"]:
                        if choice.get("index", 0) != 0:
                            continue
                        delta = choice["delta"]
                        if delta.get("tool_calls") or delta.get("function_call"):
                            raise ProviderError("The provider returned a tool request instead of text.")
                        text = delta.get("content") or ""
                        if not isinstance(text, str):
                            raise ValueError
                        size += len(text)
                        if size > max_output_characters:
                            raise ProviderError("The rewrite exceeded the document size limit.")
                        if text:
                            parts.append(text)
                            if on_delta:
                                on_delta(text)
                        reason = choice.get("finish_reason")
                        if reason is not None:
                            if reason != "stop":
                                raise ProviderError("The provider stopped before producing a complete rewrite.")
                            completed = True
            result = "".join(parts).strip()
            if not completed or not result or self.cancelled.is_set():
                raise ProviderError("The provider did not finish the rewrite.")
            return result
        except (OSError, ValueError, KeyError, TypeError):
            raise ProviderError("The provider returned an invalid or interrupted rewrite.") from None

    def transcribe(self, file_path: Path, language_code: str, model_id: str = "") -> TranscriptionResult:
        """Upload a finalized WAV to the chosen transcription deployment."""
        fields = {"model": self.resolve_model(model_id), "response_format": "json"}
        if language_code != "auto":
            fields["language"] = LANGUAGES.get(language_code, language_code)
        boundary = f"mluva-{uuid.uuid4().hex}"
        body = encode_multipart(fields, file_path, boundary)
        try:
            with self._open("/audio/transcriptions", body, f"multipart/form-data; boundary={boundary}") as response:
                raw = response.read(MAX_HTTP_BYTES + 1)
            if len(raw) > MAX_HTTP_BYTES or self.cancelled.is_set():
                raise ValueError
            payload = json.loads(raw)
            text = payload["text"]
            if not isinstance(text, str) or not text.strip():
                raise ValueError
            return TranscriptionResult(text, language_code, None, None)
        except (OSError, ValueError, KeyError, TypeError):
            raise ProviderError("The provider did not return a complete transcript.") from None

    def close(self) -> None:
        """Release the request's HTTP response without exposing its contents."""
        if self.response is not None:
            self.response.close()

    def cancel(self) -> None:
        """Invalidate late output immediately, then release the transport."""
        self.cancelled.set()
        self.close()


class VoxtypeClient:
    """Run Omarchy's Whisper engine on a file, with no daemon, clipboard or key injection."""

    def __init__(self, model: str | None = None, command: str = "voxtype", timeout: float = 300) -> None:
        """Retain an optional local model override and a bounded subprocess timeout."""
        self.model = model
        self.command = command
        self.timeout = timeout
        self.cancelled = threading.Event()
        self.process = None

    def transcribe(self, file_path: Path, language_code: str, model_id: str = "") -> TranscriptionResult:
        """Force offline Whisper and collect only the file command's standard output."""
        command = [
            self.command,
            "--quiet",
            "--engine",
            "whisper",
            "--whisper-mode",
            "local",
            "--language",
            LANGUAGES.get(language_code, language_code),
        ]
        if self.model:
            command.extend(("--model", self.model))
        command.extend(("transcribe", str(file_path)))
        if self.cancelled.is_set():
            raise ProviderError("Local transcription cancelled.")
        try:
            self.process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
            if self.cancelled.is_set():
                self.process.kill()
            stdout, _stderr = self.process.communicate(timeout=self.timeout)
            if self.process.returncode != 0 or self.cancelled.is_set():
                raise ProviderError("Local transcription failed. Check Voxtype and install a Whisper model.")
        except subprocess.TimeoutExpired:
            assert self.process is not None
            self.process.kill()
            self.process.communicate()
            raise ProviderError("Local transcription timed out.") from None
        except OSError:
            raise ProviderError("Local transcription failed. Check Voxtype and install a Whisper model.") from None
        finally:
            self.process = None
        text = stdout.partition("\n\n")[2].strip()
        if not text or len(text) > MAX_HTTP_BYTES:
            raise ProviderError("Voxtype did not return a usable transcript.")
        return TranscriptionResult(text, language_code, None, None)

    def cancel(self) -> None:
        """Stop only this file inference process and reject its late result."""
        self.cancelled.set()
        process = self.process
        if process is not None:
            try:
                process.kill()
            except ProcessLookupError:
                pass


def transcription_client(config: AppConfig) -> ElevenLabsClient | LiteLLMClient | VoxtypeClient:
    """Create the selected speech route without requiring unrelated providers' credentials."""
    if config.transcription_provider == "voxtype":
        return VoxtypeClient(config.voxtype_model)
    if config.transcription_provider == "litellm":
        return LiteLLMClient(
            config.transcription_base_url,
            config.transcription_api_key_env,
            config.transcription_remote_model,
            300,
        )
    return ElevenLabsClient(elevenlabs_api_key())


def valid_model_id(value: object) -> bool:
    """Keep catalog identifiers bounded, printable and free of hidden whitespace."""
    return (
        isinstance(value, str)
        and 0 < len(value) <= 200
        and value.isprintable()
        and not any(char.isspace() for char in value)
    )
