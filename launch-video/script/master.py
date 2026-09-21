"""Attach the approved narration mix to a silent render without changing picture or timing."""

import argparse
import json
import subprocess
import wave
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("source", type=Path)
args = parser.parse_args()
source = args.source.resolve()
output = source.with_stem(source.stem + "-master")
mix = Path(__file__).resolve().parents[1] / "public/audio/narration-mix.wav"
probe = subprocess.run(
    [
        "ffprobe",
        "-v",
        "error",
        "-select_streams",
        "v:0",
        "-show_entries",
        "stream=duration",
        "-of",
        "json",
        str(source),
    ],
    capture_output=True,
    text=True,
    check=True,
)
picture_duration = float(json.loads(probe.stdout)["streams"][0]["duration"])
with wave.open(str(mix)) as audio:
    audio_duration = audio.getnframes() / audio.getframerate()
if abs(picture_duration - audio_duration) > 0.001:
    raise SystemExit(
        "Picture and approved audio durations differ; restore the matching media inputs."
    )
subprocess.run(
    [
        "ffmpeg",
        "-nostdin",
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-i",
        str(source),
        "-i",
        str(mix),
        "-map",
        "0:v:0",
        "-map",
        "1:a:0",
        "-c:v",
        "copy",
        "-c:a",
        "aac",
        "-b:a",
        "256k",
        "-map_metadata",
        "-1",
        "-movflags",
        "+faststart",
        str(output),
    ],
    check=True,
)
print(
    json.dumps({"output": str(output), "duration": picture_duration, "audio": str(mix)})
)
