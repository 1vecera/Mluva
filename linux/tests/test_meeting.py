"""Headless coverage for explicit Meeting records, recovery, and privacy."""

import json
import stat
from datetime import UTC, datetime
from pathlib import Path

import pytest

from voice_scribe_linux.audio import MeetingCaptureResult
from voice_scribe_linux.config import AppConfig
from voice_scribe_linux.elevenlabs import SpeakerSegment, TranscriptionResult
from voice_scribe_linux.meeting import (
    MeetingAudioSource,
    MeetingFailure,
    MeetingRecognitionStatus,
    MeetingRecord,
    MeetingStore,
    MeetingWorkflow,
    extract_meeting_insights,
)


class StaticMeetingClient:
    """Return deterministic diarized recognition while recording request arguments."""

    def __init__(self, result: TranscriptionResult) -> None:
        """Store one immutable provider result."""
        self.result = result
        self.calls: list[tuple[Path, str, str]] = []

    def transcribe_meeting(self, file_path: Path, language_code: str, model_id: str) -> TranscriptionResult:
        """Record the explicit Meeting request and return configured recognition."""
        self.calls.append((file_path, language_code, model_id))
        return self.result


class RecoveringMeetingClient:
    """Fail once with sensitive detail, then return deterministic retry recognition."""

    def __init__(self, result: TranscriptionResult) -> None:
        """Store the retry result and initial failed state."""
        self.result = result
        self.should_fail = True

    def transcribe_meeting(self, _file_path: Path, language_code: str, model_id: str) -> TranscriptionResult:
        """Exercise controlled failure followed by explicit recovery."""
        assert language_code == "eng"
        assert model_id == "scribe_v2"
        if self.should_fail:
            raise RuntimeError("sensitive upstream diagnostic")
        return self.result


def transcription_result() -> TranscriptionResult:
    """Build representative Scribe output with provider speaker timing."""
    return TranscriptionResult(
        text=(
            "We reviewed the launch readiness. We decided to ship on Friday. "
            "Action item: Daniel will update the release notes. Marta will confirm the support rota."
        ),
        language_code="eng",
        language_probability=0.99,
        transcription_id="meeting-scribe",
        speaker_segments=(
            SpeakerSegment("Speaker 1", "We reviewed the launch readiness.", 0.0, 2.2),
            SpeakerSegment("Speaker 2", "We decided to ship on Friday.", 2.3, 4.6),
        ),
        audio_duration_seconds=8.0,
    )


def capture_result(path: Path) -> MeetingCaptureResult:
    """Create one finalized mixed-audio result without opening a live audio device."""
    path.write_bytes(b"private-meeting-audio")
    path.chmod(0o600)
    return MeetingCaptureResult(
        path=path,
        audio_sources=("microphone", "system"),
        warnings=(),
        duration_seconds=7.5,
    )


def test_insights_extract_only_literal_decisions_actions_and_summary() -> None:
    """Match the macOS deterministic insight contract without generating facts."""
    insights = extract_meeting_insights(transcription_result().text)

    assert insights.summary == (
        "We reviewed the launch readiness. We decided to ship on Friday. "
        "Action item: Daniel will update the release notes."
    )
    assert insights.decisions == ("We decided to ship on Friday.",)
    assert insights.action_items == (
        "Action item: Daniel will update the release notes.",
        "Marta will confirm the support rota.",
    )
    neutral = extract_meeting_insights("We explored several directions. The team shared open questions.")
    assert neutral.decisions == ()
    assert neutral.action_items == ()


def test_store_round_trips_mac_compatible_fields_and_private_permissions(tmp_path: Path) -> None:
    """Persist Meeting data separately with owner-only archive permissions."""
    store = MeetingStore(tmp_path / "meetings" / "meetings.json")
    recording_directory = tmp_path / "meetings" / "recordings"
    recording_directory.mkdir(parents=True)
    recording = recording_directory / "meeting.wav"
    recording.write_bytes(b"audio")
    meeting = MeetingRecord(
        identifier="31c1dadf-09ad-46ee-8373-1476084bd9ba",
        title="  Launch review  ",
        transcript="We decided to ship Friday.",
        duration_seconds=2.4,
        provider="googleCloud",
        language="en-US",
        audio_sources=(MeetingAudioSource.SYSTEM, MeetingAudioSource.MICROPHONE),
        timestamp="2027-01-15T08:00:00Z",
        recording_filename="meeting.wav",
    )

    store.save(meeting)
    reloaded = MeetingStore(store.path)

    assert reloaded.meetings == [meeting]
    assert meeting.title == "Launch review"
    assert meeting.audio_sources == (MeetingAudioSource.MICROPHONE, MeetingAudioSource.SYSTEM)
    assert reloaded.recording_path(meeting) == recording
    payload = json.loads(store.path.read_text(encoding="utf-8"))
    assert payload[0]["audioSources"] == ["microphone", "system"]
    assert payload[0]["duration"] == 2.4
    assert stat.S_IMODE(store.path.stat().st_mode) == 0o600
    assert stat.S_IMODE(store.path.parent.stat().st_mode) == 0o700


def test_store_loads_mac_record_without_linux_retry_fields(tmp_path: Path) -> None:
    """Decode the existing macOS Meeting schema when Linux-only metadata is absent."""
    path = tmp_path / "meetings.json"
    path.write_text(
        json.dumps(
            [
                {
                    "id": "b459cbb7-c807-40bf-8e53-b89a32046c96",
                    "title": None,
                    "transcript": "Meeting transcript.",
                    "speakers": [{"speaker": "Speaker 1", "text": "Meeting transcript.", "startedAt": 0, "endedAt": 1}],
                    "insights": {"summary": "Meeting transcript.", "decisions": [], "actionItems": []},
                    "timestamp": "2027-01-15T08:00:00Z",
                    "duration": 1,
                    "provider": "googleCloud",
                    "language": "en-US",
                    "audioSources": ["microphone", "system"],
                    "recordingFilename": None,
                }
            ]
        ),
        encoding="utf-8",
    )

    meeting = MeetingStore(path).meetings[0]

    assert meeting.provider == "googleCloud"
    assert meeting.recognition_status is MeetingRecognitionStatus.COMPLETED
    assert meeting.audio_sources == (MeetingAudioSource.MICROPHONE, MeetingAudioSource.SYSTEM)


def test_store_preserves_malformed_archive_and_rejects_path_escape(tmp_path: Path) -> None:
    """Fail closed on malformed JSON and never resolve a retained path outside the archive."""
    path = tmp_path / "meetings" / "meetings.json"
    path.parent.mkdir()
    corrupt = b"not a meeting archive"
    path.write_bytes(corrupt)
    store = MeetingStore(path)
    escaped = MeetingRecord(
        transcript="Unsafe",
        duration_seconds=1,
        recording_filename="../outside.wav",
    )

    assert store.persistence_error is not None
    assert store.recording_path(escaped) is None
    with pytest.raises(RuntimeError, match="malformed archive"):
        store.save(MeetingRecord(transcript="New meeting", duration_seconds=1))
    assert path.read_bytes() == corrupt


def test_completed_meeting_archives_audio_and_diarization_without_delivery(tmp_path: Path) -> None:
    """Archive explicit Meeting output and keep it independent from dictation delivery."""
    source_path = tmp_path / "capture.wav"
    capture = capture_result(source_path)
    store = MeetingStore(tmp_path / "data" / "meetings.json")
    client = StaticMeetingClient(transcription_result())
    workflow = MeetingWorkflow(AppConfig(), client, store)

    result = workflow.complete(
        capture,
        incognito=False,
        started_at=datetime(2027, 1, 15, 8, 0, tzinfo=UTC),
        identifier="0f08885a-72d9-4648-bb3c-99cc5e5e65e8",
    )

    archived_path = store.recording_path(result.meeting)
    assert archived_path is not None
    assert archived_path.read_bytes() == b"private-meeting-audio"
    assert not source_path.exists()
    assert client.calls == [(archived_path, "eng", "scribe_v2")]
    assert result.meeting.transcript == transcription_result().text
    assert result.meeting.speakers[1].speaker == "Speaker 2"
    assert result.meeting.insights.decisions == ("We decided to ship on Friday.",)
    assert result.meeting.duration_seconds == 7.5
    assert result.meeting.audio_sources == (MeetingAudioSource.MICROPHONE, MeetingAudioSource.SYSTEM)
    assert result.meeting.recognition_status is MeetingRecognitionStatus.COMPLETED
    assert store.meetings == [result.meeting]


def test_incognito_meeting_erases_audio_and_creates_no_archive(tmp_path: Path) -> None:
    """Return an in-memory result while retaining neither audio nor Meeting metadata."""
    source_path = tmp_path / "incognito.wav"
    capture = capture_result(source_path)
    store = MeetingStore(tmp_path / "data" / "meetings.json")
    workflow = MeetingWorkflow(AppConfig(), StaticMeetingClient(transcription_result()), store)

    result = workflow.complete(capture, incognito=True)

    assert result.incognito
    assert result.meeting.recording_filename is None
    assert not source_path.exists()
    assert store.meetings == []
    assert not store.path.exists()


def test_failed_meeting_retains_private_audio_and_explicit_retry_recovers(tmp_path: Path) -> None:
    """Store a controlled failed record, then update that same record on explicit retry."""
    source_path = tmp_path / "retry.wav"
    capture = capture_result(source_path)
    store = MeetingStore(tmp_path / "data" / "meetings.json")
    client = RecoveringMeetingClient(transcription_result())
    workflow = MeetingWorkflow(AppConfig(), client, store)

    with pytest.raises(MeetingFailure, match="retained for explicit retry") as error_info:
        workflow.complete(
            capture,
            incognito=False,
            identifier="ccdf1bbc-1e37-432e-8a3d-8fe0091a9d56",
        )

    failure = error_info.value
    assert "sensitive upstream diagnostic" not in str(failure)
    assert failure.stage == "recognition"
    assert failure.meeting is not None
    assert failure.meeting.recognition_status is MeetingRecognitionStatus.FAILED
    assert failure.retained_audio_path is not None
    assert failure.retained_audio_path.is_file()
    assert store.meetings == [failure.meeting]

    client.should_fail = False
    recovered = workflow.retry(failure.meeting.identifier)

    assert recovered.meeting.identifier == failure.meeting.identifier
    assert recovered.meeting.recognition_status is MeetingRecognitionStatus.COMPLETED
    assert recovered.meeting.transcript == transcription_result().text
    assert store.meetings == [recovered.meeting]
    assert store.recording_path(recovered.meeting) == failure.retained_audio_path


def test_failed_incognito_meeting_erases_audio_and_provider_detail(tmp_path: Path) -> None:
    """Erase failed Incognito capture while exposing only a controlled local error."""
    source_path = tmp_path / "failed-incognito.wav"
    capture = capture_result(source_path)
    store = MeetingStore(tmp_path / "data" / "meetings.json")
    workflow = MeetingWorkflow(AppConfig(), RecoveringMeetingClient(transcription_result()), store)

    with pytest.raises(MeetingFailure, match="Incognito audio was erased") as error_info:
        workflow.complete(capture, incognito=True)

    assert "sensitive upstream diagnostic" not in str(error_info.value)
    assert not source_path.exists()
    assert store.meetings == []
    assert not store.path.exists()


def test_incognito_erases_audio_when_internal_capture_metadata_is_invalid(tmp_path: Path) -> None:
    """Keep erasure fail-closed even before a provider request can begin."""
    source_path = tmp_path / "invalid-incognito.wav"
    source_path.write_bytes(b"private")
    capture = MeetingCaptureResult(
        path=source_path,
        audio_sources=("unsupported-source",),
        warnings=(),
        duration_seconds=1,
    )
    store = MeetingStore(tmp_path / "data" / "meetings.json")
    workflow = MeetingWorkflow(AppConfig(), StaticMeetingClient(transcription_result()), store)

    with pytest.raises(ValueError):
        workflow.complete(capture, incognito=True)

    assert not source_path.exists()
    assert store.meetings == []


def test_export_and_delete_touch_only_selected_managed_recording(tmp_path: Path) -> None:
    """Produce private review artifacts and delete only a validated selected recording."""
    store = MeetingStore(tmp_path / "data" / "meetings.json")
    recordings = tmp_path / "data" / "recordings"
    recordings.mkdir(parents=True)
    selected_audio = recordings / "selected.wav"
    other_audio = recordings / "other.wav"
    selected_audio.write_bytes(b"selected")
    other_audio.write_bytes(b"other")
    meeting = MeetingRecord(
        transcript="We decided to ship.",
        duration_seconds=1,
        recording_filename="selected.wav",
        insights=extract_meeting_insights("We decided to ship."),
    )
    store.save(meeting)

    markdown_path = store.export(meeting, tmp_path / "exports", "markdown")
    json_path = store.export(meeting, tmp_path / "exports", "json")

    assert markdown_path.name.startswith("mluva-meeting-")
    assert json_path.name.startswith("mluva-meeting-")
    assert "## Decisions\n\n- We decided to ship." in markdown_path.read_text(encoding="utf-8")
    assert json.loads(json_path.read_text(encoding="utf-8"))["id"] == meeting.identifier
    assert stat.S_IMODE(markdown_path.stat().st_mode) == 0o600
    assert stat.S_IMODE(json_path.stat().st_mode) == 0o600
    store.delete(meeting.identifier)
    assert not selected_audio.exists()
    assert other_audio.read_bytes() == b"other"
