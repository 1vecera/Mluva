"""Persistence, privacy and bounded-context contracts for local rewrite conversations."""

import json
import sqlite3
from pathlib import Path

import pytest

from mluva_linux.conversation import QUICK_POLISH, STRUCTURED_NOTE, ConversationStore, rewrite_prompt
from mluva_linux.history import HistoryStore


def test_existing_history_becomes_a_restart_safe_conversation(tmp_path: Path) -> None:
    """Open pre-existing sources, replay follow-ups and preserve the original through restart."""
    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    source = history.add(
        "Příliš žluťoučký kůň, um, hello.", "Příliš žluťoučký kůň, hello.", "dictation", "ces", None, "copied"
    )
    conversations = ConversationStore(history)
    conversations.initialize()
    conversations.append(source.identifier, QUICK_POLISH, "Příliš žluťoučký kůň, hello.", "fixture-model")
    reopened = ConversationStore(HistoryStore(history.path))
    reopened.initialize()
    replies = reopened.replies(source.identifier)
    prompt = rewrite_prompt(history.find(source.identifier), replies, "Make it shorter.")
    context = json.loads(prompt.split("\n", 1)[1])
    assert context["original_transcript"] == source.raw_text
    assert context["completed_rewrites"][0]["instruction"] == QUICK_POLISH
    assert context["next_instruction"] == "Make it shorter."
    assert history.find(source.identifier) == source
    assert reopened.search("Příliš")[0].identifier == source.identifier
    assert reopened.search("PŘÍLIŠ")[0].identifier == source.identifier
    assert reopened.search("ŽLUŤOUČKÝ")[0].identifier == source.identifier


def test_searches_old_sources_and_replies_without_treating_wildcards_as_query_syntax(tmp_path: Path) -> None:
    """Search the database before limiting results and handle literal SQL wildcards."""
    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    store = ConversationStore(history)
    store.initialize()
    old = history.add("old original", "old original", "dictation", "eng", None, "copied")
    store.append(old.identifier, STRUCTURED_NOTE, "Ship the 25% improvement.", "fixture-model")
    for index in range(90):
        history.add(f"New {index}", f"New {index}", "dictation", "eng", None, "copied")
    assert len(store.search()) == 80
    assert [entry.identifier for entry in store.search("25%")] == [old.identifier]
    assert [entry.identifier for entry in store.search("old original")] == [old.identifier]
    assert store.search("_") == []


def test_deletion_erases_rewrites_and_a_late_completion_cannot_resurrect_them(tmp_path: Path) -> None:
    """Use real SQLite deletion to prove retention and late-worker writes share the source lifecycle."""
    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    store = ConversationStore(history)
    store.initialize()
    source = history.add("Original", "Original", "dictation", "eng", None, "copied")
    store.append(source.identifier, "Polish", "Polished", "fixture-model")
    with sqlite3.connect(history.path) as connection:
        connection.execute("DELETE FROM transcription_history WHERE identifier = ?", (source.identifier,))
    assert store.replies(source.identifier) == []
    with pytest.raises(sqlite3.IntegrityError):
        store.append(source.identifier, "Late", "Must not reappear", "fixture-model")
    assert store.replies(source.identifier) == []


def test_oversized_context_fails_explicitly_instead_of_dropping_words(tmp_path: Path) -> None:
    """Keep display text intact while refusing transformations beyond the documented context budget."""
    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    source = history.add("Long text " * 20_000, "Long text " * 20_000, "dictation", "eng", None, "copied")
    with pytest.raises(ValueError, match="too long"):
        rewrite_prompt(source, [], QUICK_POLISH)
    assert history.find(source.identifier).raw_text == source.raw_text
