# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy"]
# ///
"""Cut the matted presenter with the voice's source boundaries, 30 fps, as VP9 with alpha.

A dissolve over the first frames after a cut blends the frames the cut skipped with the frames it joins. Both come from pauses,
so no spoken lip motion is blended and no frame count changes.
    presenter.py EDL.json RGBA.mkv OUT.webm
"""
import json, subprocess, sys
import numpy as np

edl = json.load(open(sys.argv[1]))
W, H, CH = 900, 1350, 990  # stops above where his shoulders meet the phone frame's edge
reader = subprocess.Popen(["ffmpeg", "-v", "error", "-i", sys.argv[2], "-pix_fmt", "rgba", "-f", "rawvideo", "-"], stdout=subprocess.PIPE)
frames = []
need_to = edl["segments"][-1]["src_end"]
for _ in range(need_to + 2):
    raw = reader.stdout.read(W * H * 4)
    if len(raw) < W * H * 4:
        break
    frames.append(np.frombuffer(raw, np.uint8).reshape(H, W, 4)[:CH].copy())
reader.kill()
print("read", len(frames))
enc = subprocess.Popen(["ffmpeg", "-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", f"{W}x{CH}", "-r", "30", "-i", "-", "-c:v", "libvpx-vp9", "-pix_fmt", "yuva420p", "-crf", "18", "-b:v", "0", "-cpu-used", "2", "-row-mt", "1", "-threads", "12", "-auto-alt-ref", "0", sys.argv[3]], stdin=subprocess.PIPE)


def blend(a, b, u):
    # premultiplied, so the matte's edge stays clean through the dissolve
    a = a.astype(np.float32) / 255; b = b.astype(np.float32) / 255
    pa = a[..., :3] * a[..., 3:]; pb = b[..., :3] * b[..., 3:]
    alpha = a[..., 3:] * (1 - u) + b[..., 3:] * u
    rgb = (pa * (1 - u) + pb * u) / np.maximum(alpha, 1e-4)
    return np.uint8(np.clip(np.concatenate([rgb, alpha], 2), 0, 1) * 255 + 0.5)


segments = edl["segments"]
count = 0
for k, segment in enumerate(segments):
    for n in range(segment["src_start"], segment["src_end"]):
        i = n - segment["src_start"]
        frame = frames[n]
        if k > 0:
            cut = next(c for c in edl["cuts"] if c["inn"] == segment["src_start"])
            span = 6 if cut["look"] > 6 else 4
            if i < span:
                u = (i + 1) / (span + 1)
                u = u * u * (3 - 2 * u)
                frame = blend(frames[min(cut["out"] + i, len(frames) - 1)], frame, u)
        enc.stdin.write(frame.tobytes())
        count += 1
enc.stdin.close(); enc.wait()
print("frames", count, "seconds", count / 30)
