"""Explicit Meeting transcription, insights, retry, and owner-only archival."""

from __future__ import annotations

import json
import os
import re
import shutil
import uuid
from dataclasses import dataclass, field, replace
from datetime import UTC, datetime
from enum import StrEnum
from pathlib import Path
from typing import Protocol

from voice_scribe_linux.audio import MeetingCaptureResult
from voice_scribe_linux.config import AppConfig
from voice_scribe_linux.elevenlabs import SpeakerSegment, TranscriptionResult

MAX_MEETINGS = 200
MAX_MEETING_DOCUMENT_BYTES = 100_000_000
MEETING_PROVIDER = "elevenlabsScribeV2"
_DECISION_MARKERS = ("we decided", "we agreed", "agreed to", "the decision", "decision:")
_ACTION_MARKERS = ("action item", "follow up", "follow-up", "todo", "to-do")
_NAMED_OWNER_PATTERN = re.compile(r"(?iu)(?<!\w)(?!we\b)[^\W\d_](?:[^\W\d_]|['’\-])*\s+will\b")


class MeetingAudioSource(StrEnum):
    """Name only the source streams that produced usable archived audio."""

    MICROPHONE = "microphone"
    SYSTEM = "system"


class MeetingRecognitionStatus(StrEnum):
    """Distinguish a finished transcript from retained audio awaiting retry."""

    COMPLETED = "completed"
    FAILED = "failed"


@dataclass(frozen=True, slots=True)
class MeetingSpeakerSegment:
    """Preserve one contiguous provider-diarized segment."""

    speaker: str
    text: str
    started_at_seconds: float
    ended_at_seconds: float

    def __post_init__(self) -> None:
        """Clamp provider timing so persisted segments cannot run backwards."""
        started_at_seconds = max(0.0, float(self.started_at_seconds))
        object.__setattr__(self, "started_at_seconds", started_at_seconds)
        object.__setattr__(self, "ended_at_seconds", max(started_at_seconds, float(self.ended_at_seconds)))


@dataclass(frozen=True, slots=True)
class MeetingInsights:
    """Hold deterministic, reviewable extracts without inventing meeting facts."""

    summary: str = ""
    decisions: tuple[str, ...] = ()
    action_items: tuple[str, ...] = ()


@dataclass(frozen=True, slots=True)
class MeetingRecord:
    """Represent one Meeting archive entry separately from dictation history."""

    transcript: str
    duration_seconds: float
    provider: str = MEETING_PROVIDER
    language: str = "eng"
    audio_sources: tuple[MeetingAudioSource, ...] = ()
    identifier: str = field(default_factory=lambda: str(uuid.uuid4()))
    title: str | None = None
    speakers: tuple[MeetingSpeakerSegment, ...] = ()
    insights: MeetingInsights = field(default_factory=MeetingInsights)
    timestamp: str = field(default_factory=lambda: _iso_timestamp(datetime.now(UTC)))
    recording_filename: str | None = None
    recognition_status: MeetingRecognitionStatus = MeetingRecognitionStatus.COMPLETED
    transcription_id: str | None = None
    warnings: tuple[str, ...] = ()

    def __post_init__(self) -> None:
        """Normalize bounded scalar state and stable source ordering."""
        uuid.UUID(self.identifier)
        object.__setattr__(self, "duration_seconds", max(0.0, float(self.duration_seconds)))
        normalized_title = self.title.strip() if self.title is not None else None
        object.__setattr__(self, "title", normalized_title or None)
        ordered_sources = tuple(source for source in MeetingAudioSource if source in self.audio_sources)
        object.__setattr__(self, "audio_sources", ordered_sources)

    def renamed(self, title: str | None) -> MeetingRecord:
        """Return one copy with only its optional human-authored title changed."""
        return replace(self, title=title)


@dataclass(slots=True)
class MeetingStore:
    """Persist Meeting records and recordings under a separate owner-only boundary."""

    path: Path
    recordings_directory: Path | None = None
    meetings: list[MeetingRecord] = field(init=False, default_factory=list)
    persistence_error: str | None = field(init=False, default=None)

    def __post_init__(self) -> None:
        """Resolve the private recording directory and load an existing archive."""
        if self.recordings_directory is None:
            self.recordings_directory = self.path.parent / "recordings"
        self._load()

    def save(self, meeting: MeetingRecord) -> MeetingRecord:
        """Atomically insert or update one record before deleting any overflow audio."""
        if not meeting.transcript.strip() and meeting.recording_filename is None:
            raise ValueError("A meeting requires either a transcript or a retained recording.")
        updated = list(self.meetings)
        index = next(
            (position for position, existing in enumerate(updated) if existing.identifier == meeting.identifier),
            None,
        )
        if index is None:
            updated.insert(0, meeting)
        else:
            updated[index] = meeting
        overflow = updated[MAX_MEETINGS:]
        updated = updated[:MAX_MEETINGS]
        self._persist(updated)
        self.meetings = updated
        for removed in overflow:
            self._delete_recording(removed.recording_filename)
        return meeting

    def recent(self, limit: int = 100) -> tuple[MeetingRecord, ...]:
        """Return bounded newest-first archive records."""
        if isinstance(limit, bool) or not isinstance(limit, int) or limit < 0:
            raise ValueError("Meeting limit must be a non-negative integer.")
        return tuple(self.meetings[:limit])

    def find(self, identifier: str) -> MeetingRecord:
        """Return one exact record for review or retry."""
        for meeting in self.meetings:
            if meeting.identifier == identifier:
                return meeting
        raise KeyError(identifier)

    def rename(self, identifier: str, title: str | None) -> MeetingRecord:
        """Persist a human title without rewriting transcript or provider metadata."""
        return self.save(self.find(identifier).renamed(title))

    def delete(self, identifier: str) -> None:
        """Delete one selected record, then only its validated managed recording."""
        meeting = self.find(identifier)
        updated = [existing for existing in self.meetings if existing.identifier != identifier]
        self._persist(updated)
        self.meetings = updated
        self._delete_recording(meeting.recording_filename)

    def clear(self) -> None:
        """Clear the Meeting archive and its referenced managed recordings."""
        removed = list(self.meetings)
        self._persist([])
        self.meetings = []
        for meeting in removed:
            self._delete_recording(meeting.recording_filename)

    def recording_path(self, meeting: MeetingRecord) -> Path | None:
        """Resolve a retained recording only when its basename stays inside the archive."""
        filename = meeting.recording_filename
        if filename is None or not _is_safe_recording_filename(filename):
            return None
        recordings_directory = self._recordings_directory()
        candidate = (recordings_directory / filename).resolve()
        if candidate.parent != recordings_directory.resolve() or not candidate.is_file():
            return None
        return candidate

    def archive_recording(self, source_path: Path, identifier: str) -> Path:
        """Copy finalized Meeting audio into a private, stable retry location."""
        if not source_path.is_file():
            raise FileNotFoundError(source_path)
        uuid.UUID(identifier)
        recordings_directory = self._recordings_directory()
        recordings_directory.mkdir(mode=0o700, parents=True, exist_ok=True)
        recordings_directory.chmod(0o700)
        destination = recordings_directory / f"{identifier}.wav"
        if source_path.resolve() == destination.resolve():
            destination.chmod(0o600)
            return destination
        if destination.exists():
            raise FileExistsError(destination)
        temporary_path = destination.with_name(f".{destination.name}.{uuid.uuid4().hex}.tmp")
        try:
            _create_private_file(temporary_path)
            shutil.copyfile(source_path, temporary_path)
            temporary_path.chmod(0o600)
            os.replace(temporary_path, destination)
            destination.chmod(0o600)
            source_path.unlink()
        except Exception:
            temporary_path.unlink(missing_ok=True)
            raise
        return destination

    def export(self, meeting: MeetingRecord, directory: Path, export_format: str) -> Path:
        """Export one selected meeting as owner-only JSON or reviewable Markdown."""
        directory.mkdir(mode=0o700, parents=True, exist_ok=True)
        directory.chmod(0o700)
        filename = f"mluva-meeting-{meeting.timestamp[:19].replace(':', '')}-{meeting.identifier[:8]}"
        if export_format == "json":
            output_path = directory / f"{filename}.json"
            content = json.dumps(_encode_meeting(meeting), indent=2, sort_keys=True, ensure_ascii=False) + "\n"
        elif export_format == "markdown":
            output_path = directory / f"{filename}.md"
            content = _meeting_markdown(meeting)
        else:
            raise ValueError(export_format)
        _atomic_write_text(output_path, content)
        return output_path

    def _load(self) -> None:
        """Decode a compatible archive while preserving malformed user data unchanged."""
        if not self.path.exists():
            return
        try:
            if self.path.stat().st_size > MAX_MEETING_DOCUMENT_BYTES:
                raise ValueError("Meeting archive exceeds its supported size.")
            payload = json.loads(self.path.read_text(encoding="utf-8"))
            if not isinstance(payload, list) or not all(isinstance(item, dict) for item in payload):
                raise ValueError("Meeting archive must be an array of objects.")
            meetings = [_decode_meeting(item) for item in payload]
            if len(meetings) > MAX_MEETINGS:
                raise ValueError(f"Meeting archive supports at most {MAX_MEETINGS} records.")
            self.meetings = meetings
            self.path.parent.chmod(0o700)
            self.path.chmod(0o600)
        except (OSError, ValueError, TypeError, json.JSONDecodeError) as error:
            self.meetings = []
            self.persistence_error = str(error)

    def _persist(self, meetings: list[MeetingRecord]) -> None:
        """Fail closed after malformed input and atomically write a private archive."""
        if self.persistence_error is not None:
            raise RuntimeError(
                f"Meeting changes are disabled until the malformed archive is repaired: {self.persistence_error}"
            )
        self.path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        self.path.parent.chmod(0o700)
        content = (
            json.dumps(
                [_encode_meeting(meeting) for meeting in meetings],
                indent=2,
                sort_keys=True,
                ensure_ascii=False,
            )
            + "\n"
        )
        _atomic_write_text(self.path, content)

    def _delete_recording(self, filename: str | None) -> None:
        """Delete only one safe basename inside the managed recording directory."""
        if filename is None or not _is_safe_recording_filename(filename):
            return
        recordings_directory = self._recordings_directory().resolve()
        candidate = (recordings_directory / filename).resolve()
        if candidate.parent == recordings_directory:
            candidate.unlink(missing_ok=True)

    def _recordings_directory(self) -> Path:
        """Return the resolved non-optional archive directory."""
        if self.recordings_directory is None:
            raise RuntimeError("Meeting recordings directory was not initialized.")
        return self.recordings_directory


class MeetingTranscriptionClient(Protocol):
    """Describe the diarized Scribe operation needed only by Meeting."""

    def transcribe_meeting(self, file_path: Path, language_code: str, model_id: str) -> TranscriptionResult:
        """Return one finalized transcript with any available speaker segments."""
        ...


class MeetingFailure(RuntimeError):
    """Carry controlled retry state without exposing provider response bodies."""

    def __init__(
        self,
        message: str,
        stage: str,
        meeting: MeetingRecord | None,
        retained_audio_path: Path | None,
    ) -> None:
        """Preserve the failed stage and any owner-local retry artifacts."""
        super().__init__(message)
        self.stage = stage
        self.meeting = meeting
        self.retained_audio_path = retained_audio_path


@dataclass(frozen=True, slots=True)
class MeetingWorkflowResult:
    """Return a completed Meeting without invoking dictation delivery."""

    meeting: MeetingRecord
    transcription: TranscriptionResult
    incognito: bool


@dataclass(slots=True)
class MeetingWorkflow:
    """Archive and diarize an explicitly finalized Meeting recording."""

    config: AppConfig
    elevenlabs: MeetingTranscriptionClient
    store: MeetingStore

    def complete(
        self,
        capture: MeetingCaptureResult,
        *,
        incognito: bool,
        started_at: datetime | None = None,
        identifier: str | None = None,
    ) -> MeetingWorkflowResult:
        """Transcribe one explicit capture, retaining retry audio only outside Incognito."""
        try:
            identifier = identifier or str(uuid.uuid4())
            uuid.UUID(identifier)
            timestamp = _iso_timestamp(started_at or datetime.now(UTC))
            audio_sources = tuple(MeetingAudioSource(source) for source in capture.audio_sources)
        except Exception:
            if incognito:
                capture.path.unlink(missing_ok=True)
            raise
        recording_path = capture.path
        recording_filename: str | None = None
        if not incognito:
            try:
                recording_path = self.store.archive_recording(capture.path, identifier)
                recording_filename = recording_path.name
            except Exception as error:
                raise MeetingFailure(
                    "Meeting audio could not be placed in the private archive.",
                    stage="archive",
                    meeting=None,
                    retained_audio_path=capture.path if capture.path.exists() else None,
                ) from error
        try:
            try:
                transcription = self.elevenlabs.transcribe_meeting(
                    recording_path,
                    language_code=self.config.language_code,
                    model_id=self.config.transcription_model,
                )
            except Exception as error:
                if incognito:
                    raise MeetingFailure(
                        "Meeting transcription failed and Incognito audio was erased.",
                        stage="recognition",
                        meeting=None,
                        retained_audio_path=None,
                    ) from error
                failed_meeting = MeetingRecord(
                    identifier=identifier,
                    transcript="",
                    duration_seconds=capture.duration_seconds,
                    language=self.config.language_code,
                    audio_sources=audio_sources,
                    timestamp=timestamp,
                    recording_filename=recording_filename,
                    recognition_status=MeetingRecognitionStatus.FAILED,
                    warnings=capture.warnings,
                )
                try:
                    self.store.save(failed_meeting)
                except Exception as persistence_error:
                    raise MeetingFailure(
                        "Meeting transcription failed; its audio remains private but the archive index "
                        "could not be saved.",
                        stage="persistence",
                        meeting=failed_meeting,
                        retained_audio_path=recording_path,
                    ) from persistence_error
                raise MeetingFailure(
                    "Meeting transcription failed. Its private recording is retained for explicit retry.",
                    stage="recognition",
                    meeting=failed_meeting,
                    retained_audio_path=recording_path,
                ) from error
            meeting = _completed_meeting(
                identifier=identifier,
                timestamp=timestamp,
                capture=capture,
                transcription=transcription,
                audio_sources=audio_sources,
                recording_filename=recording_filename,
            )
            if not incognito:
                try:
                    self.store.save(meeting)
                except Exception as error:
                    raise MeetingFailure(
                        "Meeting transcript completed, but the private archive index could not be saved.",
                        stage="persistence",
                        meeting=meeting,
                        retained_audio_path=recording_path,
                    ) from error
            return MeetingWorkflowResult(meeting=meeting, transcription=transcription, incognito=incognito)
        finally:
            if incognito:
                recording_path.unlink(missing_ok=True)

    def retry(self, identifier: str) -> MeetingWorkflowResult:
        """Retry retained Meeting audio into the same review record without any delivery side effect."""
        meeting = self.store.find(identifier)
        recording_path = self.store.recording_path(meeting)
        if recording_path is None:
            raise MeetingFailure(
                "This meeting has no managed recording available for retry.",
                stage="recognition",
                meeting=meeting,
                retained_audio_path=None,
            )
        try:
            transcription = self.elevenlabs.transcribe_meeting(
                recording_path,
                language_code=meeting.language,
                model_id=self.config.transcription_model,
            )
        except Exception as error:
            raise MeetingFailure(
                "Meeting transcription failed again. Its private recording remains available.",
                stage="recognition",
                meeting=meeting,
                retained_audio_path=recording_path,
            ) from error
        recovered = replace(
            meeting,
            transcript=transcription.text,
            speakers=_meeting_speakers(transcription.speaker_segments),
            insights=extract_meeting_insights(transcription.text),
            language=transcription.language_code,
            recognition_status=MeetingRecognitionStatus.COMPLETED,
            transcription_id=transcription.transcription_id,
        )
        try:
            self.store.save(recovered)
        except Exception as error:
            raise MeetingFailure(
                "Meeting retry completed, but the private archive index could not be saved.",
                stage="persistence",
                meeting=recovered,
                retained_audio_path=recording_path,
            ) from error
        return MeetingWorkflowResult(meeting=recovered, transcription=transcription, incognito=False)


def extract_meeting_insights(transcript: str) -> MeetingInsights:
    """Extract explicit decisions and actions plus a literal three-sentence summary."""
    sentences = _sentence_segments(transcript)
    decisions = tuple(
        sentence for sentence in sentences if any(marker in sentence.casefold() for marker in _DECISION_MARKERS)
    )
    action_items = tuple(
        sentence
        for sentence in sentences
        if any(marker in sentence.casefold() for marker in _ACTION_MARKERS)
        or _NAMED_OWNER_PATTERN.search(sentence) is not None
    )
    return MeetingInsights(
        summary=" ".join(sentences[:3]),
        decisions=decisions,
        action_items=action_items,
    )


def _completed_meeting(
    *,
    identifier: str,
    timestamp: str,
    capture: MeetingCaptureResult,
    transcription: TranscriptionResult,
    audio_sources: tuple[MeetingAudioSource, ...],
    recording_filename: str | None,
) -> MeetingRecord:
    """Build one completed Meeting record from capture and provider facts."""
    duration_seconds = (
        capture.duration_seconds
        if capture.duration_seconds > 0 or transcription.audio_duration_seconds is None
        else transcription.audio_duration_seconds
    )
    return MeetingRecord(
        identifier=identifier,
        transcript=transcription.text,
        speakers=_meeting_speakers(transcription.speaker_segments),
        insights=extract_meeting_insights(transcription.text),
        timestamp=timestamp,
        duration_seconds=duration_seconds,
        language=transcription.language_code,
        audio_sources=audio_sources,
        recording_filename=recording_filename,
        recognition_status=MeetingRecognitionStatus.COMPLETED,
        transcription_id=transcription.transcription_id,
        warnings=capture.warnings,
    )


def _meeting_speakers(segments: tuple[SpeakerSegment, ...]) -> tuple[MeetingSpeakerSegment, ...]:
    """Map service segments into the stable cross-platform Meeting model."""
    return tuple(
        MeetingSpeakerSegment(
            speaker=segment.speaker,
            text=segment.text,
            started_at_seconds=segment.started_at_seconds,
            ended_at_seconds=segment.ended_at_seconds,
        )
        for segment in segments
    )


def _sentence_segments(transcript: str) -> list[str]:
    """Split normalized prose conservatively while preserving sentence punctuation."""
    normalized = re.sub(r"\s+", " ", transcript).strip()
    if not normalized:
        return []
    sentences = [sentence.strip() for sentence in re.split(r"(?<=[.!?])\s+", normalized)]
    return [sentence for sentence in sentences if sentence]


def _encode_meeting(meeting: MeetingRecord) -> dict[str, object]:
    """Encode macOS-compatible Meeting fields plus explicit Linux retry metadata."""
    return {
        "id": meeting.identifier,
        "title": meeting.title,
        "transcript": meeting.transcript,
        "speakers": [
            {
                "speaker": segment.speaker,
                "text": segment.text,
                "startedAt": segment.started_at_seconds,
                "endedAt": segment.ended_at_seconds,
            }
            for segment in meeting.speakers
        ],
        "insights": {
            "summary": meeting.insights.summary,
            "decisions": list(meeting.insights.decisions),
            "actionItems": list(meeting.insights.action_items),
        },
        "timestamp": meeting.timestamp,
        "duration": meeting.duration_seconds,
        "provider": meeting.provider,
        "language": meeting.language,
        "audioSources": [source.value for source in meeting.audio_sources],
        "recordingFilename": meeting.recording_filename,
        "recognitionStatus": meeting.recognition_status.value,
        "transcriptionID": meeting.transcription_id,
        "warnings": list(meeting.warnings),
    }


def _decode_meeting(payload: dict[str, object]) -> MeetingRecord:
    """Validate one persisted Meeting row without trusting paths or scalar types."""
    speakers_payload = _object_list(payload.get("speakers", []), "speakers")
    insights_payload = payload.get("insights", {})
    if not isinstance(insights_payload, dict):
        raise ValueError("Meeting insights must be an object.")
    source_values = _string_list(payload.get("audioSources", []), "audioSources")
    sources = tuple(
        MeetingAudioSource.SYSTEM if source == "system-audio" else MeetingAudioSource(source)
        for source in source_values
    )
    status_value = payload.get("recognitionStatus", MeetingRecognitionStatus.COMPLETED.value)
    if not isinstance(status_value, str):
        raise ValueError("Meeting recognitionStatus must be a string.")
    return MeetingRecord(
        identifier=_required_string(payload.get("id"), "id"),
        title=_optional_string(payload.get("title"), "title"),
        transcript=_required_string(payload.get("transcript"), "transcript", allow_empty=True),
        speakers=tuple(
            MeetingSpeakerSegment(
                speaker=_required_string(item.get("speaker"), "speaker"),
                text=_required_string(item.get("text"), "speaker text"),
                started_at_seconds=_number(item.get("startedAt"), "startedAt"),
                ended_at_seconds=_number(item.get("endedAt"), "endedAt"),
            )
            for item in speakers_payload
        ),
        insights=MeetingInsights(
            summary=_required_string(insights_payload.get("summary", ""), "summary", allow_empty=True),
            decisions=tuple(_string_list(insights_payload.get("decisions", []), "decisions")),
            action_items=tuple(_string_list(insights_payload.get("actionItems", []), "actionItems")),
        ),
        timestamp=_required_string(payload.get("timestamp"), "timestamp"),
        duration_seconds=_number(payload.get("duration"), "duration"),
        provider=_required_string(payload.get("provider"), "provider"),
        language=_required_string(payload.get("language"), "language"),
        audio_sources=sources,
        recording_filename=_optional_string(payload.get("recordingFilename"), "recordingFilename"),
        recognition_status=MeetingRecognitionStatus(status_value),
        transcription_id=_optional_string(payload.get("transcriptionID"), "transcriptionID"),
        warnings=tuple(_string_list(payload.get("warnings", []), "warnings")),
    )


def _meeting_markdown(meeting: MeetingRecord) -> str:
    """Render one cold-readable Meeting review document."""
    speakers = "\n".join(
        f"- [{_meeting_timestamp(segment.started_at_seconds)}] {segment.speaker}: {segment.text}"
        for segment in meeting.speakers
    )
    audio_sources = ", ".join(source.value for source in meeting.audio_sources) or "none"
    title = meeting.title or "Mluva meeting"
    return (
        f"# {title}\n\n"
        f"- Captured: {meeting.timestamp}\n"
        f"- Provider: {meeting.provider}\n"
        f"- Language: {meeting.language}\n"
        f"- Audio: {audio_sources}\n"
        f"- Recognition: {meeting.recognition_status.value}\n\n"
        f"## Summary\n\n{meeting.insights.summary}\n\n"
        f"## Decisions\n\n{_markdown_list(meeting.insights.decisions)}\n\n"
        f"## Action items\n\n{_markdown_list(meeting.insights.action_items)}\n\n"
        f"## Speakers\n\n{speakers or 'Speaker labels unavailable.'}\n\n"
        f"## Transcript\n\n{meeting.transcript}\n"
    )


def _markdown_list(values: tuple[str, ...]) -> str:
    """Render literal insight rows or an honest empty-state label."""
    return "\n".join(f"- {value}" for value in values) if values else "None recorded."


def _meeting_timestamp(seconds: float) -> str:
    """Format a non-negative segment offset as minutes and seconds."""
    total_seconds = max(0, int(seconds + 0.5))
    return f"{total_seconds // 60}:{total_seconds % 60:02d}"


def _iso_timestamp(value: datetime) -> str:
    """Normalize one timestamp to an ISO-8601 UTC archive value."""
    aware = value.replace(tzinfo=UTC) if value.tzinfo is None else value
    return aware.astimezone(UTC).isoformat().replace("+00:00", "Z")


def _is_safe_recording_filename(filename: str) -> bool:
    """Accept a single non-special basename and reject path traversal."""
    return bool(filename) and filename not in {".", ".."} and Path(filename).name == filename


def _create_private_file(path: Path) -> None:
    """Create one owner-only file without following an existing destination."""
    descriptor = os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    os.close(descriptor)


def _atomic_write_text(path: Path, content: str) -> None:
    """Atomically replace one owner-only text artifact."""
    temporary_path = path.with_name(f".{path.name}.{uuid.uuid4().hex}.tmp")
    try:
        _create_private_file(temporary_path)
        temporary_path.write_text(content, encoding="utf-8")
        temporary_path.chmod(0o600)
        os.replace(temporary_path, path)
        path.chmod(0o600)
    except Exception:
        temporary_path.unlink(missing_ok=True)
        raise


def _object_list(value: object, label: str) -> list[dict[str, object]]:
    """Validate one JSON array of objects."""
    if not isinstance(value, list) or not all(isinstance(item, dict) for item in value):
        raise ValueError(f"Meeting {label} must be an array of objects.")
    return value


def _string_list(value: object, label: str) -> list[str]:
    """Validate one JSON array of strings."""
    if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
        raise ValueError(f"Meeting {label} must be an array of strings.")
    return value


def _required_string(value: object, label: str, *, allow_empty: bool = False) -> str:
    """Validate one required JSON string with optional empty transcript support."""
    if not isinstance(value, str) or (not allow_empty and not value.strip()):
        raise ValueError(f"Meeting {label} must be a non-empty string.")
    return value


def _optional_string(value: object, label: str) -> str | None:
    """Validate one optional JSON string."""
    if value is None:
        return None
    if not isinstance(value, str):
        raise ValueError(f"Meeting {label} must be a string or null.")
    return value


def _number(value: object, label: str) -> float:
    """Validate one finite-enough JSON number while rejecting booleans."""
    if isinstance(value, bool) or not isinstance(value, int | float):
        raise ValueError(f"Meeting {label} must be a number.")
    number = float(value)
    if number != number or number in {float("inf"), float("-inf")}:
        raise ValueError(f"Meeting {label} must be finite.")
    return number
