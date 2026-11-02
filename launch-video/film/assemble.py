# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy"]
# ///
"""Assemble the desktop picture on film time from the real takes: no pixel is drawn, only frames chosen, held and crossfaded.

    assemble.py ANCHORS.json OUT.mp4 ab=PATH c=PATH e=PATH f=PATH empty=PATH.png
"""
import json, subprocess, sys
import numpy as np

W, H, FPS, XF = 2560, 1440, 60, 0.35
anchors = json.load(open(sys.argv[1]))["shots"]
out = sys.argv[2]
paths = dict(a.split("=", 1) for a in sys.argv[3:])


def cap_at(shot, film):
    a = shot["anchors"]
    i = max(j for j in range(len(a)) if a[j][0] <= film + 1e-9) if film >= a[0][0] else 0
    f0, c0 = a[i]
    t = c0 + max(0.0, film - f0)
    return min(t, a[i + 1][1]) if i + 1 < len(a) else t  # rest frames held until the next anchor, or trimmed at it


class Reader:
    def __init__(self, path):
        self.proc = subprocess.Popen(["ffmpeg", "-v", "error", "-i", path, "-fps_mode", "passthrough", "-pix_fmt", "rgb24", "-f", "rawvideo", "-"], stdout=subprocess.PIPE, bufsize=W * H * 3 * 4)
        self.index = -1
        self.frame = None

    def get(self, target):
        while self.index < target:
            raw = self.proc.stdout.read(W * H * 3)
            if len(raw) < W * H * 3:
                return self.frame  # the take ended: hold its last frame
            self.frame = raw
            self.index += 1
        return self.frame


readers = {}
def frame_for(shot, film):
    name = shot["take"]
    reader = readers.setdefault(name, Reader(paths[name]))
    return reader.get(round(cap_at(shot, film) * FPS))


total = round(max(s["until"] for s in anchors) * FPS)
enc = subprocess.Popen(["ffmpeg", "-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", f"{W}x{H}", "-r", str(FPS), "-i", "-", "-c:v", "libx264", "-preset", "medium", "-crf", "11", "-pix_fmt", "yuv444p", "-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709", "-g", "30", out], stdin=subprocess.PIPE)
records = []
for n in range(total):
    t = n / FPS
    active = [s for s in anchors if s["start"] <= t < s["until"]]
    if len(active) == 2:
        a, b = active
        u = (t - b["start"]) / (a["until"] - b["start"])
        u = u * u * (3 - 2 * u)
        fa = np.frombuffer(frame_for(a, t), np.uint8).astype(np.float32)
        fb = np.frombuffer(frame_for(b, t), np.uint8).astype(np.float32)
        enc.stdin.write(np.uint8(fa + (fb - fa) * u + 0.5).tobytes())
        records.append([n, a["take"], b["take"]])
    else:
        s = active[0]
        enc.stdin.write(frame_for(s, t))
        if n % 60 == 0:
            records.append([n, s["take"], readers[s["take"]].index])
enc.stdin.close(); enc.wait()
print("frames", total)
