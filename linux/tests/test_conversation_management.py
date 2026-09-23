"""Preserve every document and recording when chats are joined or removed."""

import json
import sqlite3
from datetime import UTC, datetime, timedelta

import pytest

from mluva_linux.conversation import ConversationStore, rewrite_prompt
from mluva_linux.conversation_titles import save_generated_title
from mluva_linux.history import HistoryStore


@pytest.fixture
def chats(tmp_path):
    """Give each scenario real persisted chats with different original and prepared text."""
    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    store = ConversationStore(history)
    store.initialize()
    first = history.add("First raw.", "First prepared.", "dictation", "eng", None, "copied")
    second = history.add("Second raw.", "Second prepared.", "dictation", "eng", None, "copied")
    return history, store, first, second


def test_merge_preserves_edits_replies_originals_and_followup_context(chats, tmp_path):
    """Restart and export must retain both sources and all replies, with a combined latest version."""
    history, store, first, second = chats
    history.update_title(first.identifier, "Combined project")
    history.update_title(second.identifier, "Žluťoučký 25%")
    store.save_text(first.identifier, "First edited.")
    store.save_text(second.identifier, "Second edited.")
    first_reply = store.append(first.identifier, "Polish first", "First polished.", "first-model")
    second_reply = store.append(second.identifier, "Polish second", "Second polished.", "second-model")
    store.save_text(second.identifier, "Second rewrite edited.", second_reply.identifier)

    merged = store.merge(first.identifier, second.identifier)
    reopened = ConversationStore(HistoryStore(history.path))
    reopened.initialize()
    assert merged.title == "Combined project"
    assert merged.raw_text == "First raw."
    assert history.find(second.identifier).raw_text == "Second raw."
    assert reopened.source_text(merged) == "First edited.\n\nSecond edited."
    replies = reopened.replies(first.identifier)
    assert replies[0] == first_reply
    assert replies[1].identifier == second_reply.identifier
    assert replies[1].model == "second-model" and replies[1].created_at == second_reply.created_at
    assert replies[1].text == "Second rewrite edited."
    assert replies[-1].text == "First polished.\n\nSecond rewrite edited."
    assert [entry.identifier for entry in reopened.search()] == [first.identifier]
    for query in ("Second raw", "Second edited", "Polish second", "ŽLUŤOUČKÝ 25%"):
        assert [entry.identifier for entry in reopened.search(query)] == [first.identifier]
    context = json.loads(rewrite_prompt(merged, replies, "Shorten", reopened.source_text(merged)).split("\n", 1)[1])
    assert context["completed_rewrites"][-1]["text"] == "First polished.\n\nSecond rewrite edited."
    exported = history.export(
        merged, tmp_path / "exports", "json", rewrites=replies, source_text=reopened.source_text(merged)
    )
    document = json.loads(exported.read_text())
    assert document["recording_segments"][0]["raw_text"] == "Second raw."
    assert len(document["rewrites"]) == 3
    assert not save_generated_title(history, first.identifier, "Late title", merged.title)
    assert not save_generated_title(history, second.identifier, "Another late title", "Žluťoučký 25%")


def test_merge_of_continued_chats_keeps_one_retention_and_deletion_boundary(chats):
    """Flatten existing groups without losing recordings; pruning and deletion still own every file."""
    history, store, first, second = chats
    first_segment = history.add("First extra.", "First extra.", "dictation", "eng", None, "ready")
    second_segment = history.add("Second extra.", "Second extra.", "dictation", "eng", None, "ready")
    store.append_recording(first.identifier, first_segment)
    store.append_recording(second.identifier, second_segment)
    now = datetime.now(UTC)
    recordings = history.path.parent / "recordings"
    recordings.mkdir()
    entries = (first, second, first_segment, second_segment)
    with sqlite3.connect(history.path) as connection:
        for entry in entries:
            path = recordings / f"{entry.identifier}.wav"
            path.write_bytes(b"synthetic audio")
            connection.execute(
                "UPDATE transcription_history SET retained_audio_path = ?, created_at = ? WHERE identifier = ?",
                (str(path), (now - timedelta(days=20)).isoformat(), entry.identifier),
            )
        connection.execute(
            "UPDATE transcription_history SET created_at = ? WHERE identifier = ?",
            (now.isoformat(), second_segment.identifier),
        )
    store.merge(first.identifier, second.identifier)
    assert len(history.continuations(first.identifier)) == 3
    assert history.continuations(second.identifier) == []
    assert history.recording_conversation(second_segment.identifier).identifier == first.identifier
    assert store.source_text(first) == "First raw.\n\nFirst extra.\n\nSecond raw.\n\nSecond extra."
    assert history.prune_older_than(7, now=now) == 0
    history.delete(first.identifier)
    assert history.recent() == [] and list(recordings.iterdir()) == []
    with sqlite3.connect(history.path) as connection:
        assert connection.execute("SELECT COUNT(*) FROM conversation_sources").fetchone()[0] == 0
        assert connection.execute("SELECT COUNT(*) FROM conversation_rewrites").fetchone()[0] == 0


def test_failed_merge_rolls_back_documents_and_replies(chats):
    """A failure after moving replies must roll back the entire join, without partial edits."""
    history, store, first, second = chats
    reply = store.append(second.identifier, "Polish", "Saved reply", "fixture")
    with sqlite3.connect(history.path) as connection:
        connection.execute(
            "CREATE TRIGGER fail_merge BEFORE INSERT ON recording_continuations "
            "BEGIN SELECT RAISE(ABORT, 'synthetic write failure'); END"
        )
    with pytest.raises(sqlite3.IntegrityError, match="synthetic write failure"):
        store.merge(first.identifier, second.identifier)
    assert store.source_text(first) == "First raw."
    assert store.source_text(second) == "Second raw."
    assert store.replies(first.identifier) == []
    assert store.replies(second.identifier) == [reply]
    assert len(store.search()) == 2


def test_invalid_and_repeated_merges_leave_chats_recoverable(chats):
    """Reject self, missing, oversized and absorbed targets without losing the still-valid chats."""
    history, store, first, second = chats
    with pytest.raises(ValueError, match="different"):
        store.merge(first.identifier, first.identifier)
    with pytest.raises(KeyError):
        store.merge(first.identifier, "missing")
    store.save_text(first.identifier, "a" * 120_000)
    with pytest.raises(ValueError, match="exceeds"):
        store.merge(first.identifier, second.identifier)
    assert len(store.search()) == 2 and history.continuations(first.identifier) == []
    store.save_text(first.identifier, "First edited.")
    store.merge(first.identifier, second.identifier)
    assert store.replies(first.identifier)[-1].text == "First edited.\n\nSecond prepared."
    with pytest.raises(ValueError, match="separate"):
        store.merge(first.identifier, second.identifier)
    with pytest.raises(ValueError, match="separate"):
        store.merge(second.identifier, first.identifier)
    assert store.source_text(first) == "First edited.\n\nSecond raw."


def test_merge_picker_filters_before_limiting_and_searches_old_chats(chats):
    """A page of unrelated history must not hide eligible destinations or archived search matches."""
    history, store, first, second = chats
    for _ in range(85):
        history.add("Command", "Command", "command", "eng", None, "ready")
    assert [entry.identifier for entry in store.search(merge_target_for=first.identifier)] == [second.identifier]
    command = history.recent(1)[0]
    with pytest.raises(ValueError, match="dictation"):
        store.merge(first.identifier, command.identifier)
    for index in range(85):
        history.add(f"Recent {index}", f"Recent {index}", "dictation", "eng", None, "ready")
    assert [entry.identifier for entry in store.search("Second", merge_target_for=first.identifier)] == [
        second.identifier
    ]
