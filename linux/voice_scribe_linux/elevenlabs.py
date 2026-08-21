"""ElevenLabs Scribe v2 batch transcription client."""

from __future__ import annotations

import json
import re
import urllib.error
import urllib.request
import uuid
from dataclasses import dataclass, field
from pathlib import Path

from voice_scribe_linux.brand import LINUX_USER_AGENT

SCRIBE_ENDPOINT = "https://api.elevenlabs.io/v1/speech-to-text"


class TranscriptionError(RuntimeError):
    """Report an ElevenLabs request or response that cannot produce text."""


@dataclass(frozen=True, slots=True)
class TranscriptionResult:
    """Preserve the text and service metadata returned by Scribe."""

    text: str
    language_code: str
    language_probability: float | None
    transcription_id: str | None
    speaker_segments: tuple[SpeakerSegment, ...] = ()
    audio_duration_seconds: float | None = None


@dataclass(frozen=True, slots=True)
class SpeakerSegment:
    """Preserve one contiguous diarized speaker segment with provider timing."""

    speaker: str
    text: str
    started_at_seconds: float
    ended_at_seconds: float


def encode_multipart(fields: dict[str, str], file_path: Path, boundary: str) -> bytes:
    """Encode the exact multipart body accepted by the batch transcription API."""
    chunks: list[bytes] = []
    for name, value in fields.items():
        chunks.extend(
            [
                f"--{boundary}\r\n".encode(),
                f'Content-Disposition: form-data; name="{name}"\r\n\r\n'.encode(),
                value.encode(),
                b"\r\n",
            ]
        )
    chunks.extend(
        [
            f"--{boundary}\r\n".encode(),
            f'Content-Disposition: form-data; name="file"; filename="{file_path.name}"\r\n'.encode(),
            b"Content-Type: audio/wav\r\n\r\n",
            file_path.read_bytes(),
            b"\r\n",
            f"--{boundary}--\r\n".encode(),
        ]
    )
    return b"".join(chunks)


@dataclass(frozen=True, slots=True)
class ElevenLabsClient:
    """Transcribe bounded local recordings using ElevenLabs Scribe v2."""

    api_key: str = field(repr=False)
    endpoint: str = SCRIBE_ENDPOINT
    timeout_seconds: float = 300

    def transcribe(self, file_path: Path, language_code: str, model_id: str = "scribe_v2") -> TranscriptionResult:
        """Upload one audio file and preserve the service's raw transcript as the result."""
        fields = {
            "model_id": model_id,
            "timestamps_granularity": "word",
            "tag_audio_events": "false",
            "diarize": "false",
        }
        if language_code != "auto":
            fields["language_code"] = language_code
        payload = self._request(
            file_path,
            fields,
        )
        return _transcription_result(payload, include_speakers=False)

    def transcribe_meeting(
        self,
        file_path: Path,
        language_code: str,
        model_id: str = "scribe_v2",
    ) -> TranscriptionResult:
        """Upload one explicit meeting recording with Scribe v2 speaker diarization."""
        fields = {
            "model_id": model_id,
            "timestamps_granularity": "word",
            "tag_audio_events": "false",
            "diarize": "true",
        }
        if language_code != "auto":
            fields["language_code"] = language_code
        payload = self._request(
            file_path,
            fields,
        )
        return _transcription_result(payload, include_speakers=True)

    def _request(self, file_path: Path, fields: dict[str, str]) -> dict[str, object]:
        """Send one reviewed multipart request and sanitize provider failures."""
        boundary = f"MluvaBoundary{uuid.uuid4().hex}"
        body = encode_multipart(fields, file_path, boundary)
        request = urllib.request.Request(
            self.endpoint,
            data=body,
            headers={
                "Content-Type": f"multipart/form-data; boundary={boundary}",
                "xi-api-key": self.api_key,
                "User-Agent": LINUX_USER_AGENT,
            },
            method="POST",
        )
        try:
            with urllib.request.urlopen(request, timeout=self.timeout_seconds) as response:
                payload = json.loads(response.read())
        except urllib.error.HTTPError as error:
            error.close()
            raise TranscriptionError(f"ElevenLabs transcription failed with HTTP {error.code}.") from error
        except (urllib.error.URLError, TimeoutError) as error:
            raise TranscriptionError("ElevenLabs transcription could not reach the service.") from error
        except (json.JSONDecodeError, UnicodeDecodeError) as error:
            raise TranscriptionError("ElevenLabs returned an invalid transcription response.") from error
        if not isinstance(payload, dict):
            raise TranscriptionError("ElevenLabs returned an invalid transcription response.")
        return payload


def _transcription_result(payload: dict[str, object], include_speakers: bool) -> TranscriptionResult:
    """Validate one Scribe response and optionally assemble diarized speaker segments."""
    text_value = payload.get("text")
    language_code = payload.get("language_code")
    if not isinstance(text_value, str) or not isinstance(language_code, str):
        raise TranscriptionError("ElevenLabs returned an invalid transcription response.")
    text = text_value.strip()
    if not text:
        raise TranscriptionError("ElevenLabs returned an empty transcript.")
    language_probability = payload.get("language_probability")
    transcription_id = payload.get("transcription_id")
    audio_duration_seconds = payload.get("audio_duration_secs")
    return TranscriptionResult(
        text=text,
        language_code=language_code,
        language_probability=(float(language_probability) if isinstance(language_probability, int | float) else None),
        transcription_id=transcription_id if isinstance(transcription_id, str) else None,
        speaker_segments=_speaker_segments(payload.get("words")) if include_speakers else (),
        audio_duration_seconds=(
            max(0.0, float(audio_duration_seconds)) if isinstance(audio_duration_seconds, int | float) else None
        ),
    )


def _speaker_segments(words_value: object) -> tuple[SpeakerSegment, ...]:
    """Group contiguous Scribe word and spacing entries by speaker identifier."""
    if not isinstance(words_value, list):
        return ()
    segments: list[SpeakerSegment] = []
    current_speaker: str | None = None
    current_text: list[str] = []
    started_at_seconds = 0.0
    ended_at_seconds = 0.0
    for value in words_value:
        if not isinstance(value, dict) or value.get("type") not in {"word", "spacing"}:
            continue
        text = value.get("text")
        if not isinstance(text, str):
            continue
        speaker_value = value.get("speaker_id")
        speaker = speaker_value if isinstance(speaker_value, str) and speaker_value else current_speaker
        if speaker is None:
            speaker = "speaker_unknown"
        start_value = value.get("start")
        end_value = value.get("end")
        start = max(0.0, float(start_value)) if isinstance(start_value, int | float) else ended_at_seconds
        end = max(start, float(end_value)) if isinstance(end_value, int | float) else start
        if current_speaker is not None and speaker != current_speaker:
            _append_speaker_segment(
                segments,
                current_speaker,
                current_text,
                started_at_seconds,
                ended_at_seconds,
            )
            current_text = []
            started_at_seconds = start
        elif current_speaker is None:
            started_at_seconds = start
        current_speaker = speaker
        current_text.append(text)
        ended_at_seconds = max(ended_at_seconds, end)
    if current_speaker is not None:
        _append_speaker_segment(
            segments,
            current_speaker,
            current_text,
            started_at_seconds,
            ended_at_seconds,
        )
    return tuple(segments)


def _append_speaker_segment(
    segments: list[SpeakerSegment],
    speaker_identifier: str,
    text_parts: list[str],
    started_at_seconds: float,
    ended_at_seconds: float,
) -> None:
    """Append one non-empty human-labelled speaker segment."""
    text = "".join(text_parts).strip()
    if not text:
        return
    match = re.fullmatch(r"speaker_(\d+)", speaker_identifier)
    speaker = f"Speaker {int(match.group(1)) + 1}" if match is not None else speaker_identifier
    segments.append(
        SpeakerSegment(
            speaker=speaker,
            text=text,
            started_at_seconds=started_at_seconds,
            ended_at_seconds=max(started_at_seconds, ended_at_seconds),
        )
    )
