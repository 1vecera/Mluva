"""Persistence and deletion coverage for local transcript history."""

import sqlite3
from contextlib import closing
from datetime import UTC, datetime, timedelta
from pathlib import Path

import pytest

import voice_scribe_linux.history as history_module
from voice_scribe_linux.history import HistoryStore


class TrackingHistoryConnection(sqlite3.Connection):
    """Record whether one HistoryStore connection reached its close boundary."""

    closed = False

    def close(self) -> None:
        """Mark closure before delegating to the real SQLite connection."""
        self.closed = True
        super().close()


def test_history_store_closes_every_opened_connection(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """Prevent the long-running application from accumulating SQLite descriptors and locks."""
    real_connect = sqlite3.connect
    connections: list[TrackingHistoryConnection] = []

    def tracked_connect(path: Path) -> TrackingHistoryConnection:
        """Create one observable real SQLite connection for the store."""
        connection = real_connect(path, factory=TrackingHistoryConnection)
        connections.append(connection)
        return connection

    monkeypatch.setattr(history_module.sqlite3, "connect", tracked_connect)
    store = HistoryStore(tmp_path / "history.sqlite3")
    store.initialize()
    entry = store.add("raw", "prepared", "dictation", "eng", None, "copied")
    store.find(entry.identifier)
    store.recent()
    store.update_title(entry.identifier, "Closed connections")
    store.delete(entry.identifier)

    assert connections
    assert all(connection.closed for connection in connections)


def test_history_preserves_raw_and_delivered_text(tmp_path: Path) -> None:
    """Keep immutable recognition available beside transformed output."""
    store = HistoryStore(tmp_path / "history.sqlite3")
    store.initialize()
    entry = store.add(
        raw_text="raw um text",
        delivered_text="Raw text.",
        mode="dictation",
        language_code="eng",
        transcription_id="scribe-test",
        delivery_outcome="copied",
    )
    assert store.recent() == [entry]
    store.delete(entry.identifier)
    assert store.recent() == []
    assert store.path.stat().st_mode & 0o777 == 0o600


def test_history_persists_only_reviewed_recognition_route_metadata(tmp_path: Path) -> None:
    """Keep route observability durable without storing arbitrary provider failures."""
    store = HistoryStore(tmp_path / "history.sqlite3")
    store.initialize()

    entry = store.add(
        raw_text="batch recovery",
        delivered_text="Batch recovery.",
        mode="dictation",
        language_code="eng",
        transcription_id="batch-fallback",
        delivery_outcome="copied",
        recognition_route="scribe-v2-batch",
        recognition_fallback_reason="realtime-stream-failed",
    )

    assert store.find(entry.identifier).recognition_route == "scribe-v2-batch"
    assert store.find(entry.identifier).recognition_fallback_reason == "realtime-stream-failed"
    exported = store.export(entry, tmp_path / "exports", "markdown")
    export_text = exported.read_text(encoding="utf-8")
    assert exported.name.startswith("mluva-")
    assert "Recognition: scribe-v2-batch" in export_text
    assert "Recognition fallback: realtime-stream-failed" in export_text


def test_history_persists_controlled_enhancement_provenance_and_timings(tmp_path: Path) -> None:
    """Keep the actual app-server model and disclosed context inspectable without arbitrary metadata."""
    store = HistoryStore(tmp_path / "history.sqlite3")
    store.initialize()

    entry = store.add(
        raw_text="make this concise",
        delivered_text="Concise text.",
        mode="command",
        language_code="eng",
        transcription_id="command-test",
        delivery_outcome="pending-preview",
        enhancement_provider_id="codex-app-server",
        enhancement_model_identifier="gpt-5.4",
        enhancement_context_sources=("selected-text",),
        enhancement_outcome="completed",
        recognition_ms=123,
        enhancement_ms=456,
        delivery_ms=7,
    )

    restored = store.find(entry.identifier)
    assert restored.enhancement_provider_id == "codex-app-server"
    assert restored.enhancement_model_identifier == "gpt-5.4"
    assert restored.enhancement_context_sources == ("selected-text",)
    assert restored.enhancement_outcome == "completed"
    assert (restored.recognition_ms, restored.enhancement_ms, restored.delivery_ms) == (123, 456, 7)
    exported = store.export(restored, tmp_path / "exports", "markdown")
    export_text = exported.read_text(encoding="utf-8")
    assert "Enhancement provider: codex-app-server" in export_text
    assert "Enhancement model: gpt-5.4" in export_text
    assert "Enhancement context: selected-text" in export_text
    assert "Recognition latency: 123 ms" in export_text


@pytest.mark.parametrize(
    "metadata",
    (
        {"enhancement_model_identifier": "gpt-5.4"},
        {
            "enhancement_provider_id": "unreviewed-provider",
            "enhancement_model_identifier": "gpt-5.4",
            "enhancement_outcome": "completed",
        },
        {
            "enhancement_provider_id": "codex-app-server",
            "enhancement_model_identifier": " latest-model",
            "enhancement_outcome": "completed",
        },
        {
            "enhancement_provider_id": "codex-app-server",
            "enhancement_model_identifier": "gpt-5.4",
            "enhancement_context_sources": ("selected-text", "selected-text"),
            "enhancement_outcome": "completed",
        },
        {
            "enhancement_provider_id": "codex-app-server",
            "enhancement_model_identifier": "gpt-5.4",
            "enhancement_context_sources": ("window-title",),
            "enhancement_outcome": "completed",
        },
        {
            "enhancement_provider_id": "codex-app-server",
            "enhancement_model_identifier": "gpt-5.4",
            "enhancement_outcome": "provider-error-body",
        },
        {"recognition_ms": -1},
        {"enhancement_ms": True},
        {"delivery_ms": 86_400_001},
    ),
)
def test_history_rejects_uncontrolled_enhancement_metadata_and_timings(
    tmp_path: Path,
    metadata: dict[str, object],
) -> None:
    """Prevent free-form provider content or impossible durations from entering durable History."""
    store = HistoryStore(tmp_path / "history.sqlite3")
    store.initialize()

    with pytest.raises(ValueError):
        store.add("raw", "final", "dictation", "eng", None, "copied", **metadata)

    assert store.recent() == []


@pytest.mark.parametrize(
    ("route", "fallback_reason"),
    (
        ("unreviewed-provider", None),
        ("scribe-v2-batch", "provider-error-body"),
        ("scribe-v2-realtime", "realtime-stream-failed"),
        (None, "realtime-unavailable"),
    ),
)
def test_history_rejects_uncontrolled_or_inconsistent_recognition_metadata(
    tmp_path: Path,
    route: str | None,
    fallback_reason: str | None,
) -> None:
    """Prevent arbitrary error content or impossible routes from entering durable history."""
    store = HistoryStore(tmp_path / "history.sqlite3")
    store.initialize()

    with pytest.raises(ValueError, match="recognition|fallback reason"):
        store.add(
            raw_text="raw",
            delivered_text="final",
            mode="dictation",
            language_code="eng",
            transcription_id=None,
            delivery_outcome="copied",
            recognition_route=route,
            recognition_fallback_reason=fallback_reason,
        )

    assert store.recent() == []


def test_history_migrates_foundation_schema_and_supports_recovery_actions(tmp_path: Path) -> None:
    """Upgrade an existing PR database without losing transcripts or delivery state."""
    path = tmp_path / "history.sqlite3"
    with closing(sqlite3.connect(path)) as connection, connection:
        connection.execute(
            """
            CREATE TABLE transcription_history (
                identifier TEXT PRIMARY KEY,
                created_at TEXT NOT NULL,
                raw_text TEXT NOT NULL,
                delivered_text TEXT NOT NULL,
                mode TEXT NOT NULL,
                language_code TEXT NOT NULL,
                transcription_id TEXT,
                delivery_outcome TEXT NOT NULL
            )
            """
        )
        connection.execute(
            "INSERT INTO transcription_history VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            ("legacy", "2026-08-13T10:00:00+00:00", "raw text", "Clean text.", "dictation", "eng", None, "copied"),
        )
    store = HistoryStore(path)
    store.initialize()
    assert store.find("legacy").title is None
    assert store.find("legacy").audio_retention_policy is None
    assert store.find("legacy").application_identifier is None
    assert store.find("legacy").correction_source_text is None
    assert store.find("legacy").recognition_route is None
    assert store.find("legacy").recognition_fallback_reason is None
    assert store.find("legacy").enhancement_provider_id is None
    assert store.find("legacy").enhancement_model_identifier is None
    assert store.find("legacy").enhancement_context_sources == ()
    assert store.find("legacy").enhancement_outcome is None
    assert store.find("legacy").recognition_ms is None
    assert store.find("legacy").enhancement_ms is None
    assert store.find("legacy").delivery_ms is None
    assert store.update_title("legacy", "Recovered note").title == "Recovered note"
    restored = store.restore_raw("legacy")
    assert restored.delivered_text == "raw text"
    assert restored.delivery_outcome == "restored-raw"
    exported = store.export(restored, tmp_path / "exports", "json")
    assert exported.name.startswith("mluva-")
    assert '"raw_text": "raw text"' in exported.read_text(encoding="utf-8")
    assert exported.stat().st_mode & 0o777 == 0o600


def test_manual_correction_preserves_first_source_and_application_scope(tmp_path: Path) -> None:
    """Record only explicit edits as suggestion evidence and retain the first comparison text."""
    store = HistoryStore(tmp_path / "history.sqlite3")
    store.initialize()
    entry = store.add(
        raw_text="use post grass",
        delivered_text="Use post grass in production.",
        mode="dictation",
        language_code="eng",
        transcription_id="scribe-correction",
        delivery_outcome="copied",
        application_identifier="/usr/bin/code",
    )

    corrected = store.correct_delivered_text(entry.identifier, "Use Postgres in production.")
    corrected_again = store.correct_delivered_text(entry.identifier, "Use PostgreSQL in production.")

    assert corrected.correction_source_text == "Use post grass in production."
    assert corrected.delivery_outcome == "pending-preview"
    assert corrected_again.correction_source_text == "Use post grass in production."
    assert corrected_again.delivered_text == "Use PostgreSQL in production."
    assert corrected_again.application_identifier == "/usr/bin/code"
    assert HistoryStore(store.path).find(entry.identifier) == corrected_again
    with pytest.raises(ValueError, match="cannot be empty"):
        store.correct_delivered_text(entry.identifier, "  \n")

    restored = store.restore_raw(entry.identifier)
    assert restored.delivered_text == entry.raw_text
    assert restored.correction_source_text is None


def test_restore_and_reprocess_clear_stale_enhancement_and_delivery_evidence(tmp_path: Path) -> None:
    """Never attribute an explicit raw or local rendering to an earlier Codex result or delivery."""
    store = HistoryStore(tmp_path / "history.sqlite3")
    store.initialize()
    entry = store.add(
        raw_text="raw dictated text",
        delivered_text="Earlier generated text.",
        mode="dictation",
        language_code="eng",
        transcription_id="scribe-history-actions",
        delivery_outcome="pasted",
        enhancement_provider_id="codex-app-server",
        enhancement_model_identifier="gpt-5.4",
        enhancement_context_sources=("style-instructions",),
        enhancement_outcome="completed",
        recognition_ms=31,
        enhancement_ms=47,
        delivery_ms=12,
    )

    restored = store.restore_raw(entry.identifier)
    assert restored.delivered_text == "raw dictated text"
    assert restored.enhancement_provider_id is None
    assert restored.enhancement_model_identifier is None
    assert restored.enhancement_context_sources == ()
    assert restored.enhancement_outcome is None
    assert restored.recognition_ms == 31
    assert restored.enhancement_ms is None
    assert restored.delivery_ms is None

    reprocessed = store.reprocess(entry.identifier, "Fresh local rendering.", enhancement_ms=9)
    assert reprocessed.raw_text == "raw dictated text"
    assert reprocessed.delivered_text == "Fresh local rendering."
    assert reprocessed.delivery_outcome == "pending-preview"
    assert reprocessed.enhancement_provider_id is None
    assert reprocessed.enhancement_ms == 9
    assert reprocessed.delivery_ms is None

    with pytest.raises(ValueError, match="cannot be empty"):
        store.reprocess(entry.identifier, " \n", enhancement_ms=0)


def test_history_delete_erases_only_managed_recovery_audio(tmp_path: Path) -> None:
    """Delete retained audio inside the managed directory and reject an injected outside path."""
    data_directory = tmp_path / "data"
    store = HistoryStore(data_directory / "history.sqlite3")
    store.initialize()
    managed_audio = data_directory / "recordings" / "managed.wav"
    managed_audio.parent.mkdir()
    managed_audio.write_bytes(b"RIFF-managed")
    managed_entry = store.add("raw", "final", "scratchpad", "eng", None, "draft", str(managed_audio))
    store.delete(managed_entry.identifier)
    assert not managed_audio.exists()
    assert store.recent() == []

    outside_audio = tmp_path / "outside.wav"
    outside_audio.write_bytes(b"RIFF-outside")
    outside_entry = store.add("raw", "final", "scratchpad", "eng", None, "draft", str(outside_audio))
    with pytest.raises(ValueError, match="outside"):
        store.managed_retained_audio(outside_entry.identifier)
    with pytest.raises(ValueError, match="outside"):
        store.delete(outside_entry.identifier)
    assert outside_audio.exists()
    assert store.find(outside_entry.identifier) == outside_entry


def test_history_retention_prunes_expired_entries_and_preserves_active_draft(tmp_path: Path) -> None:
    """Apply age retention to history and managed audio without destroying unresolved work."""
    data_directory = tmp_path / "data"
    store = HistoryStore(data_directory / "history.sqlite3")
    store.initialize()
    old_audio = data_directory / "recordings" / "old.wav"
    old_audio.parent.mkdir()
    old_audio.write_bytes(b"RIFF-old")
    old_entry = store.add("old", "old", "dictation", "eng", None, "copied", str(old_audio))
    active_entry = store.add("active", "active", "scratchpad", "eng", None, "draft")
    fresh_entry = store.add("fresh", "fresh", "dictation", "eng", None, "copied")
    now = datetime(2026, 8, 13, 12, tzinfo=UTC)
    old_created_at = (now - timedelta(days=31)).isoformat()
    with closing(sqlite3.connect(store.path)) as connection, connection:
        connection.execute(
            "UPDATE transcription_history SET created_at = ? WHERE identifier IN (?, ?)",
            (old_created_at, old_entry.identifier, active_entry.identifier),
        )
    removed = store.prune_older_than(
        30,
        excluded_identifiers=frozenset((active_entry.identifier,)),
        now=now,
    )
    assert removed == 1
    assert not old_audio.exists()
    assert {entry.identifier for entry in store.recent()} == {active_entry.identifier, fresh_entry.identifier}


def test_scratchpad_acceptance_can_preserve_audio_under_always_policy(tmp_path: Path) -> None:
    """Clear a history audio reference only when the frozen session policy requires deletion."""
    store = HistoryStore(tmp_path / "history.sqlite3")
    store.initialize()
    audio_path = tmp_path / "recordings" / "draft.wav"
    audio_path.parent.mkdir()
    audio_path.write_bytes(b"RIFF-draft")
    entry = store.add("raw", "draft", "scratchpad", "eng", None, "draft", str(audio_path))
    retained = store.mark_delivered(
        entry.identifier,
        "accepted",
        "copied",
        retain_audio=True,
        delivery_ms=19,
    )
    assert retained.retained_audio_path == str(audio_path)
    assert retained.delivery_ms == 19
    assert audio_path.exists()
    cleared = store.mark_delivered(entry.identifier, "accepted again", "copied")
    assert cleared.retained_audio_path is None
    assert not audio_path.exists()


def test_recognition_retry_clears_prior_enhancement_provenance_and_replaces_timing(tmp_path: Path) -> None:
    """Do not attribute a fresh batch-retry preview to an enhancement from the failed attempt."""
    store = HistoryStore(tmp_path / "history.sqlite3")
    store.initialize()
    audio_path = tmp_path / "recordings" / "retry.wav"
    audio_path.parent.mkdir()
    audio_path.write_bytes(b"RIFF-retry")
    entry = store.add(
        "old raw",
        "old enhanced",
        "dictation",
        "eng",
        "old-transcription",
        "delivery-failed",
        retained_audio_path=str(audio_path),
        enhancement_provider_id="codex-app-server",
        enhancement_model_identifier="gpt-5.4",
        enhancement_context_sources=("style-instructions",),
        enhancement_outcome="completed",
        recognition_ms=100,
        enhancement_ms=200,
        delivery_ms=300,
    )

    recovered = store.mark_retry_ready(
        entry.identifier,
        raw_text="new raw",
        delivered_text="new local preview",
        language_code="eng",
        transcription_id="new-transcription",
        retain_audio=True,
        recognition_ms=44,
    )

    assert recovered.recognition_route == "scribe-v2-batch-retry"
    assert recovered.enhancement_provider_id is None
    assert recovered.enhancement_model_identifier is None
    assert recovered.enhancement_context_sources == ()
    assert recovered.enhancement_outcome is None
    assert recovered.recognition_ms == 44
    assert recovered.enhancement_ms is None
    assert recovered.delivery_ms is None
