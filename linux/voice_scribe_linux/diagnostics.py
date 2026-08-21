"""Content-free local timing and outcome diagnostics for Mluva."""

import json
import math
import re
import sqlite3
import uuid
from contextlib import closing
from dataclasses import asdict, dataclass
from datetime import UTC, datetime
from enum import StrEnum
from pathlib import Path

from voice_scribe_linux.config import AppConfig


class DiagnosticStage(StrEnum):
    """Name the bounded workflow stages that diagnostics may disclose."""

    CAPTURE_READY = "capture-ready"
    CAPTURE = "capture"
    RECOGNITION = "recognition"
    ENHANCEMENT = "enhancement"
    DELIVERY = "delivery"


class DiagnosticProvider(StrEnum):
    """Name providers without account, target, model alias, or credential data."""

    PIPEWIRE = "pipewire"
    ELEVENLABS_SCRIBE_V2 = "elevenlabs-scribe-v2"
    CODEX_APP_SERVER = "codex-app-server"
    DESKTOP = "desktop"
    LOCAL = "local"


class DiagnosticOutcome(StrEnum):
    """Describe controlled terminal outcomes without raw failure messages."""

    COMPLETED = "completed"
    FAILED = "failed"
    RAW_FALLBACK = "raw-fallback"
    SAFE_FALLBACK = "safe-fallback"
    DEFERRED = "deferred"
    CANCELLED = "cancelled"


@dataclass(frozen=True, slots=True)
class DiagnosticEvent:
    """Represent one content-free local event timing."""

    created_at: str
    session_identifier: str
    mode: str
    stage: str
    provider: str
    outcome: str
    duration_ms: int


@dataclass(frozen=True, slots=True)
class DiagnosticsStore:
    """Persist a bounded event ledger and export only reviewed fields."""

    path: Path
    maximum_events: int = 5_000

    def __post_init__(self) -> None:
        """Require a positive retention bound before any durable write."""
        if self.maximum_events <= 0:
            raise ValueError("maximum_events must be positive")

    def initialize(self) -> None:
        """Create an owner-only schema that has no content or arbitrary-message columns."""
        self.path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        with closing(sqlite3.connect(self.path)) as connection, connection:
            connection.execute(
                """
                CREATE TABLE IF NOT EXISTS diagnostic_events (
                    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                    created_at TEXT NOT NULL,
                    session_identifier TEXT NOT NULL,
                    mode TEXT NOT NULL,
                    stage TEXT NOT NULL,
                    provider TEXT NOT NULL,
                    outcome TEXT NOT NULL,
                    duration_ms INTEGER NOT NULL
                )
                """
            )
        self.path.chmod(0o600)

    def record(
        self,
        session_identifier: str,
        mode: str,
        stage: DiagnosticStage,
        provider: DiagnosticProvider,
        outcome: DiagnosticOutcome,
        duration_seconds: float,
    ) -> None:
        """Store one validated event without accepting free-form diagnostic content."""
        uuid.UUID(session_identifier)
        if mode not in {"dictation", "command", "scratchpad", "meeting"}:
            raise ValueError(f"Unsupported diagnostic mode: {mode}")
        if not isinstance(stage, DiagnosticStage):
            raise TypeError("stage must be a DiagnosticStage")
        if not isinstance(provider, DiagnosticProvider):
            raise TypeError("provider must be a DiagnosticProvider")
        if not isinstance(outcome, DiagnosticOutcome):
            raise TypeError("outcome must be a DiagnosticOutcome")
        if not math.isfinite(duration_seconds) or duration_seconds < 0 or duration_seconds > 86_400:
            raise ValueError("Diagnostic duration must be between zero and 24 hours")
        duration_ms = round(duration_seconds * 1_000)
        with closing(sqlite3.connect(self.path)) as connection, connection:
            connection.execute(
                """
                INSERT INTO diagnostic_events (
                    created_at, session_identifier, mode, stage, provider, outcome, duration_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    datetime.now(UTC).isoformat(),
                    session_identifier,
                    mode,
                    stage.value,
                    provider.value,
                    outcome.value,
                    duration_ms,
                ),
            )
            connection.execute(
                """
                DELETE FROM diagnostic_events
                WHERE sequence NOT IN (
                    SELECT sequence FROM diagnostic_events ORDER BY sequence DESC LIMIT ?
                )
                """,
                (self.maximum_events,),
            )

    def recent(self, limit: int = 1_000) -> list[DiagnosticEvent]:
        """Return the newest bounded event records for local inspection or export."""
        with closing(sqlite3.connect(self.path)) as connection, connection:
            rows = connection.execute(
                """
                SELECT created_at, session_identifier, mode, stage, provider, outcome, duration_ms
                FROM diagnostic_events
                ORDER BY sequence DESC
                LIMIT ?
                """,
                (min(max(limit, 0), self.maximum_events),),
            ).fetchall()
        return [DiagnosticEvent(*row) for row in rows]

    def export(self, directory: Path, config: AppConfig) -> Path:
        """Write configuration and timing metadata while excluding user and provider content."""
        directory.mkdir(mode=0o700, parents=True, exist_ok=True)
        directory.chmod(0o700)
        created_at = datetime.now(UTC)
        output_path = directory / f"mluva-diagnostics-{created_at.strftime('%Y%m%dT%H%M%S%fZ')}.json"
        safe_language_code = (
            config.language_code
            if re.fullmatch(r"[A-Za-z]{2,3}(?:-[A-Za-z0-9]{2,8})?", config.language_code)
            else "custom"
        )
        payload = {
            "schema": "mluva-diagnostics-v1",
            "generated_at": created_at.isoformat(),
            "configuration": {
                "language_code": safe_language_code,
                "transcription_model": "scribe_v2" if config.transcription_model == "scribe_v2" else "custom",
                "codex_model_configured": config.codex_model is not None,
                "microphone_target_configured": config.microphone_target is not None,
                "system_audio_target_configured": config.system_audio_target is not None,
                "default_mode": config.default_mode,
                "global_recording_key": config.global_recording_key,
                "auto_paste": config.auto_paste,
                "spoken_commands_enabled": config.spoken_commands_enabled,
                "remember_per_application": config.remember_per_application,
                "audio_retention_policy": config.audio_retention_policy.value,
                "incognito_mode": config.incognito_mode,
                "history_retention_days": config.history_retention_days,
            },
            "events": [asdict(event) for event in reversed(self.recent())],
            "excluded": [
                "audio",
                "audio paths",
                "transcript text",
                "selected text",
                "clipboard content",
                "application identity",
                "window titles",
                "credentials",
                "raw exception messages",
            ],
        }
        output_path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
        output_path.chmod(0o600)
        return output_path
