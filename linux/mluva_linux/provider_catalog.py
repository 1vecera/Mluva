"""Small provider catalog and local setup hints, independent of GTK and capture."""

import shutil
from collections.abc import Callable, Mapping
from dataclasses import dataclass

from mluva_linux.codex_client import CodexAppServerClient, CodexModel
from mluva_linux.config import ELEVENLABS_API_KEY_ENVIRONMENT_VARIABLES
from mluva_linux.providers import LiteLLMClient, ProviderError, valid_model_id


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
        "ElevenLabs · recommended",
        "transcription_model",
        None,
        "Fast live speech recognition with Scribe v2 Realtime. About $0.39/hour before taxes; "
        "$5 of usage is about 12.8 hours at that rate. Pricing may change. "
        "I’m not affiliated with ElevenLabs in any way—just a happy user.",
    ),
    Provider(
        "local",
        "Local model · private",
        "local_model",
        None,
        "Mluva downloads and owns its models. Audio stays on this device. Weights load only while transcribing.",
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
        "none",
        "Skip · transcription only",
        "rewrite_model",
        "No rewriting",
        "Polish, Live rewrite and generated titles stay off. Enable a provider later in Settings.",
    ),
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
    if provider == "local":
        return "Models are managed by Mluva. No other dictation app is required."
    if provider == "none":
        return "No text is sent for rewriting. Choose a provider to enable these controls."
    if provider == "codex":
        if which("codex"):
            return "Codex found. Sign-in is not checked here; use codex login if needed, then refresh models."
        return "Install the Codex CLI and run codex login, then restart Mluva and refresh models."
    names = ELEVENLABS_API_KEY_ENVIRONMENT_VARIABLES if provider == "elevenlabs" else (key_env,)
    if any(environ.get(name, "").strip() for name in names):
        return "Key found in Mluva’s environment. Account access is checked when you use the provider."
    if provider == "elevenlabs":
        return (
            "Add a key in Welcome setup, or set ELEVENLABS_API_KEY through your launch environment and restart Mluva."
        )
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
        return (
            "Could not load models. Check the endpoint (including /v1 if needed) and key variable. "
            "You can still enter a deployment ID; your choice is kept."
        )
    if not models:
        return "No matching models listed. Enter a model ID or check the server’s deployment configuration."
    return f"{len(models)} model(s) listed. Listing a model does not verify that your account can run it."
