import json
import os
import pathlib
import subprocess
import sys
import urllib.request
import uuid

src, dst = sys.argv[1], sys.argv[2]
k = os.environ.get("ELEVEN_LABS_STT_TOKEN") or os.environ.get("ELEVENLABS_API_KEY")
raw = subprocess.run(
    [
        "ffmpeg",
        "-v",
        "error",
        "-i",
        src,
        "-vn",
        "-ac",
        "1",
        "-ar",
        "16000",
        "-c:a",
        "libopus",
        "-b:a",
        "48k",
        "-f",
        "ogg",
        "-",
    ],
    check=True,
    capture_output=True,
).stdout
b = uuid.uuid4().hex
parts = []
for n, v in {
    "model_id": "scribe_v2",
    "timestamps_granularity": "word",
    "language_code": "en",
    "tag_audio_events": "true",
    "diarize": "false",
}.items():
    parts.append(
        f'--{b}\r\nContent-Disposition: form-data; name="{n}"\r\n\r\n{v}\r\n'.encode()
    )
parts.append(
    f'--{b}\r\nContent-Disposition: form-data; name="file"; filename="a.ogg"\r\nContent-Type: audio/ogg\r\n\r\n'.encode()
    + raw
    + b"\r\n"
)
parts.append(f"--{b}--\r\n".encode())
r = urllib.request.Request(
    "https://api.elevenlabs.io/v1/speech-to-text",
    data=b"".join(parts),
    headers={"xi-api-key": k, "Content-Type": f"multipart/form-data; boundary={b}"},
)
d = json.load(urllib.request.urlopen(r, timeout=300))
pathlib.Path(dst).write_text(json.dumps(d, indent=1, ensure_ascii=False))
print(d.get("text"))
