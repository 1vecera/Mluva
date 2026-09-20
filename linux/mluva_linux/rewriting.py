"""Provider selection and execution shared by Live and conversation rewrites, without GTK."""

from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path
from typing import Protocol

from mluva_linux.codex_client import CodexAppServerClient, CodexModel, select_model
from mluva_linux.config import AppConfig
from mluva_linux.conversation import MAX_REWRITE_CHARACTERS
from mluva_linux.providers import LiteLLMClient


class RewriteClient(Protocol):
    """Describe the text transport independently of its process or HTTP implementation."""

    def list_models(self) -> list[CodexModel]:
        """Discover advertised models without sending document content."""
        ...

    def resolve_model(self, requested_model: str | None) -> str:
        """Resolve a configured identifier without silently selecting another model."""
        ...

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
        """Return completed text; deltas are provisional and failures raise."""
        ...

    def close(self) -> None:
        """Release the request's process or connection."""
        ...

    def cancel(self) -> None:
        """Cancel this exact request and release its transport."""
        ...


class DisabledRewriteClient:
    """Keep capture usable while preventing all inference when rewriting is skipped."""

    def list_models(self):
        """Expose no models and perform no discovery."""
        return []

    def resolve_model(self, *_args, **_kwargs):
        """Reject an accidental rewrite entry point before any external work."""
        raise RuntimeError("Choose a rewriting provider in Settings first.")

    transform = resolve_model

    def close(self):
        """No process or connection exists to release."""

    cancel = close


class UnsupportedRewriteSpeed(ValueError):
    """Expose an actionable speed selection failure without provider response text."""


@dataclass(frozen=True, slots=True)
class RewriteResult:
    """Pair completed text with the actual model that produced it."""

    text: str
    model: str


def rewrite_client(
    config: AppConfig,
    *,
    request_timeout_seconds: float | None = None,
    turn_timeout_seconds: float | None = None,
) -> RewriteClient:
    """Freeze one transport; short catalog/title budgets also apply to compatible HTTP providers."""
    if config.rewrite_provider == "none":
        return DisabledRewriteClient()
    if config.rewrite_provider == "litellm":
        timeout = turn_timeout_seconds if turn_timeout_seconds is not None else request_timeout_seconds
        return LiteLLMClient(
            config.litellm_base_url,
            config.litellm_api_key_env,
            config.litellm_model,
            timeout=60 if timeout is None else timeout,
        )
    return CodexAppServerClient(
        request_timeout_seconds=30 if request_timeout_seconds is None else request_timeout_seconds,
        turn_timeout_seconds=180 if turn_timeout_seconds is None else turn_timeout_seconds,
    )


def rewrite_text(
    client: RewriteClient,
    config: AppConfig,
    prompt: str,
    cwd: Path,
    *,
    on_delta: Callable[[str], None] | None = None,
) -> RewriteResult:
    """Apply the same frozen model, speed and output limits to every full-document rewrite."""
    effort = None
    service_tier = None
    if config.rewrite_provider == "none":
        raise RuntimeError("Choose a rewriting provider in Settings first.")
    if config.rewrite_provider == "litellm":
        model = client.resolve_model(config.litellm_model)
    else:
        selected = select_model(client.list_models(), config.rewrite_model or config.codex_model)
        if config.rewrite_fast_mode and selected.fast_tier is None:
            raise UnsupportedRewriteSpeed(
                "Fast mode is unavailable for this model. Turn it off or choose another model."
            )
        model = selected.identifier
        effort = selected.rewrite_effort
        service_tier = selected.fast_tier if config.rewrite_fast_mode else "default"
    text = client.transform(
        prompt,
        cwd,
        model,
        max_output_characters=MAX_REWRITE_CHARACTERS,
        on_delta=on_delta,
        effort=effort,
        service_tier=service_tier,
    )
    return RewriteResult(text, model)
