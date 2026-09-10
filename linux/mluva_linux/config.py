"""Persistent, non-secret Linux application settings."""

import json
import os
import re
from collections.abc import Mapping
from dataclasses import asdict, dataclass
from enum import StrEnum
from pathlib import Path
from urllib.parse import urlsplit

ELEVENLABS_API_KEY_ENVIRONMENT_VARIABLES = (
    "ELEVENLABS_API_KEY",
    "DAS_ITEM_ELEVEN_LABS_API_KEY__CREDENTIAL",
    "ELEVEN_LABS_STT_TOKEN",
)
SUPPORTED_CAPTURE_MODES = frozenset(("dictation", "command", "scratchpad"))
TRANSCRIPTION_LANGUAGE_OPTIONS = (
    ("auto", "Auto-detect"),
    ("eng", "English"),
    ("ces", "Czech"),
    ("spa", "Spanish"),
    ("fra", "French"),
    ("deu", "German"),
    ("ita", "Italian"),
    ("por", "Portuguese"),
    ("nld", "Dutch"),
    ("jpn", "Japanese"),
    ("zho", "Mandarin Chinese"),
    ("kor", "Korean"),
    ("pol", "Polish"),
    ("rus", "Russian"),
    ("slk", "Slovak"),
    ("ukr", "Ukrainian"),
)
MAX_CODEX_MODEL_CHARACTERS = 200
FUNCTION_KEY_OPTIONS = tuple(f"F{number}" for number in range(1, 25))
DEFAULT_GLOBAL_RECORDING_KEY = "F9"


class AudioRetentionPolicy(StrEnum):
    """Describe when finalized microphone audio remains available for recovery."""

    NEVER = "never"
    FAILURES = "failures"
    ALWAYS = "always"

    def should_retain(self, delivery_succeeded: bool) -> bool:
        """Return whether the configured policy retains this session's audio."""
        if self is AudioRetentionPolicy.NEVER:
            return False
        if self is AudioRetentionPolicy.FAILURES:
            return not delivery_succeeded
        return True


@dataclass(frozen=True, slots=True)
class AppConfig:
    """Describe settings safe to persist on the local machine."""

    language_code: str = "eng"
    transcription_model: str = "scribe_v2"
    codex_model: str | None = None
    rewrite_model: str | None = None
    rewrite_fast_mode: bool = False
    rewrite_provider: str = "codex"
    litellm_base_url: str = "http://localhost:4000/v1"
    litellm_model: str | None = None
    litellm_api_key_env: str = "LITELLM_API_KEY"
    transcription_provider: str = "elevenlabs"
    transcription_base_url: str = "http://localhost:4000/v1"
    transcription_api_key_env: str = "LITELLM_API_KEY"
    transcription_remote_model: str = "whisper"
    voxtype_model: str | None = None
    transcription_chunk_seconds: int = 8
    auto_copy_dictation: bool = True
    auto_copy_rewrite: bool = True
    show_copy_action: bool = True
    show_save_action: bool = True
    review_timeout_seconds: int = 4
    smooth_scrolling: bool = True
    scroll_duration_ms: int = 800
    scroll_lookahead_lines: int = 2
    live_rewrite_enabled: bool = False
    live_rewrite_template: str = "task-spec"
    live_rewrite_custom_instructions: str = ""
    live_rewrite_min_characters: int = 160
    live_rewrite_interval_seconds: int = 4
    microphone_target: str | None = None
    system_audio_target: str | None = None
    default_mode: str = "dictation"
    global_recording_key: str = DEFAULT_GLOBAL_RECORDING_KEY
    auto_paste: bool = False
    automatic_titles: bool = True
    spoken_commands_enabled: bool = True
    remember_per_application: bool = False
    audio_retention_policy: AudioRetentionPolicy = AudioRetentionPolicy.FAILURES
    incognito_mode: bool = False
    history_retention_days: int = 0

    def __post_init__(self) -> None:
        """Reject invalid provider, routing, mode, and retention settings."""
        if not isinstance(self.language_code, str) or (
            self.language_code != "auto" and re.fullmatch(r"[a-z]{2,3}", self.language_code) is None
        ):
            raise ValueError("language_code must be 'auto' or a lowercase ISO-639-1/3 code")
        if self.transcription_model != "scribe_v2":
            raise ValueError("transcription_model must be 'scribe_v2'")
        _validate_codex_model(self.codex_model)
        _validate_codex_model(self.rewrite_model)
        _validate_codex_model(self.litellm_model)
        _validate_codex_model(self.voxtype_model)
        _validate_codex_model(self.transcription_remote_model)
        if self.rewrite_provider not in {"codex", "litellm"}:
            raise ValueError("rewrite_provider must be codex or litellm")
        if self.transcription_provider not in {"elevenlabs", "litellm", "voxtype"}:
            raise ValueError("transcription_provider must be elevenlabs, litellm or voxtype")
        for endpoint in (self.litellm_base_url, self.transcription_base_url):
            validate_provider_url(endpoint)
        for name in (self.litellm_api_key_env, self.transcription_api_key_env):
            if not isinstance(name, str) or re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]{0,127}", name) is None:
                raise ValueError("API key settings must name an environment variable, never contain a key")
        for name in (
            "auto_copy_dictation",
            "auto_copy_rewrite",
            "show_copy_action",
            "show_save_action",
            "smooth_scrolling",
            "live_rewrite_enabled",
        ):
            if not isinstance(getattr(self, name), bool):
                raise TypeError(f"{name} must be a boolean")
        for name, minimum, maximum in (
            ("review_timeout_seconds", 1, 60),
            ("scroll_duration_ms", 0, 2000),
            ("scroll_lookahead_lines", 0, 6),
            ("transcription_chunk_seconds", 3, 30),
            ("live_rewrite_min_characters", 40, 4000),
            ("live_rewrite_interval_seconds", 2, 60),
        ):
            value = getattr(self, name)
            if type(value) is not int or not minimum <= value <= maximum:
                raise ValueError(f"{name} must be an integer from {minimum} to {maximum}")
        if self.live_rewrite_template not in {"task-spec", "structured-note", "polish", "custom"}:
            raise ValueError("Unsupported live rewrite template")
        if (
            not isinstance(self.live_rewrite_custom_instructions, str)
            or len(self.live_rewrite_custom_instructions) > 8000
        ):
            raise ValueError("Custom live instructions must contain at most 8000 characters")
        if not isinstance(self.rewrite_fast_mode, bool):
            raise TypeError("rewrite_fast_mode must be a boolean")
        _validate_pipewire_target(self.microphone_target, "microphone_target")
        _validate_pipewire_target(self.system_audio_target, "system_audio_target")
        if self.default_mode not in SUPPORTED_CAPTURE_MODES:
            raise ValueError(f"Unsupported default capture mode: {self.default_mode}")
        if not isinstance(self.global_recording_key, str):
            raise TypeError("global_recording_key must be text")
        if self.global_recording_key not in FUNCTION_KEY_OPTIONS:
            raise ValueError("global_recording_key must be one of F1 through F24")
        if not isinstance(self.auto_paste, bool):
            raise TypeError("auto_paste must be a boolean")
        if not isinstance(self.automatic_titles, bool):
            raise TypeError("automatic_titles must be a boolean")
        if not isinstance(self.spoken_commands_enabled, bool):
            raise TypeError("spoken_commands_enabled must be a boolean")
        if not isinstance(self.remember_per_application, bool):
            raise TypeError("remember_per_application must be a boolean")
        if not isinstance(self.incognito_mode, bool):
            raise TypeError("incognito_mode must be a boolean")
        if not isinstance(self.audio_retention_policy, AudioRetentionPolicy):
            raise TypeError("audio_retention_policy must be an AudioRetentionPolicy")
        if isinstance(self.history_retention_days, bool) or not isinstance(self.history_retention_days, int):
            raise TypeError("history_retention_days must be an integer")
        if self.history_retention_days < 0:
            raise ValueError("history_retention_days cannot be negative")


def default_config_dir(environ: dict[str, str]) -> Path:
    """Return the XDG-compliant directory for application configuration."""
    base = Path(environ["XDG_CONFIG_HOME"]) if "XDG_CONFIG_HOME" in environ else Path(environ["HOME"]) / ".config"
    return base / "mluva"


def default_data_dir(environ: dict[str, str]) -> Path:
    """Return the XDG-compliant directory for durable user data."""
    base = Path(environ["XDG_DATA_HOME"]) if "XDG_DATA_HOME" in environ else Path(environ["HOME"]) / ".local/share"
    return base / "mluva"


def default_runtime_dir(environ: dict[str, str]) -> Path:
    """Return an owner-local runtime directory with a durable fallback for non-desktop tests."""
    if "XDG_RUNTIME_DIR" in environ:
        return Path(environ["XDG_RUNTIME_DIR"]) / "mluva"
    return default_data_dir(environ) / "runtime"


def load_config(path: Path) -> AppConfig:
    """Load persisted settings or return documented defaults on first launch."""
    if not path.exists():
        return AppConfig()
    payload = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(payload, dict):
        raise ValueError("Mluva config must be a JSON object")
    legacy_retention = payload.pop("retain_audio_on_failure", None)
    if "audio_retention_policy" not in payload and legacy_retention is not None:
        payload["audio_retention_policy"] = "failures" if legacy_retention else "never"
    if "audio_retention_policy" in payload:
        payload["audio_retention_policy"] = AudioRetentionPolicy(payload["audio_retention_policy"])
    return AppConfig(**payload)


def save_config(config: AppConfig, path: Path) -> None:
    """Persist non-secret settings with owner-only permissions."""
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    temporary_path = path.with_suffix(".tmp")
    temporary_path.write_text(json.dumps(asdict(config), indent=2) + "\n", encoding="utf-8")
    temporary_path.chmod(0o600)
    temporary_path.replace(path)


def elevenlabs_api_key(
    environ: Mapping[str, str] = os.environ,
    variable_names: tuple[str, ...] = ELEVENLABS_API_KEY_ENVIRONMENT_VARIABLES,
) -> str:
    """Resolve the first supported ElevenLabs credential without persisting it."""
    for variable_name in variable_names:
        value = environ[variable_name] if variable_name in environ else ""
        if value and not value.isspace():
            return value
    raise RuntimeError(
        "ElevenLabs credential unavailable. Set ELEVENLABS_API_KEY in Mluva's process environment "
        "through a secret manager or session service."
    )


def _validate_pipewire_target(value: str | None, label: str) -> None:
    """Reject blank, unbounded, or control-bearing persisted PipeWire node names."""
    if value is None:
        return
    if not isinstance(value, str):
        raise TypeError(f"{label} must be text or null")
    if value != value.strip() or not value or len(value) > 512 or any(ord(character) < 32 for character in value):
        raise ValueError(f"{label} must be a bounded single-line PipeWire node name")


def _validate_codex_model(value: str | None) -> None:
    """Accept one optional bounded model identifier without hidden whitespace or controls."""
    if value is None:
        return
    if not isinstance(value, str):
        raise TypeError("codex_model must be text or null")
    if (
        value != value.strip()
        or not value
        or len(value) > MAX_CODEX_MODEL_CHARACTERS
        or any(ord(character) < 32 for character in value)
    ):
        raise ValueError("codex_model must be a bounded single-line model identifier")


def validate_provider_url(value: str) -> None:
    """Allow HTTPS services and loopback HTTP without credentials or secret URL components."""
    if not isinstance(value, str) or value != value.strip() or any(ord(char) < 32 for char in value):
        raise ValueError("Provider URL must be a plain HTTP(S) base URL")
    parsed = urlsplit(value)
    if (
        not parsed.hostname
        or parsed.scheme not in {"http", "https"}
        or parsed.username is not None
        or parsed.password is not None
        or parsed.query
        or parsed.fragment
        or (parsed.scheme == "http" and parsed.hostname not in {"localhost", "127.0.0.1", "::1"})
    ):
        raise ValueError("Use HTTPS (or loopback HTTP) without credentials, query parameters or fragments")
    _ = parsed.port
