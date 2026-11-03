# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy","scipy","soundfile"]
# ///
"""Voice chain for the film: DeepFilterNet3 (run first on the mono take), then lavalier-tuned EQ, de-ess and compression.
Picture and sound use identical source boundaries. No pitch or rate change. Writes voice.wav on film time."""

import json
import os
import subprocess
from pathlib import Path

import numpy as np
import soundfile as sf

ROOT = os.path.join(
    os.environ.get(
        "MLUVA_FILM_MEDIA", os.path.expanduser("~/Videos/Mluva/launch-2026-recording")
    ),
    "work",
)
edl = json.loads(Path(f"{ROOT}/edit/edl.json").read_text())
FPS = edl["fps"]
SR = 48000
y, sr = sf.read(f"{ROOT}/audio/df/mono.wav", dtype="float64")
assert sr == SR
fade = int(0.006 * SR)
parts = []
for s in edl["segments"]:
    a, b = round(s["src_start"] * SR / FPS), round(s["src_end"] * SR / FPS)
    c = y[a:b].copy()
    w = np.linspace(0, 1, fade)
    c[:fade] *= w
    c[-fade:] *= w[::-1]
    parts.append(c)
out = np.concatenate(parts)
total = round(edl["duration"] * SR)
assert len(out) == total, (len(out), total)
sf.write(f"{ROOT}/audio/final/voice-cut.wav", out, SR, subtype="FLOAT")
print("samples", len(out), "seconds", len(out) / SR)

# Lavalier on a shirt: the measured long-term spectrum has a hole around 4 kHz and too much above 7 kHz against v7's approved voice.
chain = "highpass=f=60:p=2,lowshelf=f=115:t=q:w=0.7:g=3.0,equalizer=f=250:t=q:w=1.0:g=-1.6,equalizer=f=2600:t=q:w=1.2:g=2.0,equalizer=f=3900:t=q:w=1.3:g=7.5,highshelf=f=6500:t=q:w=0.7:g=-6.0,equalizer=f=8500:t=q:w=1.2:g=-2.0,deesser=i=0.2:m=0.45:f=0.55"
import re

tmp = f"{ROOT}/audio/final/voice-eq.wav"
subprocess.run(
    [
        "ffmpeg",
        "-nostdin",
        "-v",
        "error",
        "-y",
        "-i",
        f"{ROOT}/audio/final/voice-cut.wav",
        "-af",
        chain,
        "-ar",
        "48000",
        "-c:a",
        "pcm_f32le",
        tmp,
    ],
    check=True,
)


def measure(path, opts):
    r = subprocess.run(
        [
            "ffmpeg",
            "-nostdin",
            "-hide_banner",
            "-i",
            path,
            "-af",
            f"loudnorm={opts}:print_format=json",
            "-f",
            "null",
            "-",
        ],
        capture_output=True,
        check=False,
    )
    return json.loads(re.search(rb'\{\s*"input_i".*?\}', r.stderr, re.DOTALL)[0])


m = measure(tmp, "I=-23:TP=-3:LRA=8")
gain = -23 - float(m["input_i"])
comp = f"volume={gain:.2f}dB,acompressor=threshold=0.09:ratio=2.6:attack=12:release=150:knee=2.8:makeup=1.4:mix=0.85"
toned = f"{ROOT}/audio/final/voice-toned.wav"
subprocess.run(
    [
        "ffmpeg",
        "-nostdin",
        "-v",
        "error",
        "-y",
        "-i",
        tmp,
        "-af",
        comp,
        "-ar",
        "48000",
        "-c:a",
        "pcm_f32le",
        toned,
    ],
    check=True,
)
opts = "I=-18:TP=-3:LRA=6"
m = measure(toned, opts)
corr = ":".join(
    f"{a}={m[b]}"
    for a, b in [
        ("measured_I", "input_i"),
        ("measured_TP", "input_tp"),
        ("measured_LRA", "input_lra"),
        ("measured_thresh", "input_thresh"),
        ("offset", "target_offset"),
    ]
)
voice = f"{ROOT}/audio/final/voice.wav"
subprocess.run(
    [
        "ffmpeg",
        "-nostdin",
        "-v",
        "error",
        "-y",
        "-i",
        toned,
        "-af",
        f"loudnorm={opts}:{corr}:linear=true,aresample=48000,apad=whole_len={total},atrim=end_sample={total}",
        "-ar",
        "48000",
        "-c:a",
        "pcm_s24le",
        voice,
    ],
    check=True,
)
print(measure(voice, opts))
Path(f"{ROOT}/audio/final/chain.json").write_text(
    json.dumps(
        {
            "denoiser": "DeepFilterNet3 v0.5.6, local, attenuation limit 18 dB",
            "filter": chain,
            "compression": comp,
            "loudnorm": opts,
            "pitch_shift": 0,
            "speech_rate": 1,
        },
        indent=1,
    )
)
