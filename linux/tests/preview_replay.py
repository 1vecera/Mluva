"""Capture a disclosed, offline replay of the campaign's shrinking recognition preview."""

import json
import os
import subprocess
import time
from pathlib import Path

from gi.repository import GLib

from mluva_linux.overlay_state import RecordingOverlayState


def replay_preview(publisher, observe, output: Path) -> None:
    """Retain 60 fps pixels and geometry around the exact recorded display-only text updates."""
    fixture = json.loads(Path(__file__).with_name("fixtures").joinpath("preview-contraction.json").read_text())
    events = fixture["events"]

    def publish(text):
        """Keep recording phase and timer stable while the recorded preview changes."""
        publisher.publish(RecordingOverlayState(phase="recording", elapsed_seconds=26, preview=text))

    def pump():
        """Dispatch the private publisher while the independent QML process paints."""
        while GLib.MainContext.default().pending():
            GLib.MainContext.default().iteration(False)
        time.sleep(0.002)

    publish(events[0]["text"])
    before = observe("recording", events[0]["text"])
    with (output / "capture.log").open("w") as log:
        capture = subprocess.Popen(
            [
                "ffmpeg",
                "-nostdin",
                "-y",
                "-v",
                "error",
                "-f",
                "x11grab",
                "-framerate",
                "60",
                "-video_size",
                f"{before['screenWidth']}x{before['screenHeight']}",
                "-i",
                os.environ["DISPLAY"],
                "-t",
                "5",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-crf",
                "0",
                str(output / "replay.mp4"),
            ],
            stderr=log,
        )
        started = time.monotonic()
        try:
            for event in events[1:]:
                while time.monotonic() - started < 0.5 + event["seconds"]:
                    pump()
                publish(event["text"])
            while capture.poll() is None and time.monotonic() - started < 10:
                pump()
            assert capture.poll() == 0, "Private 60 fps capture failed; inspect capture.log"
        finally:
            if capture.poll() is None:
                capture.terminate()
                capture.wait(timeout=5)
    after = observe("recording", events[-1]["text"])
    (output / "replay.json").write_text(
        json.dumps(
            {
                "fixture": fixture,
                "display": os.environ["DISPLAY"],
                "fps": 60,
                "boundary": "Offline provider-event replay, production QML, private X11/D-Bus; no microphone/providers",
                "before": before,
                "after": after,
            },
            indent=2,
        )
    )
