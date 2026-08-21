"""Review-only vocabulary suggestion coverage for explicit history corrections."""

from pathlib import Path

from voice_scribe_linux.history import HistoryEntry, HistoryStore
from voice_scribe_linux.personalization import PersonalizationStore
from voice_scribe_linux.vocabulary import VocabularySuggestionEngine


def _add_correction(
    store: HistoryStore,
    delivered_text: str,
    corrected_text: str,
    application_identifier: str | None = None,
) -> HistoryEntry:
    """Create one persisted explicit correction for a suggestion test."""
    entry = store.add(
        raw_text=delivered_text,
        delivered_text=delivered_text,
        mode="dictation",
        language_code="eng",
        transcription_id=None,
        delivery_outcome="copied",
        application_identifier=application_identifier,
    )
    return store.correct_delivered_text(entry.identifier, corrected_text)


def test_focused_manual_correction_suggests_only_changed_phrase(tmp_path: Path) -> None:
    """Keep shared context out of the proposed scoped vocabulary rule."""
    store = HistoryStore(tmp_path / "history.sqlite3")
    store.initialize()
    entry = _add_correction(
        store,
        "Use post grass in production.",
        "Use Postgres in production.",
        application_identifier="/usr/bin/code",
    )

    suggestion = VocabularySuggestionEngine().suggestions((entry,))[0]

    assert suggestion.spoken == "post grass"
    assert suggestion.written == "Postgres"
    assert suggestion.application_identifier == "/usr/bin/code"
    assert suggestion.occurrences == 1
    assert suggestion.identifier == "/usr/bin/code\x1fpost grass\x1fPostgres"


def test_repeated_corrections_combine_while_broad_rewrites_stay_out(tmp_path: Path) -> None:
    """Rank repeated small edits without treating a rewritten document as vocabulary."""
    store = HistoryStore(tmp_path / "history.sqlite3")
    store.initialize()
    first = _add_correction(store, "Ship through cube control.", "Ship through kubectl.")
    second = _add_correction(store, "Run cube control now.", "Run kubectl now.")
    rewrite = _add_correction(
        store,
        "This is a complete sentence that needs work.",
        "Rewrite the entire idea in a different way.",
    )

    suggestions = VocabularySuggestionEngine().suggestions((first, second, rewrite))

    assert len(suggestions) == 1
    assert suggestions[0].spoken == "cube control"
    assert suggestions[0].written == "kubectl"
    assert suggestions[0].occurrences == 2


def test_existing_and_dismissed_replacements_are_not_suggested(tmp_path: Path) -> None:
    """Honor both explicit dictionary ownership and durable dismissal decisions."""
    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    entry = _add_correction(
        history,
        "Use post grass here.",
        "Use Postgres here.",
        application_identifier="/usr/bin/code",
    )
    engine = VocabularySuggestionEngine()
    suggestion = engine.suggestions((entry,))[0]
    personalization = PersonalizationStore(tmp_path / "personalization.json")
    unrelated_scope = personalization.save_dictionary_replacement(
        "post grass",
        "Unrelated",
        application_identifier="/usr/bin/mail",
    )

    assert engine.suggestions((entry,), dictionary=(unrelated_scope,)) == (suggestion,)
    global_rule = personalization.save_dictionary_replacement("post grass", "Postgres")
    assert engine.suggestions((entry,), dictionary=(global_rule,)) == ()
    assert engine.suggestions((entry,), dismissed_identifiers={suggestion.identifier}) == ()


def test_technical_punctuation_remains_part_of_suggestion(tmp_path: Path) -> None:
    """Trim sentence punctuation while preserving punctuation inside a technical term."""
    store = HistoryStore(tmp_path / "history.sqlite3")
    store.initialize()
    entry = _add_correction(
        store,
        "Compile with C plus plus today.",
        "Compile with C++ today.",
    )

    suggestion = VocabularySuggestionEngine().suggestions((entry,))[0]

    assert suggestion.spoken == "C plus plus"
    assert suggestion.written == "C++"


def test_ordinary_transformation_and_oversized_edit_never_become_suggestions(tmp_path: Path) -> None:
    """Require explicit correction metadata and reject a changed phrase beyond four words."""
    store = HistoryStore(tmp_path / "history.sqlite3")
    store.initialize()
    transformed = store.add(
        raw_text="say new paragraph here",
        delivered_text="Say\n\nhere",
        mode="dictation",
        language_code="eng",
        transcription_id=None,
        delivery_outcome="copied",
    )
    oversized = _add_correction(
        store,
        "Keep one two three four five after.",
        "Keep alpha beta gamma delta epsilon after.",
    )

    assert VocabularySuggestionEngine().suggestions((transformed, oversized)) == ()
