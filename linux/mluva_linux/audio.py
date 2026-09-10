"""PipeWire microphone and explicit Meeting capture for Linux."""

import os
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import wave
from array import array
from collections.abc import Callable
from dataclasses import dataclass, field
from math import sqrt
from pathlib import Path
from typing import BinaryIO

SAMPLE_RATE = 16_000
CHANNEL_COUNT = 1
SAMPLE_WIDTH_BYTES = 2
PIPEWIRE_SYSTEM_CAPTURE_PROPERTIES = '{ "stream.capture.sink": true }'
ACCEPTED_PIPEWIRE_FINALIZATION_CODES = frozenset((0, 1, -signal.SIGINT))
REALTIME_CHUNK_FRAMES = SAMPLE_RATE // 10
REALTIME_CHUNK_BYTES = REALTIME_CHUNK_FRAMES * CHANNEL_COUNT * SAMPLE_WIDTH_BYTES

AudioChunkCallback = Callable[[bytes, float], None]


class AudioCaptureError(RuntimeError):
    """Report a failed or invalid microphone capture."""


@dataclass(frozen=True, slots=True)
class MeetingCaptureResult:
    """Describe a finalized Meeting recording without overstating captured sources."""

    path: Path
    audio_sources: tuple[str, ...]
    warnings: tuple[str, ...]
    duration_seconds: float


@dataclass(slots=True)
class PipeWireRecorder:
    """Capture raw PipeWire PCM into a private WAV while exposing bounded live chunks."""

    executable: str
    target: str | None = None
    process: subprocess.Popen[bytes] | None = field(default=None, init=False, repr=False)
    output_path: Path | None = field(default=None, init=False)
    audio_level: float = field(default=0.0, init=False)
    _audio_callback: AudioChunkCallback | None = field(default=None, init=False, repr=False)
    _reader_thread: threading.Thread | None = field(default=None, init=False, repr=False)
    _reader_error: Exception | None = field(default=None, init=False, repr=False)
    _stderr_file: BinaryIO | None = field(default=None, init=False, repr=False)

    @classmethod
    def from_system(cls, target: str | None = None) -> "PipeWireRecorder":
        """Resolve the PipeWire recorder installed by supported Linux desktops."""
        executable = shutil.which("pw-record")
        if executable is None:
            raise AudioCaptureError("pw-record is required. Install PipeWire tools for your distribution.")
        return cls(executable=executable, target=target)

    def start(self, output_path: Path, on_audio_chunk: AudioChunkCallback | None = None) -> None:
        """Begin one microphone capture and drain every PCM chunk independently of consumers."""
        if self.process is not None:
            raise AudioCaptureError("A recording is already active.")
        output_path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        output_path.parent.chmod(0o700)
        if output_path.exists():
            raise AudioCaptureError("The recording destination already exists.")
        _create_private_file(output_path)
        self.output_path = output_path
        self.audio_level = 0.0
        self._audio_callback = on_audio_chunk
        self._reader_error = None
        self._stderr_file = tempfile.TemporaryFile(mode="w+b")
        try:
            process = subprocess.Popen(
                _pw_record_raw_command(self.executable, target=self.target),
                stdout=subprocess.PIPE,
                stderr=self._stderr_file,
                bufsize=0,
                umask=0o077,
            )
        except Exception:
            self._close_stderr_file()
            self.output_path = None
            self._audio_callback = None
            output_path.unlink(missing_ok=True)
            raise
        self.process = process
        self._reader_thread = threading.Thread(
            target=self._drain_raw_audio,
            args=(process, output_path),
            name="pipewire-pcm-drain",
            daemon=True,
        )
        self._reader_thread.start()

    def stop(self) -> Path:
        """Finalize the WAV and return it only when capture produced audio."""
        if self.process is None or self.output_path is None:
            raise AudioCaptureError("No recording is active.")
        process = self.process
        output_path = self.output_path
        if process.poll() is None:
            try:
                process.send_signal(signal.SIGINT)
            except ProcessLookupError:
                pass
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
        self._join_reader()
        stderr = self._read_stderr()
        reader_error = self._reader_error
        self._clear_streaming_state()
        if reader_error is not None:
            raise AudioCaptureError("The microphone audio stream could not be saved.") from reader_error
        if not _is_compatible_pcm_wav(output_path):
            message = stderr.decode(errors="replace").strip()
            raise AudioCaptureError(message or "The microphone capture contained no audio.")
        if process.returncode not in ACCEPTED_PIPEWIRE_FINALIZATION_CODES:
            raise AudioCaptureError(stderr.decode(errors="replace").strip() or "PipeWire microphone capture failed.")
        return output_path

    def cancel(self) -> None:
        """Stop capture and remove audio when the user cancels dictation."""
        if self.process is None or self.output_path is None:
            return
        process = self.process
        output_path = self.output_path
        _terminate_process_without_communicate(process)
        self._join_reader()
        self._clear_streaming_state()
        output_path.unlink(missing_ok=True)

    def _drain_raw_audio(self, process: subprocess.Popen[bytes], output_path: Path) -> None:
        """Drain stdout continuously so a slow optional consumer cannot stall PipeWire."""
        stdout = process.stdout
        if stdout is None:
            self._reader_error = AudioCaptureError("PipeWire did not expose an audio stream.")
            return
        pending_frames = bytearray()
        try:
            with wave.open(str(output_path), "wb") as recording:
                recording.setnchannels(CHANNEL_COUNT)
                recording.setsampwidth(SAMPLE_WIDTH_BYTES)
                recording.setframerate(SAMPLE_RATE)
                while True:
                    incoming = stdout.read(REALTIME_CHUNK_BYTES)
                    if not incoming:
                        break
                    pending_frames.extend(incoming)
                    while len(pending_frames) >= REALTIME_CHUNK_BYTES:
                        complete_frames = bytes(pending_frames[:REALTIME_CHUNK_BYTES])
                        del pending_frames[:REALTIME_CHUNK_BYTES]
                        self._write_and_publish_audio(recording, complete_frames)
                complete_length = len(pending_frames) - (len(pending_frames) % SAMPLE_WIDTH_BYTES)
                if complete_length > 0:
                    self._write_and_publish_audio(recording, bytes(pending_frames[:complete_length]))
            output_path.chmod(0o600)
        except Exception as error:
            self._reader_error = error
        finally:
            stdout.close()

    def _write_and_publish_audio(self, recording: wave.Wave_write, frames: bytes) -> None:
        """Persist one complete chunk before offering its RMS and samples to an optional consumer."""
        recording.writeframesraw(frames)
        level = pcm16_audio_level(frames)
        self.audio_level = level
        callback = self._audio_callback
        if callback is not None:
            try:
                callback(frames, level)
            except Exception:
                self._audio_callback = None

    def _join_reader(self) -> None:
        """Wait for the stdout drain to finish after the child closes its pipe."""
        reader_thread = self._reader_thread
        if reader_thread is not None:
            reader_thread.join(timeout=5)
            if reader_thread.is_alive() and self._reader_error is None:
                self._reader_error = AudioCaptureError("PipeWire audio finalization timed out.")

    def _read_stderr(self) -> bytes:
        """Read the anonymous PipeWire error stream without persisting it."""
        stderr_file = self._stderr_file
        if stderr_file is None:
            return b""
        try:
            stderr_file.seek(0)
            return stderr_file.read()
        finally:
            stderr_file.close()
            self._stderr_file = None

    def _close_stderr_file(self) -> None:
        """Close an anonymous error stream during failed setup."""
        if self._stderr_file is not None:
            self._stderr_file.close()
            self._stderr_file = None

    def _clear_streaming_state(self) -> None:
        """Release per-capture process state without deleting the finalized output."""
        self._close_stderr_file()
        self.process = None
        self.output_path = None
        self.audio_level = 0.0
        self._audio_callback = None
        self._reader_thread = None
        self._reader_error = None


@dataclass(slots=True)
class PipeWireMeetingRecorder:
    """Capture configured microphone and sink output only after explicit Meeting start."""

    executable: str
    microphone_target: str | None = None
    system_target: str | None = None
    microphone_process: subprocess.Popen[bytes] | None = None
    system_process: subprocess.Popen[bytes] | None = None
    output_path: Path | None = None
    microphone_path: Path | None = None
    system_path: Path | None = None

    @classmethod
    def from_system(
        cls,
        microphone_target: str | None = None,
        system_target: str | None = None,
    ) -> "PipeWireMeetingRecorder":
        """Resolve the PipeWire recorder without enumerating or opening live devices."""
        executable = shutil.which("pw-record")
        if executable is None:
            raise AudioCaptureError("pw-record is required. Install PipeWire tools for your distribution.")
        return cls(
            executable=executable,
            microphone_target=microphone_target,
            system_target=system_target,
        )

    def start(self, output_path: Path) -> None:
        """Start both Meeting sources behind an explicit, non-dictation API boundary."""
        if self.microphone_process is not None or self.system_process is not None:
            raise AudioCaptureError("A meeting recording is already active.")
        if output_path.exists():
            raise AudioCaptureError("The meeting recording destination already exists.")
        output_path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        output_path.parent.chmod(0o700)
        microphone_path = output_path.with_name(f".{output_path.name}.microphone.part.wav")
        system_path = output_path.with_name(f".{output_path.name}.system.part.wav")
        if microphone_path.exists() or system_path.exists():
            raise AudioCaptureError("Unfinished meeting capture files already exist at this destination.")
        self.output_path = output_path
        self.microphone_path = microphone_path
        self.system_path = system_path
        try:
            self.microphone_process = subprocess.Popen(
                _pw_record_command(
                    self.executable,
                    microphone_path,
                    target=self.microphone_target,
                ),
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                umask=0o077,
            )
            self.system_process = subprocess.Popen(
                _pw_record_command(
                    self.executable,
                    system_path,
                    target=self.system_target,
                    properties=PIPEWIRE_SYSTEM_CAPTURE_PROPERTIES,
                ),
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                umask=0o077,
            )
        except Exception as error:
            self.cancel()
            raise AudioCaptureError("Meeting capture could not start both PipeWire streams.") from error

    def stop(self) -> MeetingCaptureResult:
        """Finalize, validate, and locally mix the sources without touching text delivery."""
        if (
            self.microphone_process is None
            or self.system_process is None
            or self.output_path is None
            or self.microphone_path is None
            or self.system_path is None
        ):
            raise AudioCaptureError("No meeting recording is active.")
        microphone_process = self.microphone_process
        system_process = self.system_process
        output_path = self.output_path
        microphone_path = self.microphone_path
        system_path = self.system_path
        succeeded = False
        try:
            microphone_status = _finalize_pipewire_process(microphone_process)
            system_status = _finalize_pipewire_process(system_process)
            microphone_valid = microphone_status in ACCEPTED_PIPEWIRE_FINALIZATION_CODES and _is_compatible_pcm_wav(
                microphone_path
            )
            system_valid = system_status in ACCEPTED_PIPEWIRE_FINALIZATION_CODES and _is_compatible_pcm_wav(system_path)
            warnings: tuple[str, ...]
            if microphone_valid and system_valid:
                _mix_pcm16_wav(microphone_path, system_path, output_path)
                audio_sources = ("microphone", "system")
                warnings = ()
            elif microphone_valid:
                _copy_private_audio(microphone_path, output_path)
                audio_sources = ("microphone",)
                warnings = ("System audio was unavailable; this meeting contains microphone audio only.",)
            elif system_valid:
                _copy_private_audio(system_path, output_path)
                audio_sources = ("system",)
                warnings = ("Microphone audio was unavailable; this meeting contains system audio only.",)
            else:
                raise AudioCaptureError("Meeting capture produced no usable microphone or system audio.")
            duration_seconds = _wav_duration_seconds(output_path)
            succeeded = True
            return MeetingCaptureResult(
                path=output_path,
                audio_sources=audio_sources,
                warnings=warnings,
                duration_seconds=duration_seconds,
            )
        finally:
            self._clear_state()
            microphone_path.unlink(missing_ok=True)
            system_path.unlink(missing_ok=True)
            if not succeeded:
                output_path.unlink(missing_ok=True)

    def cancel(self) -> None:
        """Stop both Meeting streams and erase every partial or mixed recording."""
        microphone_process = self.microphone_process
        system_process = self.system_process
        output_path = self.output_path
        microphone_path = self.microphone_path
        system_path = self.system_path
        for process in (microphone_process, system_process):
            if process is not None:
                _terminate_process(process)
        self._clear_state()
        for path in (microphone_path, system_path, output_path):
            if path is not None:
                path.unlink(missing_ok=True)

    def _clear_state(self) -> None:
        """Release process and path state after finalization or cancellation."""
        self.microphone_process = None
        self.system_process = None
        self.output_path = None
        self.microphone_path = None
        self.system_path = None


def _pw_record_command(
    executable: str,
    output_path: Path,
    target: str | None = None,
    properties: str | None = None,
) -> list[str]:
    """Build one fixed-format PipeWire capture command without shell interpretation."""
    command = [
        executable,
        "--rate",
        str(SAMPLE_RATE),
        "--channels",
        str(CHANNEL_COUNT),
        "--format",
        "s16",
    ]
    if target is not None:
        command.extend(("--target", target))
    if properties is not None:
        command.extend(("--properties", properties))
    command.append(str(output_path))
    return command


def _pw_record_raw_command(executable: str, target: str | None = None) -> list[str]:
    """Build a fixed raw-PCM stdout command for simultaneous WAV storage and streaming."""
    command = [
        executable,
        "--rate",
        str(SAMPLE_RATE),
        "--channels",
        str(CHANNEL_COUNT),
        "--format",
        "s16",
    ]
    if target is not None:
        command.extend(("--target", target))
    command.extend(("--raw", "-"))
    return command


def _create_private_file(path: Path) -> None:
    """Create an owner-only file without following an existing destination."""
    descriptor = os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    os.close(descriptor)


def _finalize_pipewire_process(process: subprocess.Popen[bytes]) -> int:
    """Request graceful WAV finalization and return the recorder status."""
    if process.poll() is None:
        try:
            process.send_signal(signal.SIGINT)
        except ProcessLookupError:
            pass
    try:
        process.communicate(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()
        process.communicate()
    return process.returncode if process.returncode is not None else -1


def _terminate_process(process: subprocess.Popen[bytes]) -> None:
    """Terminate one capture process during cancellation without surfacing races."""
    if process.poll() is None:
        try:
            process.terminate()
        except ProcessLookupError:
            pass
    try:
        process.communicate(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()
        process.communicate()


def _terminate_process_without_communicate(process: subprocess.Popen[bytes]) -> None:
    """Terminate a stdout-drained process without starting a second pipe reader."""
    if process.poll() is None:
        try:
            process.terminate()
        except ProcessLookupError:
            pass
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait()


def _is_compatible_pcm_wav(path: Path) -> bool:
    """Return whether a capture is non-empty mono 16-bit PCM at the required rate."""
    try:
        with wave.open(str(path), "rb") as recording:
            return (
                recording.getnchannels() == CHANNEL_COUNT
                and recording.getsampwidth() == SAMPLE_WIDTH_BYTES
                and recording.getframerate() == SAMPLE_RATE
                and recording.getcomptype() == "NONE"
                and recording.getnframes() > 0
            )
    except (EOFError, OSError, wave.Error):
        return False


def _mix_pcm16_wav(microphone_path: Path, system_path: Path, output_path: Path) -> None:
    """Average aligned samples and preserve unpaired tails without loading a meeting into memory."""
    _create_private_file(output_path)
    try:
        with (
            wave.open(str(microphone_path), "rb") as microphone,
            wave.open(str(system_path), "rb") as system,
            wave.open(str(output_path), "wb") as output,
        ):
            output.setnchannels(CHANNEL_COUNT)
            output.setsampwidth(SAMPLE_WIDTH_BYTES)
            output.setframerate(SAMPLE_RATE)
            while True:
                microphone_samples = _pcm16_samples(microphone.readframes(4_096))
                system_samples = _pcm16_samples(system.readframes(4_096))
                if not microphone_samples and not system_samples:
                    break
                mixed_samples = array("h")
                for index in range(max(len(microphone_samples), len(system_samples))):
                    if index < len(microphone_samples) and index < len(system_samples):
                        sample_sum = microphone_samples[index] + system_samples[index]
                        mixed_samples.append(sample_sum // 2 if sample_sum >= 0 else -((-sample_sum) // 2))
                    elif index < len(microphone_samples):
                        mixed_samples.append(microphone_samples[index])
                    else:
                        mixed_samples.append(system_samples[index])
                if sys.byteorder != "little":
                    mixed_samples.byteswap()
                output.writeframes(mixed_samples.tobytes())
        output_path.chmod(0o600)
    except Exception:
        output_path.unlink(missing_ok=True)
        raise


def _pcm16_samples(frames: bytes) -> array[int]:
    """Decode little-endian signed PCM16 frames into native integers."""
    samples = array("h")
    samples.frombytes(frames)
    if sys.byteorder != "little":
        samples.byteswap()
    return samples


def pcm16_audio_level(frames: bytes) -> float:
    """Return normalized RMS for one little-endian mono PCM16 chunk."""
    samples = _pcm16_samples(frames[: len(frames) - (len(frames) % SAMPLE_WIDTH_BYTES)])
    if not samples:
        return 0.0
    mean_square = sum(sample * sample for sample in samples) / len(samples)
    return min(1.0, sqrt(mean_square) / 32_768)


def _copy_private_audio(source_path: Path, output_path: Path) -> None:
    """Copy one valid fallback source to a new owner-only final recording."""
    _create_private_file(output_path)
    try:
        shutil.copyfile(source_path, output_path)
        output_path.chmod(0o600)
    except Exception:
        output_path.unlink(missing_ok=True)
        raise


def _wav_duration_seconds(path: Path) -> float:
    """Read the finalized WAV duration from its frame metadata."""
    with wave.open(str(path), "rb") as recording:
        return recording.getnframes() / recording.getframerate()
