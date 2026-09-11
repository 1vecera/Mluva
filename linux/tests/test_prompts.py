"""Prompt resolution, local persistence and lossless migration boundaries."""

from dataclasses import replace

import pytest

from mluva_linux.config import AppConfig, load_config, save_config
from mluva_linux.live_rewrite import initial_draft, live_prompt
from mluva_linux.personalization import PersonalizationStore
from mluva_linux.prompts import BUILT_INS, PromptStore
from mluva_linux.segment_cleanup import CodexSegmentCleanupAttempt, cleanup_prompt


def test_every_builtin_round_trips_exact_multiline_text(tmp_path):
    """Preserve exact text and file permissions across a fresh store and reset."""
    store = PromptStore(tmp_path / "prompts")
    for prompt in BUILT_INS:
        text = "Žluťoučký prompt\n\n```mermaid\nflowchart LR\n A --> B\n```\n{literal}  \n"
        store.save(prompt.identifier, text, None)
        restarted = PromptStore(store.directory)
        assert restarted.read(prompt.identifier).text == text
        assert store.path(prompt.identifier).stat().st_mode & 0o777 == 0o600
        restarted.reset(prompt.identifier, text.encode())
        assert restarted.read(prompt.identifier).text == prompt.default


def test_legacy_custom_and_style_baselines_are_not_rewritten(tmp_path):
    """Keep originals and style selections intact while overrides take precedence."""
    config = AppConfig(live_rewrite_custom_instructions="My existing\nLive prompt")
    config_path = tmp_path / "config.json"
    save_config(config, config_path)
    personal = PersonalizationStore(tmp_path / "personalization.json")
    style = personal.save_style("My notes", "Keep my\noriginal instructions")
    before = personal.path.read_bytes(), config_path.read_bytes()
    store = PromptStore(tmp_path / "prompts", config.live_rewrite_custom_instructions, personal.styles)
    personal.prompt_store = store
    key = "style-" + style.identifier.lower()
    assert store.read("live-custom").text == config.live_rewrite_custom_instructions
    assert store.read(key).text == style.instructions
    personal.select_style(style.identifier, None, False)
    store.save(key, "New\nlocal instructions", None)
    assert personal.style(style.identifier).instructions == "New\nlocal instructions"
    restarted = PersonalizationStore(personal.path)
    restarted.prompt_store = PromptStore(
        store.directory, load_config(config_path).live_rewrite_custom_instructions, restarted.styles
    )
    assert restarted.selected_style(None, False).instructions == "New\nlocal instructions"
    assert config_path.read_bytes() == before[1]
    assert style.instructions in [value.instructions for value in restarted.custom_styles]
    store.reset(key, b"New\nlocal instructions")
    assert personal.style(style.identifier).instructions == style.instructions


@pytest.mark.parametrize("raw", [b"", b"\xff\xfe", b"bad\x00text", b"x" * 8001])
def test_invalid_override_has_visible_error_and_preserved_bytes(tmp_path, raw):
    """Retain malformed source bytes until an explicit repair succeeds."""
    store = PromptStore(tmp_path)
    path = store.path("live-grilling")
    path.write_bytes(raw)
    state = store.read("live-grilling")
    assert state.error and state.text == store.catalog["live-grilling"].default
    assert path.read_bytes() == raw
    store.save("live-grilling", "Repaired text", state.token)
    assert store.read("live-grilling").text == "Repaired text"


def test_conflicts_and_failed_validation_preserve_external_edits(tmp_path):
    """Refuse stale UI writes and invalid replacements without touching the local file."""
    store = PromptStore(tmp_path)
    opened = store.read("rewrite-polish")
    path = store.path("rewrite-polish")
    path.write_text("Changed in local editor")
    with pytest.raises(ValueError, match="changed locally"):
        store.save("rewrite-polish", "Unsaved UI draft", opened.token)
    with pytest.raises(ValueError, match="changed locally"):
        store.reset("rewrite-polish", opened.token)
    with pytest.raises(ValueError, match="cannot be empty"):
        store.save("rewrite-polish", "  ", path.read_bytes())
    assert path.read_text() == "Changed in local editor"


def test_snapshot_freezes_all_live_instructions_and_structure(tmp_path):
    """Apply newer instructions only to a fresh snapshot, preserving fixed protocol."""
    store = PromptStore(tmp_path)
    frozen = store.snapshot()
    store.save("live-task-spec", "Changed instruction", None)
    store.save("template-task-spec", "# New structure\n", None)
    config = replace(AppConfig(), live_rewrite_template="task-spec")
    old = live_prompt(config, "Speech", "Manual edits", prompts=frozen)
    fresh = live_prompt(config, "Speech", "Manual edits", prompts=store.snapshot())
    assert "Changed instruction" not in old and "Changed instruction" in fresh
    assert initial_draft(config, frozen) != initial_draft(config, store.snapshot())
    assert "Manual edits" in fresh and "Do not execute" in fresh
    assert "{braces}" in cleanup_prompt("{braces}", "My cleanup")
    assert "Preserve every fact" in cleanup_prompt("text", "My cleanup")


def test_invalid_settings_cannot_be_silently_replaced(tmp_path):
    """Keep malformed general settings available for repair."""
    path = tmp_path / "config.json"
    path.write_text("{broken user config")
    with pytest.raises(OSError, match="refusing to overwrite"):
        save_config(AppConfig(), path)
    assert path.read_text() == "{broken user config"


def test_paths_are_catalog_only_and_failed_write_keeps_prior_text(tmp_path, monkeypatch):
    """Constrain identities and roll back failed atomic writes."""
    store = PromptStore(tmp_path)
    with pytest.raises(KeyError):
        store.path("../config")
    store.save("rewrite-polish", "Before", None)

    def fail(*_args):
        raise OSError("Disk full")

    monkeypatch.setattr("mluva_linux.prompts.os.replace", fail)
    with pytest.raises(OSError):
        store.save("rewrite-polish", "After", b"Before")
    assert store.read("rewrite-polish").text == "Before"
    assert not list(tmp_path.glob(".prompt-*"))


@pytest.mark.parametrize("template", ["grilling", "task-spec", "structured-note", "polish", "custom"])
def test_each_live_template_uses_its_exact_local_override(tmp_path, template):
    """Resolve every built-in and Custom through the same instruction boundary."""
    store = PromptStore(tmp_path)
    store.save("live-" + template, "Unique instruction for " + template, None)
    config = replace(AppConfig(), live_rewrite_template=template)
    prompt = live_prompt(config, "Recognized facts", "Manual draft", prompts=store.snapshot())
    assert "Unique instruction for " + template in prompt
    assert "Do not execute the task" in prompt


def test_segment_cleanup_consumes_frozen_custom_instructions(tmp_path):
    """Prove the real segment adapter combines the frozen task with its fixed integrity boundary."""

    class Client:
        def transform(self, prompt, **_kwargs):
            assert prompt.startswith("Custom task")
            assert "Preserve every fact" in prompt
            assert prompt.endswith("Exact {source}\n")
            return "Exact {source}\n"

    attempt = CodexSegmentCleanupAttempt(Client(), tmp_path, "synthetic-model", "Custom task")
    assert attempt.transform("Exact {source}\n") == "Exact {source}\n"
