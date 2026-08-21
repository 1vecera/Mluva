"""Recovery coverage for unresolved Scratchpad drafts."""

from dataclasses import replace
from pathlib import Path

import pytest

from voice_scribe_linux.scratchpad import ScratchpadDraft, ScratchpadDraftStore


def test_scratchpad_round_trip_and_explicit_cleanup(tmp_path: Path) -> None:
    """Restore editable work after restart and erase audio only on explicit resolution."""
    audio_path = tmp_path / "recordings" / "draft.wav"
    audio_path.parent.mkdir()
    audio_path.write_bytes(b"RIFF-draft")
    store = ScratchpadDraftStore(tmp_path / "scratchpad.json")
    draft = ScratchpadDraft(
        identifier="draft-test",
        history_identifier="history-test",
        created_at="2026-08-13T10:00:00+00:00",
        raw_text="raw draft",
        text="Edited draft.",
        audio_path=str(audio_path),
    )
    store.save(draft)
    assert ScratchpadDraftStore(store.path).draft == draft
    assert store.path.stat().st_mode & 0o777 == 0o600
    store.clear(remove_audio=True)
    assert store.draft is None
    assert not store.path.exists()
    assert not audio_path.exists()


def test_scratchpad_refuses_to_delete_audio_outside_managed_recordings(tmp_path: Path) -> None:
    """Treat a persisted outside path as untrusted and preserve both the draft and target file."""
    audio_path = tmp_path.parent / "outside.wav"
    audio_path.write_bytes(b"RIFF-outside")
    store = ScratchpadDraftStore(tmp_path / "scratchpad.json")
    draft = ScratchpadDraft(
        identifier="draft-test",
        history_identifier="history-test",
        created_at="2026-08-13T10:00:00+00:00",
        raw_text="raw draft",
        text="Edited draft.",
        audio_path=str(audio_path),
    )
    store.save(draft)
    with pytest.raises(ValueError, match="outside"):
        store.clear(remove_audio=True)
    assert audio_path.exists()
    assert store.path.exists()
    assert store.draft == draft


def test_incognito_scratchpad_remains_memory_only(tmp_path: Path) -> None:
    """Keep private editing state alive for the process without writing restart state."""
    store = ScratchpadDraftStore(tmp_path / "scratchpad.json")
    temporary_path = store.path.with_suffix(".tmp")
    temporary_path.write_text("stale partial draft", encoding="utf-8")
    draft = ScratchpadDraft(
        identifier="incognito-draft",
        history_identifier=None,
        created_at="2026-08-13T10:00:00+00:00",
        raw_text="private",
        text="Private draft.",
        audio_path=None,
        incognito=True,
    )
    store.save(draft, persist=False)
    assert store.draft == draft
    assert not store.path.exists()
    assert not temporary_path.exists()
    store.save(replace(draft, text="Edited private draft."))
    assert store.draft is not None
    assert store.draft.text == "Edited private draft."
    assert not store.path.exists()
    assert ScratchpadDraftStore(store.path).draft is None


def test_malformed_scratchpad_is_preserved_and_future_writes_fail_closed(tmp_path: Path) -> None:
    """Keep corrupt recovery evidence intact instead of crashing startup or overwriting it."""
    path = tmp_path / "scratchpad.json"
    malformed = '{"identifier": "unfinished"'
    path.write_text(malformed, encoding="utf-8")

    store = ScratchpadDraftStore(path)

    assert store.draft is None
    assert store.persistence_error is not None
    assert path.read_text(encoding="utf-8") == malformed
    replacement = ScratchpadDraft(
        identifier="replacement",
        history_identifier=None,
        created_at="2026-08-18T10:00:00+00:00",
        raw_text="raw",
        text="replacement",
        audio_path=None,
    )
    with pytest.raises(RuntimeError, match="malformed recovery document"):
        store.save(replacement)
    assert path.read_text(encoding="utf-8") == malformed
