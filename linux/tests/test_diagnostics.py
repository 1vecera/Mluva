"""Privacy and retention coverage for local workflow diagnostics."""

import json
import sqlite3
import uuid
from contextlib import closing
from pathlib import Path

import pytest

import voice_scribe_linux.diagnostics as diagnostics_module
from voice_scribe_linux.config import AppConfig, AudioRetentionPolicy
from voice_scribe_linux.diagnostics import (
    DiagnosticOutcome,
    DiagnosticProvider,
    DiagnosticsStore,
    DiagnosticStage,
)


class TrackingDiagnosticsConnection(sqlite3.Connection):
    """Record whether one DiagnosticsStore connection reached its close boundary."""

    closed = False

    def close(self) -> None:
        """Mark closure before delegating to the real SQLite connection."""
        self.closed = True
        super().close()


def test_diagnostics_store_closes_every_opened_connection(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """Prevent repeated timing events from leaking SQLite descriptors in a long-running session."""
    real_connect = sqlite3.connect
    connections: list[TrackingDiagnosticsConnection] = []

    def tracked_connect(path: Path) -> TrackingDiagnosticsConnection:
        """Create one observable real SQLite connection for the store."""
        connection = real_connect(path, factory=TrackingDiagnosticsConnection)
        connections.append(connection)
        return connection

    monkeypatch.setattr(diagnostics_module.sqlite3, "connect", tracked_connect)
    store = DiagnosticsStore(tmp_path / "diagnostics.sqlite3")
    store.initialize()
    store.record(
        str(uuid.uuid4()),
        "dictation",
        DiagnosticStage.RECOGNITION,
        DiagnosticProvider.ELEVENLABS_SCRIBE_V2,
        DiagnosticOutcome.COMPLETED,
        0.1,
    )
    store.recent()

    assert connections
    assert all(connection.closed for connection in connections)


def test_diagnostic_schema_and_export_exclude_content_and_secrets(tmp_path: Path) -> None:
    """Export only controlled configuration and event fields with owner permissions."""
    store = DiagnosticsStore(tmp_path / "diagnostics.sqlite3")
    store.initialize()
    session_identifier = str(uuid.uuid4())
    store.record(
        session_identifier,
        "command",
        DiagnosticStage.ENHANCEMENT,
        DiagnosticProvider.CODEX_APP_SERVER,
        DiagnosticOutcome.COMPLETED,
        0.321,
    )

    output_path = store.export(
        tmp_path / "exports",
        AppConfig(
            language_code="eng",
            codex_model="configured-model-must-not-be-exported",
            microphone_target="private.microphone.identity",
            system_audio_target="private.system.output.identity",
            audio_retention_policy=AudioRetentionPolicy.FAILURES,
        ),
    )
    payload = json.loads(output_path.read_text(encoding="utf-8"))
    serialized = json.dumps(payload)

    assert store.path.stat().st_mode & 0o777 == 0o600
    assert output_path.stat().st_mode & 0o777 == 0o600
    assert output_path.name.startswith("mluva-diagnostics-")
    assert payload["schema"] == "mluva-diagnostics-v1"
    assert payload["configuration"]["codex_model_configured"] is True
    assert payload["configuration"]["microphone_target_configured"] is True
    assert payload["configuration"]["system_audio_target_configured"] is True
    assert payload["configuration"]["global_recording_key"] == "F9"
    assert "configured-model-must-not-be-exported" not in serialized
    assert "private.microphone.identity" not in serialized
    assert "private.system.output.identity" not in serialized
    assert payload["events"] == [
        {
            "created_at": payload["events"][0]["created_at"],
            "session_identifier": session_identifier,
            "mode": "command",
            "stage": "enhancement",
            "provider": "codex-app-server",
            "outcome": "completed",
            "duration_ms": 321,
        }
    ]
    with closing(sqlite3.connect(store.path)) as connection, connection:
        columns = {row[1] for row in connection.execute("PRAGMA table_info(diagnostic_events)")}
    assert columns == {
        "sequence",
        "created_at",
        "session_identifier",
        "mode",
        "stage",
        "provider",
        "outcome",
        "duration_ms",
    }


def test_export_redacts_free_form_provider_configuration(tmp_path: Path) -> None:
    """Do not trust a hand-edited model string as safe diagnostic metadata."""
    store = DiagnosticsStore(tmp_path / "diagnostics.sqlite3")
    store.initialize()
    output_path = store.export(
        tmp_path / "exports",
        AppConfig(
            codex_model="sensitive-custom-model-alias",
        ),
    )
    serialized = output_path.read_text(encoding="utf-8")
    assert "sensitive-custom-model-alias" not in serialized
    payload = json.loads(serialized)
    assert payload["configuration"]["language_code"] == "eng"
    assert payload["configuration"]["transcription_model"] == "scribe_v2"
    assert payload["configuration"]["codex_model_configured"] is True


def test_diagnostics_reject_unbounded_identifiers_modes_and_durations(tmp_path: Path) -> None:
    """Prevent free-form content from entering controlled correlation fields."""
    store = DiagnosticsStore(tmp_path / "diagnostics.sqlite3")
    store.initialize()
    session_identifier = str(uuid.uuid4())
    with pytest.raises(ValueError):
        store.record(
            "not-a-uuid transcript text",
            "dictation",
            DiagnosticStage.RECOGNITION,
            DiagnosticProvider.ELEVENLABS_SCRIBE_V2,
            DiagnosticOutcome.FAILED,
            0.1,
        )
    with pytest.raises(ValueError, match="Unsupported diagnostic mode"):
        store.record(
            session_identifier,
            "arbitrary transcript text",
            DiagnosticStage.RECOGNITION,
            DiagnosticProvider.ELEVENLABS_SCRIBE_V2,
            DiagnosticOutcome.FAILED,
            0.1,
        )
    with pytest.raises(ValueError, match="between zero and 24 hours"):
        store.record(
            session_identifier,
            "dictation",
            DiagnosticStage.RECOGNITION,
            DiagnosticProvider.ELEVENLABS_SCRIBE_V2,
            DiagnosticOutcome.FAILED,
            float("nan"),
        )


def test_diagnostics_retention_keeps_only_newest_events(tmp_path: Path) -> None:
    """Bound durable observability independently from transcription history."""
    store = DiagnosticsStore(tmp_path / "diagnostics.sqlite3", maximum_events=2)
    store.initialize()
    for duration in (0.1, 0.2, 0.3):
        store.record(
            str(uuid.uuid4()),
            "dictation",
            DiagnosticStage.RECOGNITION,
            DiagnosticProvider.ELEVENLABS_SCRIBE_V2,
            DiagnosticOutcome.COMPLETED,
            duration,
        )
    assert [event.duration_ms for event in reversed(store.recent())] == [200, 300]
