"""Process-level coverage for PipeWire recording finalization."""

import json
import stat
import time
import wave
from array import array
from pathlib import Path

import pytest

from voice_scribe_linux.audio import (
    PIPEWIRE_SYSTEM_CAPTURE_PROPERTIES,
    AudioCaptureError,
    PipeWireMeetingRecorder,
    PipeWireRecorder,
    pcm16_audio_level,
)


def wait_for_wav(path: Path) -> None:
    """Wait for the fake subprocess to finalize its deterministic WAV header."""
    deadline = time.monotonic() + 2
    while time.monotonic() < deadline:
        try:
            with wave.open(str(path), "rb") as recording:
                if recording.getnframes() > 0:
                    return
        except (EOFError, OSError, wave.Error):
            pass
        time.sleep(0.01)
    raise AssertionError(f"Fake recorder did not create {path.name}")


def test_stop_accepts_fedora_pw_record_finalization(tmp_path: Path) -> None:
    """Accept status one only when PipeWire already finalized non-empty audio."""
    output_path = tmp_path / "capture.wav"
    fake_recorder = Path(__file__).with_name("fake_pw_record.py")
    recorder = PipeWireRecorder(executable=str(fake_recorder), target="configured.microphone")
    chunks: list[bytes] = []
    levels: list[float] = []
    recorder.start(output_path, lambda chunk, level: (chunks.append(chunk), levels.append(level)))
    wait_for_wav(output_path)
    assert recorder.process is not None
    assert isinstance(recorder.process.args, list)
    assert recorder.process.args[recorder.process.args.index("--target") + 1] == "configured.microphone"
    assert recorder.process.args[-2:] == ["--raw", "-"]
    assert recorder.stop() == output_path
    assert output_path.stat().st_size > 44
    assert stat.S_IMODE(output_path.stat().st_mode) == 0o600
    assert chunks
    assert all(len(chunk) == 3_200 for chunk in chunks)
    assert levels == pytest.approx([1_000 / 32_768] * len(levels))
    with wave.open(str(output_path), "rb") as recording:
        assert recording.getnchannels() == 1
        assert recording.getsampwidth() == 2
        assert recording.getframerate() == 16_000
        assert recording.getnframes() >= 1_600


def test_cancel_terminates_capture_and_erases_partial_audio(tmp_path: Path) -> None:
    """Guarantee Escape-style cancellation leaves no audio available for recognition."""
    output_path = tmp_path / "cancelled.wav"
    fake_recorder = Path(__file__).with_name("fake_pw_record.py")
    recorder = PipeWireRecorder(executable=str(fake_recorder))
    recorder.start(output_path)
    wait_for_wav(output_path)

    recorder.cancel()

    assert recorder.process is None
    assert recorder.output_path is None
    assert not output_path.exists()


def test_slow_consumer_failure_never_stops_local_audio_drain(tmp_path: Path) -> None:
    """Disable a failed optional stream callback while continuing the recoverable WAV."""
    output_path = tmp_path / "callback-failed.wav"
    fake_recorder = Path(__file__).with_name("fake_pw_record.py")
    callback_calls = 0

    def fail_once(_chunk: bytes, _level: float) -> None:
        nonlocal callback_calls
        callback_calls += 1
        raise RuntimeError("synthetic optional consumer failure")

    recorder = PipeWireRecorder(executable=str(fake_recorder))
    recorder.start(output_path, fail_once)
    wait_for_wav(output_path)
    recorder.stop()

    assert callback_calls == 1
    with wave.open(str(output_path), "rb") as recording:
        assert recording.getnframes() >= 1_600


def test_pcm16_audio_level_is_bounded_and_handles_empty_frames() -> None:
    """Match the waveform projection to normalized RMS without retaining samples."""
    samples = array("h", (-32_768, 32_767))
    assert pcm16_audio_level(b"") == 0.0
    assert pcm16_audio_level(samples.tobytes()) == pytest.approx(0.9999847413)


def test_meeting_capture_requests_sink_audio_and_mixes_bounded_pcm(tmp_path: Path) -> None:
    """Prove explicit Meeting starts two processes and locally mixes aligned samples."""
    output_path = tmp_path / "meeting.wav"
    fake_recorder = Path(__file__).with_name("fake_meeting_pw_record.py")
    recorder = PipeWireMeetingRecorder(
        executable=str(fake_recorder),
        microphone_target="configured.microphone",
        system_target="configured.output",
    )

    recorder.start(output_path)
    assert recorder.microphone_path is not None
    assert recorder.system_path is not None
    assert recorder.system_process is not None
    wait_for_wav(recorder.microphone_path)
    wait_for_wav(recorder.system_path)
    assert stat.S_IMODE(recorder.microphone_path.stat().st_mode) == 0o600
    assert stat.S_IMODE(recorder.system_path.stat().st_mode) == 0o600
    system_arguments = recorder.system_process.args
    assert isinstance(system_arguments, list)
    assert PIPEWIRE_SYSTEM_CAPTURE_PROPERTIES in system_arguments
    assert json.loads(PIPEWIRE_SYSTEM_CAPTURE_PROPERTIES) == {"stream.capture.sink": True}
    assert system_arguments[system_arguments.index("--target") + 1] == "configured.output"
    assert recorder.microphone_process is not None
    microphone_arguments = recorder.microphone_process.args
    assert isinstance(microphone_arguments, list)
    assert microphone_arguments[microphone_arguments.index("--target") + 1] == "configured.microphone"
    result = recorder.stop()

    assert result.path == output_path
    assert result.audio_sources == ("microphone", "system")
    assert result.warnings == ()
    assert result.duration_seconds == pytest.approx(8 / 16_000)
    assert stat.S_IMODE(output_path.stat().st_mode) == 0o600
    with wave.open(str(output_path), "rb") as recording:
        samples = array("h")
        samples.frombytes(recording.readframes(recording.getnframes()))
    assert samples.tolist() == [2_000] * 6 + [1_000] * 2
    assert not (tmp_path / ".meeting.wav.microphone.part.wav").exists()
    assert not (tmp_path / ".meeting.wav.system.part.wav").exists()


def test_meeting_capture_reports_partial_source_without_overstating_it(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Preserve valid microphone audio when the fake sink-monitor stream fails."""
    monkeypatch.setenv("VOICE_SCRIBE_FAKE_SYSTEM_FAILURE", "1")
    output_path = tmp_path / "partial-meeting.wav"
    fake_recorder = Path(__file__).with_name("fake_meeting_pw_record.py")
    recorder = PipeWireMeetingRecorder(executable=str(fake_recorder))

    recorder.start(output_path)
    assert recorder.microphone_path is not None
    wait_for_wav(recorder.microphone_path)
    assert recorder.system_process is not None
    recorder.system_process.wait(timeout=2)
    result = recorder.stop()

    assert result.audio_sources == ("microphone",)
    assert result.warnings == ("System audio was unavailable; this meeting contains microphone audio only.",)
    with wave.open(str(output_path), "rb") as recording:
        samples = array("h")
        samples.frombytes(recording.readframes(recording.getnframes()))
    assert samples.tolist() == [1_000] * 8


def test_cancel_meeting_erases_both_sources_and_final_destination(tmp_path: Path) -> None:
    """Guarantee explicit cancellation leaves no Meeting audio artifact behind."""
    output_path = tmp_path / "cancelled-meeting.wav"
    fake_recorder = Path(__file__).with_name("fake_meeting_pw_record.py")
    recorder = PipeWireMeetingRecorder(executable=str(fake_recorder))
    recorder.start(output_path)
    assert recorder.microphone_path is not None
    assert recorder.system_path is not None
    microphone_path = recorder.microphone_path
    system_path = recorder.system_path
    wait_for_wav(microphone_path)
    wait_for_wav(system_path)

    recorder.cancel()

    assert recorder.microphone_process is None
    assert recorder.system_process is None
    assert recorder.output_path is None
    assert not microphone_path.exists()
    assert not system_path.exists()
    assert not output_path.exists()


def test_meeting_capture_fails_closed_when_both_sources_are_invalid(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Reject an empty Meeting and erase all paths before any provider can receive it."""
    monkeypatch.setenv("VOICE_SCRIBE_FAKE_MICROPHONE_FAILURE", "1")
    monkeypatch.setenv("VOICE_SCRIBE_FAKE_SYSTEM_FAILURE", "1")
    output_path = tmp_path / "invalid-meeting.wav"
    fake_recorder = Path(__file__).with_name("fake_meeting_pw_record.py")
    recorder = PipeWireMeetingRecorder(executable=str(fake_recorder))
    recorder.start(output_path)
    assert recorder.microphone_process is not None
    assert recorder.system_process is not None
    recorder.microphone_process.wait(timeout=2)
    recorder.system_process.wait(timeout=2)

    with pytest.raises(AudioCaptureError, match="no usable microphone or system audio"):
        recorder.stop()

    assert recorder.microphone_process is None
    assert recorder.system_process is None
    assert not output_path.exists()
    assert not (tmp_path / ".invalid-meeting.wav.microphone.part.wav").exists()
    assert not (tmp_path / ".invalid-meeting.wav.system.part.wav").exists()
