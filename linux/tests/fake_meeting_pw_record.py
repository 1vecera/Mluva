#!/usr/bin/env python3
"""Emulate two fixed-format PipeWire streams without opening desktop audio."""

import os
import signal
import sys
import time
import wave
from array import array
from pathlib import Path

SYSTEM_CAPTURE_PROPERTY = "stream.capture.sink"


def finalize(_signal_number: int, _frame: object) -> None:
    """Match Fedora pw-record's successful status-one SIGINT finalization."""
    raise SystemExit(1)


def write_fixture(path: Path, sample: int, frame_count: int) -> None:
    """Write deterministic mono PCM16 frames for local mixer assertions."""
    samples = array("h", [sample] * frame_count)
    with wave.open(str(path), "wb") as recording:
        recording.setnchannels(1)
        recording.setsampwidth(2)
        recording.setframerate(16_000)
        recording.writeframes(samples.tobytes())


def main() -> None:
    """Write one source fixture and wait until the recorder explicitly stops it."""
    signal.signal(signal.SIGINT, finalize)
    output_path = Path(sys.argv[-1])
    captures_system_audio = any(SYSTEM_CAPTURE_PROPERTY in argument for argument in sys.argv[1:-1])
    if captures_system_audio and os.environ.get("VOICE_SCRIBE_FAKE_SYSTEM_FAILURE") == "1":
        raise SystemExit(2)
    if not captures_system_audio and os.environ.get("VOICE_SCRIBE_FAKE_MICROPHONE_FAILURE") == "1":
        raise SystemExit(2)
    write_fixture(
        output_path,
        sample=3_000 if captures_system_audio else 1_000,
        frame_count=6 if captures_system_audio else 8,
    )
    while True:
        time.sleep(0.1)


if __name__ == "__main__":
    main()
