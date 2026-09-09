"""Small provider catalog and local setup hints, independent of GTK and capture."""

import json
import shutil
import subprocess
import threading
from collections.abc import Callable, Mapping
from dataclasses import dataclass

from voice_scribe_linux.codex_client import CodexAppServerClient, CodexModel
from voice_scribe_linux.config import ELEVENLABS_API_KEY_ENVIRONMENT_VARIABLES
from voice_scribe_linux.providers import MAX_HTTP_BYTES, LiteLLMClient, ProviderError, valid_model_id


@dataclass(frozen=True, slots=True)
class Provider:
    """Describe a supported route without duplicating runtime defaults or credentials."""

    id: str
    label: str
    model_field: str
    default_label: str | None
    description: str


SPEECH_PROVIDERS = (
    Provider(
        "elevenlabs",
        "ElevenLabs",
        "transcription_model",
        None,
        "Live recognition with Scribe v2. Meeting also uses ElevenLabs for speaker labels.",
    ),
    Provider(
        "voxtype",
        "Voxtype · local Whisper",
        "voxtype_model",
        "Use Voxtype configuration",
        "Audio stays on this device. Install a multilingual Whisper model for languages other than English.",
    ),
    Provider(
        "litellm",
        "Compatible API",
        "transcription_remote_model",
        None,
        "Recognizes audio through your LiteLLM or OpenAI-compatible server.",
    ),
)
REWRITE_PROVIDERS = (
    Provider(
        "codex",
        "Codex",
        "rewrite_model",
        "Use Codex default",
        "Uses your installed Codex app-server, sign-in and provider for inference.",
    ),
    Provider(
        "litellm",
        "Compatible API",
        "litellm_model",
        None,
        "Uses a LiteLLM or OpenAI-compatible chat server for rewrites, cleanup and titles.",
    ),
)


def connection_hint(
    provider: str,
    key_env: str,
    environ: Mapping[str, str],
    which: Callable[[str], str | None] = shutil.which,
) -> str:
    """Report local setup evidence, never equating an installed client or key with account access."""
    if provider == "codex":
        if which("codex"):
            return "Codex found. Sign-in is not checked here; use codex login if needed, then refresh models."
        return "Install the Codex CLI and run codex login, then restart Mluva and refresh models."
    if provider == "voxtype":
        if which("voxtype"):
            return "Voxtype found. Refresh to list installed Whisper models; use voxtype setup model to add one."
        return "Install Voxtype and a Whisper model with voxtype setup model, then restart Mluva."
    names = ELEVENLABS_API_KEY_ENVIRONMENT_VARIABLES if provider == "elevenlabs" else (key_env,)
    if any(environ.get(name, "").strip() for name in names):
        return "Key found in Mluva’s environment. Account access is checked when you use the provider."
    if provider == "elevenlabs":
        return "Set ELEVENLABS_API_KEY through your secret manager or session service, then restart Mluva."
    return (
        "No key found. A local server may not need one. Otherwise set the named variable in Mluva’s "
        "launch environment and restart. Connection details contain the variable name only."
    )


@dataclass(frozen=True, slots=True)
class CatalogRequest:
    """Freeze just the nonsecret route; speech and rewrite catalogs never cross endpoints."""

    scope: str
    provider: str
    base_url: str = ""
    key_env: str = ""

    def client(self):
        """Create a bounded discovery client, without starting a provider or sending user content."""
        if self.provider == "codex":
            return CodexAppServerClient(request_timeout_seconds=5)
        if self.provider == "voxtype":
            return VoxtypeCatalog()
        return LiteLLMClient(self.base_url, self.key_env, None, timeout=5)


def read_catalog(client, request: CatalogRequest) -> list[CodexModel]:
    """Use advertised capabilities where present, retaining unknown deployment aliases."""
    if request.provider == "litellm":
        return client.list_models(capability=request.scope)
    models = client.list_models()
    if any(
        not valid_model_id(model.id)
        or not valid_model_id(model.identifier)
        or not isinstance(model.name, str)
        or not model.name.isprintable()
        or len(model.name) > 200
        or type(model.hidden) is not bool
        or type(model.is_default) is not bool
        for model in models
    ):
        raise ProviderError("Invalid model catalog.")
    return models


def catalog_message(request: CatalogRequest, models: list[CodexModel] | None) -> str:
    """Explain an empty or failed listing without displaying a provider exception or response."""
    if models is None:
        if request.provider == "codex":
            return "Could not load models. Check Codex sign-in, then refresh. Your choice is kept."
        if request.provider == "voxtype":
            return "Could not list local models. Check Voxtype or enter a model name. Your choice is kept."
        return (
            "Could not load models. Check the endpoint (including /v1 if needed) and key variable. "
            "You can still enter a deployment ID; your choice is kept."
        )
    if request.provider == "voxtype":
        return (
            f"{len(models)} installed Whisper model(s). Refresh after adding a model."
            if models
            else "No installed Whisper models found. Run voxtype setup model, then refresh."
        )
    if not models:
        return "No matching models listed. Enter a model ID or check the server’s deployment configuration."
    return f"{len(models)} model(s) listed. Listing a model does not verify that your account can run it."


class VoxtypeCatalog:
    """Inspect installed Whisper models without downloads, capture, daemon or output commands."""

    def __init__(self, command: tuple[str, ...] = ("voxtype",)) -> None:
        """Allow an isolated CLI fixture while keeping production discovery file-only."""
        self.command = command
        self.process = None
        self.cancelled = threading.Event()

    def list_models(self) -> list[CodexModel]:
        """Read the installed model flags from Voxtype's machine-readable inventory."""
        try:
            if self.cancelled.is_set():
                raise ValueError
            self.process = subprocess.Popen(
                (*self.command, "info", "models", "--json", "--engine", "whisper"),
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL,
            )
            if self.cancelled.is_set():
                self.close()
            raw, _stderr = self.process.communicate(timeout=5)
            if self.process.returncode or len(raw) > MAX_HTTP_BYTES:
                raise ValueError
            payload = json.loads(raw)
            rows = payload["engines"]["whisper"]["models"]
            if not isinstance(rows, list) or len(rows) > 2000:
                raise ValueError
            models = []
            for row in rows:
                name = row["name"]
                if not valid_model_id(name) or type(row["installed"]) is not bool:
                    raise ValueError
                if row["installed"]:
                    models.append(CodexModel(name, name, name, False))
            return list({model.identifier: model for model in models}.values())
        except (OSError, ValueError, KeyError, TypeError, subprocess.TimeoutExpired):
            raise ProviderError("Could not read installed Voxtype models.") from None
        finally:
            self.close()
            if self.process is not None:
                self.process.communicate()

    def close(self) -> None:
        """Stop only this inventory process, including on timeout or window close."""
        process = self.process
        if process is not None:
            if process.poll() is None:
                try:
                    process.kill()
                except ProcessLookupError:
                    pass

    def cancel(self) -> None:
        """Prevent startup or stop this inventory when its settings page is hidden."""
        self.cancelled.set()
        self.close()
