# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy","scipy","soundfile"]
# ///
"""Check a finished film: full decode, black frames, loudness, the mix's alignment with the voice, and a fresh Scribe pass.

verify.py FILM.mp4 VOICE.wav PLAN.json SCRIBE.py OUT.json
"""

import json
import re
import subprocess
import sys
from pathlib import Path

import numpy as np
import soundfile as sf
from scipy.signal import correlate, correlation_lags

film, voice_path, plan_path, scribe, out = sys.argv[1:6]
record = {}
probe = json.loads(
    subprocess.run(
        [
            "ffprobe",
            "-v",
            "error",
            "-show_entries",
            "stream=codec_name,width,height,r_frame_rate,duration,nb_frames,sample_rate",
            "-of",
            "json",
            film,
        ],
        capture_output=True,
        text=True,
        check=False,
    ).stdout
)
record["streams"] = probe["streams"]
decode = subprocess.run(
    [
        "ffmpeg",
        "-nostdin",
        "-v",
        "error",
        "-i",
        film,
        "-map",
        "0:v",
        "-vf",
        "blackdetect=d=0.1:pix_th=0.05",
        "-f",
        "null",
        "-",
    ],
    capture_output=True,
    text=True,
    check=False,
)
record["decode_errors"] = [
    l for l in decode.stderr.splitlines() if "black_start" not in l
][:5]
record["black_intervals"] = re.findall(
    r"black_start:(\S+) black_end:(\S+)", decode.stderr
)
frames = subprocess.run(
    [
        "ffprobe",
        "-v",
        "error",
        "-count_frames",
        "-select_streams",
        "v",
        "-show_entries",
        "stream=nb_read_frames",
        "-of",
        "csv=p=0",
        film,
    ],
    capture_output=True,
    text=True,
    check=False,
).stdout.strip()
record["decoded_frames"] = int(frames)
loud = subprocess.run(
    [
        "ffmpeg",
        "-nostdin",
        "-hide_banner",
        "-i",
        film,
        "-af",
        "loudnorm=I=-16:TP=-1.8:LRA=8:print_format=json",
        "-f",
        "null",
        "-",
    ],
    capture_output=True,
    check=False,
)
m = json.loads(re.search(rb'\{\s*"input_i".*?\}', loud.stderr, re.DOTALL)[0])
record["loudness"] = {
    "lufs": m["input_i"],
    "true_peak_dbtp": m["input_tp"],
    "lra": m["input_lra"],
}
voice, sr = sf.read(voice_path)
mix = np.frombuffer(
    subprocess.run(
        [
            "ffmpeg",
            "-nostdin",
            "-v",
            "error",
            "-i",
            film,
            "-vn",
            "-ac",
            "1",
            "-ar",
            str(sr),
            "-f",
            "f32le",
            "-",
        ],
        capture_output=True,
        check=False,
    ).stdout,
    np.float32,
)
sync = []
for start in (3, 14, 25, 36, 47, 58):
    a = voice[start * sr : (start + 6) * sr]
    b = mix[start * sr : (start + 6) * sr]
    c = correlate(b, a, mode="full", method="fft")
    lags = correlation_lags(len(b), len(a), mode="full")
    keep = np.abs(lags) <= 4800
    lag = int(lags[keep][np.argmax(c[keep])])
    sync.append(
        {"start": start, "lag_samples_48khz": lag, "offset_ms": round(lag / 48, 3)}
    )
record["voice_sync"] = sync
plan = json.loads(Path(plan_path).read_text())
expected = [w["text"] for p in plan["poppy_captions"] for w in p["words"]]
subprocess.run(
    ["python3", scribe, film, out + ".scribe.json"], check=True, capture_output=True
)
heard = [
    w["text"]
    for w in json.loads(Path(out + ".scribe.json").read_text())["words"]
    if w["type"] == "word"
]
norm = lambda s: re.sub(r"[^a-z0-9]", "", s.lower())
record["scribe_words"] = len(heard)
record["scribe_text"] = " ".join(heard)
record["unexpected_fillers"] = [w for w in heard if norm(w) in ("uh", "um", "if")]
Path(out).write_text(json.dumps(record, indent=1))
print(
    json.dumps(
        {k: v for k, v in record.items() if k not in ("scribe_text", "streams")},
        indent=1,
    )
)
