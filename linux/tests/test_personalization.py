"""Headless coverage for deterministic personalization and its local document."""

import json
from pathlib import Path

import pytest

from voice_scribe_linux.personalization import (
    BUILT_IN_STYLES,
    DictionaryCaseBehavior,
    PersonalizationStore,
    integrity_violations,
)


def test_dictionary_and_snippets_persist_owner_only(tmp_path: Path) -> None:
    """Persist compatible dictionary and snippet fields under owner-only permissions."""
    path = tmp_path / "private" / "personalization.json"
    store = PersonalizationStore(path)

    store.save_dictionary_replacement(
        "example value",
        "replacement text",
        case_behavior=DictionaryCaseBehavior.MATCH_SPOKEN,
    )
    store.save_snippet(
        "email signoff",
        "Best,\nDaniel",
        typed_trigger=";signoff",
    )

    reloaded = PersonalizationStore(path)
    assert reloaded.persistence_error is None
    assert reloaded.dictionary[0].case_behavior is DictionaryCaseBehavior.MATCH_SPOKEN
    assert reloaded.snippets[0].typed_trigger == ";signoff"
    assert reloaded.snippets[0].expansion == "Best,\nDaniel"
    assert path.stat().st_mode & 0o777 == 0o600
    assert path.parent.stat().st_mode & 0o777 == 0o700
    payload = json.loads(path.read_text(encoding="utf-8"))
    assert payload["dictionary"][0]["caseBehavior"] == "matchSpoken"
    assert payload["snippets"][0]["typedTrigger"] == ";signoff"


def test_vocabulary_suggestion_dismissal_persists_without_learning(tmp_path: Path) -> None:
    """Store an explicit dismissal while leaving the replacement dictionary unchanged."""
    path = tmp_path / "personalization.json"
    store = PersonalizationStore(path)
    identifier = "/usr/bin/code\x1fpost grass\x1fPostgres"

    store.dismiss_vocabulary_suggestion(identifier)
    reloaded = PersonalizationStore(path)

    assert reloaded.dismissed_vocabulary_suggestion_identifiers == {identifier}
    assert reloaded.dictionary == []
    assert json.loads(path.read_text(encoding="utf-8"))["dismissedVocabularySuggestionIDs"] == [identifier]
    with pytest.raises(ValueError, match="cannot be empty"):
        reloaded.dismiss_vocabulary_suggestion("")


def test_personalization_is_exact_ordered_and_preserves_immutable_input(tmp_path: Path) -> None:
    """Apply longest whole-phrase rules before explicit spoken snippets without mutating source."""
    store = PersonalizationStore(tmp_path / "personalization.json")
    store.save_dictionary_replacement("project", "wrong-short-match")
    store.save_dictionary_replacement("project mluva", "Mluva")
    store.save_dictionary_replacement(
        "example value",
        "replacement text",
        case_behavior=DictionaryCaseBehavior.MATCH_SPOKEN,
    )
    store.save_snippet("daily stamp", "{{date}} at {{time}} {{unknown}} costs $5")
    raw = (
        "PROJECT MLUVA and projected mluvas. EXAMPLE VALUE, Example value, example value. "
        "daily stamp; snippet daily stamp"
    )

    result = store.process_transcript(
        raw,
        application_identifier=None,
        variables={"date": "July 31, 2026", "time": "14:30"},
    )

    assert raw.endswith("daily stamp; snippet daily stamp")
    assert result == (
        "Mluva and projected mluvas. REPLACEMENT TEXT, Replacement text, replacement text. "
        "daily stamp; July 31, 2026 at 14:30 {{unknown}} costs $5"
    )


def test_application_rules_override_global_rules_without_leaking_to_other_apps(tmp_path: Path) -> None:
    """Prefer same-trigger application rules while retaining unrelated global rules."""
    store = PersonalizationStore(tmp_path / "personalization.json")
    store.save_dictionary_replacement("ship it", "Ship it")
    store.save_dictionary_replacement("cloud", "Cloud")
    store.save_dictionary_replacement("ship it", "git push", application_identifier="/usr/bin/code")
    store.save_snippet("review", "Please review", typed_trigger=";review")
    store.save_snippet(
        "review",
        "gh pr view --web",
        typed_trigger=";review",
        application_identifier="/usr/bin/code",
    )

    editor = store.process_transcript(
        "ship it cloud snippet review",
        "/usr/bin/code",
        variables={},
    )
    mail = store.process_transcript(
        "ship it cloud snippet review",
        "/usr/bin/mail",
        variables={},
    )

    assert editor == "git push Cloud gh pr view --web"
    assert mail == "Ship it Cloud Please review"
    assert store.expand_typed_trigger(";review", "/usr/bin/code", {}) == "gh pr view --web"
    assert store.expand_typed_trigger(";review", "/usr/bin/mail", {}) == "Please review"
    assert store.expand_typed_trigger(";Review", "/usr/bin/mail", {}) is None


def test_upserts_are_scoped_and_typed_triggers_are_single_tokens(tmp_path: Path) -> None:
    """Update duplicate scoped keys and reject ambiguous multi-token typed triggers."""
    store = PersonalizationStore(tmp_path / "personalization.json")
    first = store.save_dictionary_replacement("g c p", "GCP")
    updated = store.save_dictionary_replacement("G C P", "Google Cloud")
    first_snippet = store.save_snippet("short signature", "First", typed_trigger=";sig")
    updated_snippet = store.save_snippet("full signature", "Second", typed_trigger=";sig")

    assert updated.identifier == first.identifier
    assert store.dictionary == [updated]
    assert updated_snippet.identifier == first_snippet.identifier
    assert store.snippets == [updated_snippet]
    with pytest.raises(ValueError, match="cannot contain whitespace"):
        store.save_snippet("signature", "Signature", typed_trigger="two words")


def test_built_in_and_custom_styles_survive_restart_and_remain_distinct(tmp_path: Path) -> None:
    """Reconstruct immutable presets while persisting custom CRUD and scoped selections."""
    path = tmp_path / "personalization.json"
    store = PersonalizationStore(path)
    assert [style.name for style in store.styles] == [
        "Message",
        "Google Chat",
        "Tasks",
        "Email",
        "Prose",
        "Technical notes",
        "Prompt",
    ]
    custom = store.save_style("Release note", "Create a concise customer-facing release note.")
    custom = store.update_style(
        custom.identifier,
        "Customer release note",
        "Create a concise customer-facing release note and preserve every stated limit.",
    )
    email = next(style for style in BUILT_IN_STYLES if style.name == "Email")
    store.select_style(email.identifier, "/usr/bin/mail", remember_per_application=True)
    store.select_style(None, "/usr/bin/plain-editor", remember_per_application=True)
    store.select_style(custom.identifier, None, remember_per_application=False)

    reloaded = PersonalizationStore(path)
    assert reloaded.selected_style("/usr/bin/mail", remember_per_application=True) == email
    assert reloaded.selected_style("/usr/bin/code", remember_per_application=True) is None
    assert reloaded.has_application_style_selection("/usr/bin/plain-editor")
    assert reloaded.selected_style("/usr/bin/plain-editor", remember_per_application=True) is None
    assert reloaded.selected_style(None, remember_per_application=False) == custom
    assert len(json.loads(path.read_text(encoding="utf-8"))["styles"]) == 1
    with pytest.raises(ValueError, match="Built-in"):
        reloaded.save_style("Email", "Replace the preset")
    with pytest.raises(KeyError):
        reloaded.delete_style(email.identifier)


def test_mode_profiles_are_opt_in_and_local_to_concrete_applications(tmp_path: Path) -> None:
    """Remember mode only for an identified application when the setting is enabled."""
    path = tmp_path / "personalization.json"
    store = PersonalizationStore(path)
    store.select_mode("command", "/usr/bin/mail", remember_per_application=True)
    store.select_mode("scratchpad", "/usr/bin/code", remember_per_application=False)

    reloaded = PersonalizationStore(path)
    assert reloaded.selected_mode("/usr/bin/mail", True, "dictation") == "command"
    assert reloaded.selected_mode("/usr/bin/code", True, "dictation") == "dictation"
    assert reloaded.selected_mode("/usr/bin/mail", False, "scratchpad") == "scratchpad"


def test_integrity_validator_protects_meaning_critical_tokens() -> None:
    """Reject a fluent candidate that drops exact technical facts and negation."""
    source = "Do not deploy PostgreSQL 17 to https://example.com/api at /srv/voice_scribe with --dry-run and requestID."
    candidate = "Deploy PostgreSQL to https://example.org at /srv with dry run and requestId."

    assert integrity_violations(source, candidate, ("PostgreSQL",)) == (
        "--dry-run",
        "/srv/voice_scribe",
        "17",
        "Do not",
        "https://example.com/api",
        "requestID",
    )
    assert (
        integrity_violations(
            source,
            "Do not deploy PostgreSQL 17 to https://example.com/api at /srv/voice_scribe with --dry-run and requestID.",
            ("PostgreSQL",),
        )
        == ()
    )


def test_malformed_document_is_preserved_and_mutation_fails_closed(tmp_path: Path) -> None:
    """Do not silently overwrite a malformed durable personalization document."""
    path = tmp_path / "personalization.json"
    path.write_text("{not-json", encoding="utf-8")
    original = path.read_bytes()
    store = PersonalizationStore(path)

    assert store.persistence_error is not None
    assert store.dictionary == []
    with pytest.raises(RuntimeError, match="malformed document"):
        store.save_dictionary_replacement("project mluva", "Mluva")
    assert path.read_bytes() == original
