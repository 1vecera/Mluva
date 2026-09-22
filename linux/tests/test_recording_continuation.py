"""Protect edits, immutable raw segments and retention when recording continues."""

import json
import sqlite3
from datetime import UTC, datetime, timedelta

import pytest

from mluva_linux.conversation import ConversationStore
from mluva_linux.history import HistoryStore


@pytest.fixture
def conversation(tmp_path):
    """Create a real saved conversation and a separate finalized capture."""
    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    store = ConversationStore(history)
    store.initialize()
    original = history.add("First raw words.", "First words.", "dictation", "eng", None, "copied")
    segment = history.add("More raw words.", "More words.", "dictation", "eng", None, "ready")
    return history, store, original, segment


def test_continue_preserves_edits_and_raw_captures_across_restart(conversation, tmp_path):
    """Append to both working documents, preserving each immutable source and prior rewrites."""
    history, store, original, segment = conversation
    earlier = store.append(original.identifier, "Polish", "Earlier rewrite.", "fixture")
    latest = store.append(original.identifier, "Shorten", "Latest rewrite.", "fixture")
    store.save_text(original.identifier, "My edited source.")
    store.save_text(original.identifier, "My edited rewrite.", latest.identifier)
    assert store.append_recording(original.identifier, segment) == "My edited rewrite.\n\nMore words."
    reopened = ConversationStore(HistoryStore(history.path))
    assert reopened.source_text(original) == "My edited source.\n\nMore raw words."
    assert reopened.replies(original.identifier)[0] == earlier
    assert reopened.replies(original.identifier)[-1].text == "My edited rewrite.\n\nMore words."
    assert history.find(original.identifier) == original
    assert history.find(segment.identifier) == segment
    assert [entry.identifier for entry in reopened.search()] == [original.identifier]
    assert [entry.identifier for entry in reopened.search("More raw")] == [original.identifier]
    assert history.recording_conversation(segment.identifier) == original
    exported = history.export(original, tmp_path / "exports", "json", source_text=reopened.source_text(original))
    assert json.loads(exported.read_text())["recording_segments"][0]["raw_text"] == "More raw words."


def test_failed_append_keeps_both_recordings_and_does_not_duplicate_text(conversation):
    """Reject duplicate and oversized appends without committing half a conversation."""
    history, store, original, segment = conversation
    store.save_text(original.identifier, "a" * 120_000)
    with pytest.raises(ValueError, match="full"):
        store.append_recording(original.identifier, segment)
    assert history.continuations(original.identifier) == []
    assert len(store.source_text(original)) == 120_000
    store.save_text(original.identifier, "First edited.")
    assert store.append_recording(original.identifier, segment) == "First edited.\n\nMore raw words."
    with pytest.raises(ValueError, match="already"):
        store.append_recording(original.identifier, segment)
    assert store.source_text(original) == "First edited.\n\nMore raw words."


def test_deleting_conversation_erases_all_segment_audio(conversation):
    """A continued conversation has one deletion boundary including retained audio."""
    history, store, original, segment = conversation
    store.append_recording(original.identifier, segment)
    audio = history.path.parent / "recordings" / "segment.wav"
    audio.parent.mkdir()
    audio.write_bytes(b"synthetic")
    with sqlite3.connect(history.path) as connection:
        connection.execute(
            "UPDATE transcription_history SET retained_audio_path = ? WHERE identifier = ?",
            (str(audio), segment.identifier),
        )
    history.delete(original.identifier)
    assert not audio.exists()
    assert history.recent() == []
    with pytest.raises(KeyError):
        store.append_recording(original.identifier, segment)


def test_retention_uses_the_latest_segment_and_preserves_active_conversation(conversation):
    """Do not prune a recently continued note or leave orphan segments after it expires."""
    history, store, original, segment = conversation
    store.append_recording(original.identifier, segment)
    now = datetime.now(UTC)
    with sqlite3.connect(history.path) as connection:
        connection.execute(
            "UPDATE transcription_history SET created_at = ? WHERE identifier = ?",
            ((now - timedelta(days=20)).isoformat(), original.identifier),
        )
    assert history.prune_older_than(7, now=now) == 0
    future = now + timedelta(days=10)
    assert history.prune_older_than(7, now=future, excluded_identifiers=frozenset({original.identifier})) == 0
    assert history.prune_older_than(7, now=future) == 2
    assert history.recent() == []


def test_continued_conversation_returns_to_the_top_of_history(conversation):
    """Latest-conversation shortcuts must return the continued note rather than an unrelated newer root."""
    history, store, original, segment = conversation
    history.add("Another note.", "Another note.", "dictation", "eng", None, "ready")
    with sqlite3.connect(history.path) as connection:
        connection.execute(
            "UPDATE transcription_history SET created_at = ? WHERE identifier = ?",
            ((datetime.now(UTC) + timedelta(seconds=1)).isoformat(), segment.identifier),
        )
    store.append_recording(original.identifier, history.find(segment.identifier))
    assert store.search(limit=1)[0].identifier == original.identifier
