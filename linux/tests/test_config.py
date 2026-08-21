"""Coverage for Linux XDG settings and secret boundaries."""

import json
from pathlib import Path

import pytest

from voice_scribe_linux.config import (
    DEFAULT_GLOBAL_RECORDING_KEY,
    FUNCTION_KEY_OPTIONS,
    AppConfig,
    AudioRetentionPolicy,
    default_config_dir,
    default_data_dir,
    default_runtime_dir,
    elevenlabs_api_key,
    load_config,
    save_config,
)


def test_xdg_paths_prefer_explicit_roots() -> None:
    """Keep application files out of hard-coded home subdirectories."""
    environ = {"HOME": "/home/test", "XDG_CONFIG_HOME": "/config", "XDG_DATA_HOME": "/data"}
    assert default_config_dir(environ) == Path("/config/voice-scribe")
    assert default_data_dir(environ) == Path("/data/voice-scribe")
    assert default_runtime_dir({**environ, "XDG_RUNTIME_DIR": "/run/user/1000"}) == Path("/run/user/1000/voice-scribe")
    assert default_runtime_dir(environ) == Path("/data/voice-scribe/runtime")


def test_config_round_trip_does_not_include_api_key(tmp_path: Path) -> None:
    """Persist product settings while leaving credentials environment-only."""
    path = tmp_path / "config.json"
    config = AppConfig(
        language_code="ces",
        codex_model="gpt-5.4",
        microphone_target="alsa_input.usb_microphone",
        system_audio_target="alsa_output.usb_headset",
    )
    save_config(config, path)
    assert load_config(path) == config
    assert "api_key" not in path.read_text(encoding="utf-8")
    assert path.stat().st_mode & 0o777 == 0o600


def test_privacy_defaults_and_legacy_audio_retention_migration(tmp_path: Path) -> None:
    """Keep failure recovery as the default and migrate the foundation boolean setting."""
    assert AppConfig().audio_retention_policy is AudioRetentionPolicy.FAILURES
    assert AppConfig().default_mode == "dictation"
    assert AppConfig().global_recording_key == "F9" == DEFAULT_GLOBAL_RECORDING_KEY
    assert FUNCTION_KEY_OPTIONS == tuple(f"F{number}" for number in range(1, 25))
    assert not AppConfig().incognito_mode
    assert not AppConfig().auto_paste
    assert AppConfig().spoken_commands_enabled
    assert not AppConfig().remember_per_application
    assert AppConfig().history_retention_days == 0
    path = tmp_path / "config.json"
    path.write_text(json.dumps({"retain_audio_on_failure": False}), encoding="utf-8")
    migrated = load_config(path)
    assert migrated.audio_retention_policy is AudioRetentionPolicy.NEVER
    assert migrated.global_recording_key == "F9"


def test_explicit_experimental_auto_paste_choice_is_preserved(tmp_path: Path) -> None:
    """Default safely on new installs without overriding a user's persisted opt-in."""
    path = tmp_path / "config.json"
    path.write_text(json.dumps({"auto_paste": True}), encoding="utf-8")

    assert load_config(path).auto_paste


def test_config_rejects_negative_history_retention() -> None:
    """Prevent malformed configuration from turning a cutoff into unintended deletion."""
    with pytest.raises(ValueError, match="negative"):
        AppConfig(history_retention_days=-1)


def test_config_rejects_unsupported_default_mode() -> None:
    """Keep persisted mode values inside the workflow's controlled vocabulary."""
    with pytest.raises(ValueError, match="Unsupported default capture mode"):
        AppConfig(default_mode="arbitrary transcript text")


@pytest.mark.parametrize("global_recording_key", ["F0", "F25", "f9", "RightAlt", "CTRL+F9"])
def test_config_rejects_unsupported_global_recording_key(global_recording_key: str) -> None:
    """Persist only one unmodified function key supported by the settings picker."""
    with pytest.raises(ValueError, match="F1 through F24"):
        AppConfig(global_recording_key=global_recording_key)


def test_config_rejects_non_text_global_recording_key() -> None:
    """Reject malformed hand-edited values before portal registration."""
    with pytest.raises(TypeError, match="global_recording_key"):
        AppConfig(global_recording_key=9)  # type: ignore[arg-type]


def test_config_pins_scribe_v2_and_validates_optional_codex_model() -> None:
    """Keep hand-edited provider configuration inside the shipped integration contract."""
    with pytest.raises(ValueError, match="scribe_v2"):
        AppConfig(transcription_model="future_model")
    with pytest.raises(TypeError, match="codex_model"):
        AppConfig(codex_model=42)  # type: ignore[arg-type]
    with pytest.raises(ValueError, match="codex_model"):
        AppConfig(codex_model=" model-with-hidden-space")
    assert AppConfig(codex_model="gpt-5.4").codex_model == "gpt-5.4"


def test_config_requires_typed_audio_retention_policy() -> None:
    """Reject raw strings that would fail later at the frozen retention boundary."""
    with pytest.raises(TypeError, match="AudioRetentionPolicy"):
        AppConfig(audio_retention_policy="failures")  # type: ignore[arg-type]


@pytest.mark.parametrize("language_code", ["", "EN", "english", "sensitive transcript fragment"])
def test_config_rejects_invalid_scribe_language_code(language_code: str) -> None:
    """Send only auto-detection or documented ISO-639-1/3 values to Scribe."""
    with pytest.raises(ValueError, match="language_code"):
        AppConfig(language_code=language_code)


@pytest.mark.parametrize(
    "payload",
    [
        {"microphone_target": ""},
        {"microphone_target": " leading-space"},
        {"system_audio_target": "line\nbreak"},
        {"system_audio_target": 42},
    ],
)
def test_config_rejects_invalid_pipewire_targets(payload: dict[str, object]) -> None:
    """Keep selected node names bounded and safe as direct process arguments."""
    with pytest.raises((TypeError, ValueError)):
        AppConfig(**payload)


@pytest.mark.parametrize(
    "payload",
    [
        {"auto_paste": "yes"},
        {"spoken_commands_enabled": 1},
        {"remember_per_application": "yes"},
        {"incognito_mode": "no"},
        {"history_retention_days": True},
    ],
)
def test_config_rejects_malformed_general_and_privacy_types(payload: dict[str, object]) -> None:
    """Do not let hand-edited truthy strings silently change capture or persistence behavior."""
    with pytest.raises(TypeError):
        AppConfig(**payload)


def test_elevenlabs_api_key_uses_supported_names_in_precedence_order() -> None:
    """Prefer the standard name while accepting compatibility aliases."""
    environ = {
        "ELEVENLABS_API_KEY": "canonical-test-key",
        "DAS_ITEM_ELEVEN_LABS_API_KEY__CREDENTIAL": "scoped-test-key",
        "ELEVEN_LABS_STT_TOKEN": "legacy-test-key",
    }
    assert elevenlabs_api_key(environ) == "canonical-test-key"
    del environ["ELEVENLABS_API_KEY"]
    assert elevenlabs_api_key(environ) == "scoped-test-key"
    del environ["DAS_ITEM_ELEVEN_LABS_API_KEY__CREDENTIAL"]
    assert elevenlabs_api_key(environ) == "legacy-test-key"


def test_elevenlabs_api_key_rejects_missing_or_blank_credentials() -> None:
    """Fail with actionable guidance when no supported process credential exists."""
    environ = {"ELEVENLABS_API_KEY": "  "}
    with pytest.raises(RuntimeError, match="Set ELEVENLABS_API_KEY"):
        elevenlabs_api_key(environ)
