"""SQLite-backed local transcription history."""

import json
import sqlite3
import uuid
from contextlib import closing
from dataclasses import asdict, dataclass
from datetime import UTC, datetime, timedelta
from pathlib import Path

RECOGNITION_ROUTE_REALTIME = "scribe-v2-realtime"
RECOGNITION_ROUTE_BATCH = "scribe-v2-batch"
RECOGNITION_ROUTE_BATCH_RETRY = "scribe-v2-batch-retry"
RECOGNITION_FALLBACK_UNAVAILABLE = "realtime-unavailable"
RECOGNITION_FALLBACK_STARTUP_FAILED = "realtime-startup-failed"
RECOGNITION_FALLBACK_STREAM_FAILED = "realtime-stream-failed"
SUPPORTED_RECOGNITION_ROUTES = frozenset(
    (RECOGNITION_ROUTE_REALTIME, RECOGNITION_ROUTE_BATCH, RECOGNITION_ROUTE_BATCH_RETRY)
)
SUPPORTED_RECOGNITION_FALLBACK_REASONS = frozenset(
    (
        RECOGNITION_FALLBACK_UNAVAILABLE,
        RECOGNITION_FALLBACK_STARTUP_FAILED,
        RECOGNITION_FALLBACK_STREAM_FAILED,
    )
)
ENHANCEMENT_PROVIDER_CODEX_APP_SERVER = "codex-app-server"
ENHANCEMENT_CONTEXT_SELECTED_TEXT = "selected-text"
ENHANCEMENT_CONTEXT_STYLE_INSTRUCTIONS = "style-instructions"
SUPPORTED_ENHANCEMENT_PROVIDERS = frozenset((ENHANCEMENT_PROVIDER_CODEX_APP_SERVER,))
SUPPORTED_ENHANCEMENT_CONTEXT_SOURCES = frozenset(
    (ENHANCEMENT_CONTEXT_SELECTED_TEXT, ENHANCEMENT_CONTEXT_STYLE_INSTRUCTIONS)
)
SUPPORTED_ENHANCEMENT_OUTCOMES = frozenset(("completed", "raw-fallback", "safe-fallback", "failed"))


@dataclass(frozen=True, slots=True)
class HistoryEntry:
    """Represent one raw and delivered transcript without retained secrets."""

    identifier: str
    created_at: str
    raw_text: str
    delivered_text: str
    mode: str
    language_code: str
    transcription_id: str | None
    delivery_outcome: str
    title: str | None
    retained_audio_path: str | None
    audio_retention_policy: str | None
    application_identifier: str | None
    correction_source_text: str | None
    recognition_route: str | None
    recognition_fallback_reason: str | None
    enhancement_provider_id: str | None
    enhancement_model_identifier: str | None
    enhancement_context_sources: tuple[str, ...]
    enhancement_outcome: str | None
    recognition_ms: int | None
    enhancement_ms: int | None
    delivery_ms: int | None


@dataclass(frozen=True, slots=True)
class HistoryStore:
    """Persist recoverable transcript state separately from application settings."""

    path: Path

    def initialize(self) -> None:
        """Create the owner-local history schema when it does not exist."""
        self.path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        with closing(sqlite3.connect(self.path)) as connection, connection:
            connection.execute(
                """
                CREATE TABLE IF NOT EXISTS transcription_history (
                    identifier TEXT PRIMARY KEY,
                    created_at TEXT NOT NULL,
                    raw_text TEXT NOT NULL,
                    delivered_text TEXT NOT NULL,
                    mode TEXT NOT NULL,
                    language_code TEXT NOT NULL,
                    transcription_id TEXT,
                    delivery_outcome TEXT NOT NULL,
                    title TEXT,
                    retained_audio_path TEXT,
                    audio_retention_policy TEXT,
                    application_identifier TEXT,
                    correction_source_text TEXT,
                    recognition_route TEXT,
                    recognition_fallback_reason TEXT,
                    enhancement_provider_id TEXT,
                    enhancement_model_identifier TEXT,
                    enhancement_context_sources TEXT,
                    enhancement_outcome TEXT,
                    recognition_ms INTEGER,
                    enhancement_ms INTEGER,
                    delivery_ms INTEGER
                )
                """
            )
            columns = {row[1] for row in connection.execute("PRAGMA table_info(transcription_history)")}
            if "title" not in columns:
                connection.execute("ALTER TABLE transcription_history ADD COLUMN title TEXT")
            if "retained_audio_path" not in columns:
                connection.execute("ALTER TABLE transcription_history ADD COLUMN retained_audio_path TEXT")
            if "audio_retention_policy" not in columns:
                connection.execute("ALTER TABLE transcription_history ADD COLUMN audio_retention_policy TEXT")
            if "application_identifier" not in columns:
                connection.execute("ALTER TABLE transcription_history ADD COLUMN application_identifier TEXT")
            if "correction_source_text" not in columns:
                connection.execute("ALTER TABLE transcription_history ADD COLUMN correction_source_text TEXT")
            if "recognition_route" not in columns:
                connection.execute("ALTER TABLE transcription_history ADD COLUMN recognition_route TEXT")
            if "recognition_fallback_reason" not in columns:
                connection.execute("ALTER TABLE transcription_history ADD COLUMN recognition_fallback_reason TEXT")
            migration_columns = (
                ("enhancement_provider_id", "TEXT"),
                ("enhancement_model_identifier", "TEXT"),
                ("enhancement_context_sources", "TEXT"),
                ("enhancement_outcome", "TEXT"),
                ("recognition_ms", "INTEGER"),
                ("enhancement_ms", "INTEGER"),
                ("delivery_ms", "INTEGER"),
            )
            for column, column_type in migration_columns:
                if column not in columns:
                    connection.execute(f"ALTER TABLE transcription_history ADD COLUMN {column} {column_type}")
        self.path.chmod(0o600)

    def add(
        self,
        raw_text: str,
        delivered_text: str,
        mode: str,
        language_code: str,
        transcription_id: str | None,
        delivery_outcome: str,
        retained_audio_path: str | None = None,
        audio_retention_policy: str | None = None,
        application_identifier: str | None = None,
        recognition_route: str | None = None,
        recognition_fallback_reason: str | None = None,
        enhancement_provider_id: str | None = None,
        enhancement_model_identifier: str | None = None,
        enhancement_context_sources: tuple[str, ...] = (),
        enhancement_outcome: str | None = None,
        recognition_ms: int | None = None,
        enhancement_ms: int | None = None,
        delivery_ms: int | None = None,
    ) -> HistoryEntry:
        """Commit one completed workflow so raw recognition remains recoverable."""
        _validate_recognition_metadata(recognition_route, recognition_fallback_reason)
        _validate_enhancement_metadata(
            enhancement_provider_id,
            enhancement_model_identifier,
            enhancement_context_sources,
            enhancement_outcome,
        )
        _validate_timing("recognition_ms", recognition_ms)
        _validate_timing("enhancement_ms", enhancement_ms)
        _validate_timing("delivery_ms", delivery_ms)
        entry = HistoryEntry(
            identifier=str(uuid.uuid4()),
            created_at=datetime.now(UTC).isoformat(),
            raw_text=raw_text,
            delivered_text=delivered_text,
            mode=mode,
            language_code=language_code,
            transcription_id=transcription_id,
            delivery_outcome=delivery_outcome,
            title=None,
            retained_audio_path=retained_audio_path,
            audio_retention_policy=audio_retention_policy,
            application_identifier=application_identifier,
            correction_source_text=None,
            recognition_route=recognition_route,
            recognition_fallback_reason=recognition_fallback_reason,
            enhancement_provider_id=enhancement_provider_id,
            enhancement_model_identifier=enhancement_model_identifier,
            enhancement_context_sources=enhancement_context_sources,
            enhancement_outcome=enhancement_outcome,
            recognition_ms=recognition_ms,
            enhancement_ms=enhancement_ms,
            delivery_ms=delivery_ms,
        )
        with closing(sqlite3.connect(self.path)) as connection, connection:
            connection.execute(
                """
                INSERT INTO transcription_history (
                    identifier, created_at, raw_text, delivered_text, mode,
                    language_code, transcription_id, delivery_outcome, title,
                    retained_audio_path, audio_retention_policy,
                    application_identifier, correction_source_text,
                    recognition_route, recognition_fallback_reason,
                    enhancement_provider_id, enhancement_model_identifier,
                    enhancement_context_sources, enhancement_outcome,
                    recognition_ms, enhancement_ms, delivery_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    entry.identifier,
                    entry.created_at,
                    entry.raw_text,
                    entry.delivered_text,
                    entry.mode,
                    entry.language_code,
                    entry.transcription_id,
                    entry.delivery_outcome,
                    entry.title,
                    entry.retained_audio_path,
                    entry.audio_retention_policy,
                    entry.application_identifier,
                    entry.correction_source_text,
                    entry.recognition_route,
                    entry.recognition_fallback_reason,
                    entry.enhancement_provider_id,
                    entry.enhancement_model_identifier,
                    ",".join(entry.enhancement_context_sources) or None,
                    entry.enhancement_outcome,
                    entry.recognition_ms,
                    entry.enhancement_ms,
                    entry.delivery_ms,
                ),
            )
        return entry

    def recent(self, limit: int = 100) -> list[HistoryEntry]:
        """Return recent entries newest-first for inspection and recovery."""
        with closing(sqlite3.connect(self.path)) as connection, connection:
            connection.row_factory = sqlite3.Row
            rows = connection.execute(
                """
                SELECT identifier, created_at, raw_text, delivered_text, mode,
                       language_code, transcription_id, delivery_outcome, title,
                       retained_audio_path, audio_retention_policy,
                       application_identifier, correction_source_text,
                       recognition_route, recognition_fallback_reason,
                       enhancement_provider_id, enhancement_model_identifier,
                       enhancement_context_sources, enhancement_outcome,
                       recognition_ms, enhancement_ms, delivery_ms
                FROM transcription_history
                ORDER BY created_at DESC
                LIMIT ?
                """,
                (limit,),
            ).fetchall()
        return [_decode_history_entry(row) for row in rows]

    def find(self, identifier: str) -> HistoryEntry:
        """Return one exact entry for recovery actions that require current persisted state."""
        with closing(sqlite3.connect(self.path)) as connection, connection:
            connection.row_factory = sqlite3.Row
            row = connection.execute(
                """
                SELECT identifier, created_at, raw_text, delivered_text, mode,
                       language_code, transcription_id, delivery_outcome, title,
                       retained_audio_path, audio_retention_policy,
                       application_identifier, correction_source_text,
                       recognition_route, recognition_fallback_reason,
                       enhancement_provider_id, enhancement_model_identifier,
                       enhancement_context_sources, enhancement_outcome,
                       recognition_ms, enhancement_ms, delivery_ms
                FROM transcription_history
                WHERE identifier = ?
                """,
                (identifier,),
            ).fetchone()
        if row is None:
            raise KeyError(identifier)
        return _decode_history_entry(row)

    def update_title(self, identifier: str, title: str | None) -> HistoryEntry:
        """Persist an optional human label without rewriting transcript content."""
        with closing(sqlite3.connect(self.path)) as connection, connection:
            connection.execute(
                "UPDATE transcription_history SET title = ? WHERE identifier = ?",
                (title, identifier),
            )
        return self.find(identifier)

    def restore_raw(self, identifier: str) -> HistoryEntry:
        """Select immutable raw recognition and clear enhancement and pending-delivery evidence."""
        with closing(sqlite3.connect(self.path)) as connection, connection:
            connection.execute(
                """
                UPDATE transcription_history
                SET delivered_text = raw_text,
                    correction_source_text = NULL,
                    delivery_outcome = 'restored-raw',
                    enhancement_provider_id = NULL,
                    enhancement_model_identifier = NULL,
                    enhancement_context_sources = NULL,
                    enhancement_outcome = NULL,
                    enhancement_ms = NULL,
                    delivery_ms = NULL
                WHERE identifier = ?
                """,
                (identifier,),
            )
        return self.find(identifier)

    def reprocess(self, identifier: str, delivered_text: str, enhancement_ms: int) -> HistoryEntry:
        """Select a fresh deterministic rendering of immutable raw recognition for later delivery."""
        processed_text = delivered_text.strip()
        if not processed_text:
            raise ValueError("Reprocessed text cannot be empty")
        _validate_timing("enhancement_ms", enhancement_ms)
        with closing(sqlite3.connect(self.path)) as connection, connection:
            connection.execute(
                """
                UPDATE transcription_history SET
                    delivered_text = ?,
                    correction_source_text = NULL,
                    delivery_outcome = 'pending-preview',
                    enhancement_provider_id = NULL,
                    enhancement_model_identifier = NULL,
                    enhancement_context_sources = NULL,
                    enhancement_outcome = NULL,
                    enhancement_ms = ?,
                    delivery_ms = NULL
                WHERE identifier = ?
                """,
                (processed_text, enhancement_ms, identifier),
            )
        return self.find(identifier)

    def correct_delivered_text(self, identifier: str, delivered_text: str) -> HistoryEntry:
        """Save one explicit manual correction while preserving its first comparison source."""
        corrected_text = delivered_text.strip()
        if not corrected_text:
            raise ValueError("Corrected text cannot be empty")
        entry = self.find(identifier)
        if corrected_text == entry.delivered_text:
            return entry
        correction_source_text = entry.correction_source_text or entry.delivered_text
        with closing(sqlite3.connect(self.path)) as connection, connection:
            connection.execute(
                """
                UPDATE transcription_history SET
                    delivered_text = ?, correction_source_text = ?,
                    delivery_outcome = 'pending-preview'
                WHERE identifier = ?
                """,
                (corrected_text, correction_source_text, identifier),
            )
        return self.find(identifier)

    def mark_delivered(
        self,
        identifier: str,
        delivered_text: str,
        outcome: str,
        retain_audio: bool = False,
        delivery_ms: int | None = None,
    ) -> HistoryEntry:
        """Record one explicit preview decision and apply its frozen audio policy."""
        entry = self.find(identifier)
        _validate_timing("delivery_ms", delivery_ms)
        if not retain_audio:
            self._delete_retained_audio(entry)
        with closing(sqlite3.connect(self.path)) as connection, connection:
            connection.execute(
                """
                UPDATE transcription_history SET
                    delivered_text = ?,
                    delivery_outcome = ?,
                    retained_audio_path = CASE WHEN ? THEN retained_audio_path ELSE NULL END,
                    delivery_ms = COALESCE(?, delivery_ms)
                WHERE identifier = ?
                """,
                (delivered_text, outcome, retain_audio, delivery_ms, identifier),
            )
        return self.find(identifier)

    def mark_retry_ready(
        self,
        identifier: str,
        raw_text: str,
        delivered_text: str,
        language_code: str,
        transcription_id: str | None,
        retain_audio: bool,
        recognition_ms: int | None = None,
    ) -> HistoryEntry:
        """Store recovered recognition as a preview without delivering it automatically."""
        entry = self.find(identifier)
        _validate_timing("recognition_ms", recognition_ms)
        with closing(sqlite3.connect(self.path)) as connection, connection:
            connection.execute(
                """
                UPDATE transcription_history SET
                    raw_text = ?, delivered_text = ?, language_code = ?,
                    transcription_id = ?, correction_source_text = NULL,
                    recognition_route = 'scribe-v2-batch-retry',
                    recognition_fallback_reason = NULL,
                    enhancement_provider_id = NULL,
                    enhancement_model_identifier = NULL,
                    enhancement_context_sources = NULL,
                    enhancement_outcome = NULL,
                    recognition_ms = ?,
                    enhancement_ms = NULL,
                    delivery_ms = NULL,
                    delivery_outcome = 'retry-ready'
                WHERE identifier = ?
                """,
                (raw_text, delivered_text, language_code, transcription_id, recognition_ms, identifier),
            )
        if not retain_audio:
            self._delete_retained_audio(entry)
            with closing(sqlite3.connect(self.path)) as connection, connection:
                connection.execute(
                    "UPDATE transcription_history SET retained_audio_path = NULL WHERE identifier = ?",
                    (identifier,),
                )
        return self.find(identifier)

    def managed_retained_audio(self, identifier: str) -> Path:
        """Return a retry source only after proving it belongs to the managed recordings directory."""
        entry = self.find(identifier)
        if entry.retained_audio_path is None:
            raise ValueError("History entry has no retained audio")
        audio_path = self._validated_audio_path(entry.retained_audio_path)
        if not audio_path.is_file():
            raise FileNotFoundError(audio_path)
        return audio_path

    def prune_older_than(
        self,
        retention_days: int,
        excluded_identifiers: frozenset[str] = frozenset(),
        now: datetime | None = None,
    ) -> int:
        """Delete expired history and managed audio while preserving active drafts."""
        if retention_days <= 0:
            return 0
        cutoff = (now or datetime.now(UTC)) - timedelta(days=retention_days)
        removed = 0
        for entry in self.recent(limit=2_147_483_647):
            if entry.identifier in excluded_identifiers:
                continue
            created_at = datetime.fromisoformat(entry.created_at)
            if created_at.tzinfo is None:
                created_at = created_at.replace(tzinfo=UTC)
            if created_at >= cutoff:
                continue
            try:
                self._delete_retained_audio(entry)
            except ValueError:
                pass
            with closing(sqlite3.connect(self.path)) as connection, connection:
                connection.execute("DELETE FROM transcription_history WHERE identifier = ?", (entry.identifier,))
            removed += 1
        return removed

    def export(self, entry: HistoryEntry, directory: Path, export_format: str) -> Path:
        """Write one selected history entry to an owner-local recovery artifact."""
        directory.mkdir(mode=0o700, parents=True, exist_ok=True)
        directory.chmod(0o700)
        filename = f"mluva-{entry.created_at[:19].replace(':', '')}-{entry.identifier[:8]}"
        if export_format == "json":
            output_path = directory / f"{filename}.json"
            content = json.dumps(asdict(entry), indent=2) + "\n"
        elif export_format == "markdown":
            output_path = directory / f"{filename}.md"
            title = entry.title or "Mluva transcript"
            content = (
                f"# {title}\n\n"
                f"- Created: {entry.created_at}\n"
                f"- Mode: {entry.mode}\n"
                f"- Language: {entry.language_code}\n"
                f"- Delivery: {entry.delivery_outcome}\n"
                f"- Recognition: {entry.recognition_route or 'Legacy/unknown'}\n"
                f"- Recognition fallback: {entry.recognition_fallback_reason or 'None'}\n"
                f"- Enhancement provider: {entry.enhancement_provider_id or 'None'}\n"
                f"- Enhancement model: {entry.enhancement_model_identifier or 'None'}\n"
                f"- Enhancement context: {', '.join(entry.enhancement_context_sources) or 'None'}\n"
                f"- Enhancement outcome: {entry.enhancement_outcome or 'None'}\n"
                f"- Recognition latency: {_format_milliseconds(entry.recognition_ms)}\n"
                f"- Enhancement latency: {_format_milliseconds(entry.enhancement_ms)}\n"
                f"- Delivery latency: {_format_milliseconds(entry.delivery_ms)}\n"
                f"- Application: {entry.application_identifier or 'Not captured'}\n\n"
                f"## Delivered text\n\n{entry.delivered_text}\n\n"
                f"## Raw transcript\n\n{entry.raw_text}\n"
            )
        else:
            raise ValueError(export_format)
        output_path.write_text(content, encoding="utf-8")
        output_path.chmod(0o600)
        return output_path

    def delete(self, identifier: str) -> None:
        """Permanently delete one explicitly selected history entry."""
        entry = self.find(identifier)
        self._delete_retained_audio(entry)
        with closing(sqlite3.connect(self.path)) as connection, connection:
            connection.execute("DELETE FROM transcription_history WHERE identifier = ?", (identifier,))

    def _delete_retained_audio(self, entry: HistoryEntry) -> None:
        """Erase only recovery audio proven to live inside the managed directory."""
        if entry.retained_audio_path is None:
            return
        audio_path = self._validated_audio_path(entry.retained_audio_path)
        audio_path.unlink(missing_ok=True)

    def _validated_audio_path(self, retained_audio_path: str) -> Path:
        """Resolve a persisted audio path without trusting database contents."""
        audio_path = Path(retained_audio_path).resolve()
        recordings_directory = (self.path.parent / "recordings").resolve()
        if not audio_path.is_relative_to(recordings_directory):
            raise ValueError(f"Refusing to access history audio outside {recordings_directory}")
        return audio_path


def _validate_recognition_metadata(route: str | None, fallback_reason: str | None) -> None:
    """Keep durable provider routing limited to reviewed non-content values."""
    if route is not None and route not in SUPPORTED_RECOGNITION_ROUTES:
        raise ValueError(f"Unsupported recognition route: {route}")
    if fallback_reason is not None and fallback_reason not in SUPPORTED_RECOGNITION_FALLBACK_REASONS:
        raise ValueError(f"Unsupported recognition fallback reason: {fallback_reason}")
    if fallback_reason is not None and route != RECOGNITION_ROUTE_BATCH:
        raise ValueError("A realtime fallback reason requires the Scribe v2 batch route")


def _validate_enhancement_metadata(
    provider_id: str | None,
    model_identifier: str | None,
    context_sources: tuple[str, ...],
    outcome: str | None,
) -> None:
    """Keep durable enhancement provenance complete and limited to reviewed non-content values."""
    if provider_id is None:
        if model_identifier is not None or context_sources or outcome is not None:
            raise ValueError("Enhancement metadata requires a provider ID")
        return
    if provider_id not in SUPPORTED_ENHANCEMENT_PROVIDERS:
        raise ValueError(f"Unsupported enhancement provider: {provider_id}")
    if (
        model_identifier is None
        or not model_identifier
        or model_identifier != model_identifier.strip()
        or len(model_identifier) > 200
        or any(ord(character) < 32 for character in model_identifier)
    ):
        raise ValueError("Enhancement model must be one bounded concrete identifier")
    if len(context_sources) != len(set(context_sources)) or any(
        source not in SUPPORTED_ENHANCEMENT_CONTEXT_SOURCES for source in context_sources
    ):
        raise ValueError("Enhancement context sources must be unique reviewed values")
    if outcome not in SUPPORTED_ENHANCEMENT_OUTCOMES:
        raise ValueError("Enhancement outcome must be one reviewed terminal value")


def _validate_timing(label: str, value: int | None) -> None:
    """Reject negative, boolean, or longer-than-day stage durations before persistence."""
    if value is None:
        return
    if isinstance(value, bool) or not isinstance(value, int) or not 0 <= value <= 86_400_000:
        raise ValueError(f"{label} must be an integer from zero through 24 hours")


def _decode_history_entry(row: sqlite3.Row) -> HistoryEntry:
    """Decode one named SQLite row while normalizing legacy nullable context metadata."""
    encoded_context = row["enhancement_context_sources"]
    return HistoryEntry(
        identifier=row["identifier"],
        created_at=row["created_at"],
        raw_text=row["raw_text"],
        delivered_text=row["delivered_text"],
        mode=row["mode"],
        language_code=row["language_code"],
        transcription_id=row["transcription_id"],
        delivery_outcome=row["delivery_outcome"],
        title=row["title"],
        retained_audio_path=row["retained_audio_path"],
        audio_retention_policy=row["audio_retention_policy"],
        application_identifier=row["application_identifier"],
        correction_source_text=row["correction_source_text"],
        recognition_route=row["recognition_route"],
        recognition_fallback_reason=row["recognition_fallback_reason"],
        enhancement_provider_id=row["enhancement_provider_id"],
        enhancement_model_identifier=row["enhancement_model_identifier"],
        enhancement_context_sources=tuple(encoded_context.split(",")) if encoded_context else (),
        enhancement_outcome=row["enhancement_outcome"],
        recognition_ms=row["recognition_ms"],
        enhancement_ms=row["enhancement_ms"],
        delivery_ms=row["delivery_ms"],
    )


def _format_milliseconds(value: int | None) -> str:
    """Render one optional stage duration without fabricating timing for legacy rows."""
    return f"{value} ms" if value is not None else "Not recorded"
