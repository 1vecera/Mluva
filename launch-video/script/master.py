"""Master a local render with measured two-pass loudness, retaining its video stream."""

import argparse
import json
import re
import subprocess
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("source", type=Path)
parser.add_argument(
    "--audio-advance-samples",
    type=int,
    default=0,
    help="Remove a measured render delay, in samples at 48 kHz (default: 0).",
)
args = parser.parse_args()
if args.audio_advance_samples < 0:
    parser.error("Audio advance must be nonnegative.")
source = args.source.resolve()
output = source.with_stem(source.stem + "-master")
probe = subprocess.run(
    [
        "ffprobe", "-v", "error", "-select_streams", "v:0",
        "-show_entries", "stream=duration", "-of", "json", str(source),
    ],
    capture_output=True,
    text=True,
    check=True,
)
duration = float(json.loads(probe.stdout)["streams"][0]["duration"])
if args.audio_advance_samples >= round(duration * 48000):
    parser.error("Audio advance must be shorter than the picture.")
align = (
    f"aresample=48000,atrim=start_sample={args.audio_advance_samples},"
    f"asetpts=PTS-STARTPTS,apad=whole_dur={duration},atrim=duration={duration},"
)
target = "I=-16:TP=-1.8:LRA=11"
first = subprocess.run(
    [
        "ffmpeg",
        "-hide_banner",
        "-i",
        str(source),
        "-af",
        f"{align}loudnorm={target}:print_format=json",
        "-f",
        "null",
        "-",
    ],
    capture_output=True,
    text=True,
    check=True,
)
measure = json.loads(re.search(r'\{\s*"input_i".*?\}', first.stderr, re.DOTALL)[0])
options = ":".join(
    f"{name}={measure[key]}"
    for name, key in [
        ("measured_I", "input_i"),
        ("measured_TP", "input_tp"),
        ("measured_LRA", "input_lra"),
        ("measured_thresh", "input_thresh"),
        ("offset", "target_offset"),
    ]
)
subprocess.run(
    [
        "ffmpeg",
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-i",
        str(source),
        "-af",
        f"{align}loudnorm={target}:{options}:linear=true",
        "-ar",
        "48000",
        "-c:v",
        "copy",
        "-c:a",
        "aac",
        "-b:a",
        "256k",
        "-movflags",
        "+faststart",
        str(output),
    ],
    check=True,
)
final = subprocess.run(
    [
        "ffmpeg",
        "-hide_banner",
        "-i",
        str(output),
        "-af",
        f"loudnorm={target}:print_format=json",
        "-f",
        "null",
        "-",
    ],
    capture_output=True,
    text=True,
    check=True,
)
verified = json.loads(re.search(r'\{\s*"input_i".*?\}', final.stderr, re.DOTALL)[0])
output.with_suffix(".audio.json").write_text(
    json.dumps(
        {
            "source": str(source),
            "target_lufs": -16,
            "target_true_peak": -1.8,
            "audio_advance_samples_48khz": args.audio_advance_samples,
            "picture_duration": duration,
            "first_pass": measure,
            "verified_encoded_output": verified,
        },
        indent=2,
    )
    + "\n"
)
print(
    json.dumps(
        {
            "output": str(output),
            "lufs": verified["input_i"],
            "true_peak": verified["input_tp"],
        }
    )
)
if float(verified["input_tp"]) > -1.5 or abs(float(verified["input_i"]) + 16) > 0.5:
    raise SystemExit(
        "Master loudness is outside the target tolerance. Review before delivery."
    )
