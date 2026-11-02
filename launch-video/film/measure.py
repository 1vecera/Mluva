# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy"]
# ///
"""List the frames where a take visibly changes, as (capture seconds, change size), to anchor it on Daniel's words.

    measure.py TAKE.mkv [THRESHOLD] [x y w h]    region in 2560x1440 pixels, optional
"""
import subprocess, sys
import numpy as np

path = sys.argv[1]
threshold = float(sys.argv[2]) if len(sys.argv) > 2 else 0.25
crop = sys.argv[3:7]
vf = (f"crop={crop[2]}:{crop[3]}:{crop[0]}:{crop[1]}," if crop else "") + "scale=320:180:flags=area,format=gray"
raw = subprocess.run(["ffmpeg", "-v", "error", "-i", path, "-vf", vf, "-fps_mode", "passthrough", "-f", "rawvideo", "-"], capture_output=True, check=True).stdout
pts = subprocess.run(["ffprobe", "-v", "error", "-select_streams", "v", "-show_entries", "packet=pts_time", "-of", "csv=p=0", path], capture_output=True, text=True).stdout.split()
pts = [float(x) for x in pts]
frames = np.frombuffer(raw, np.uint8).reshape(-1, 180, 320).astype(np.float32)
d = np.abs(np.diff(frames, axis=0)).mean(axis=(1, 2))
active = False
for i, v in enumerate(d):
    if v > threshold and not active:
        j = i
        while j + 1 < len(d) and d[j + 1] > threshold / 3:
            j += 1
        print(f"frame {i + 1:5d} t={pts[i + 1]:7.3f} peak={d[i:j + 1].max():6.2f} until t={pts[min(j + 1, len(pts) - 1)]:7.3f}")
        active = True
    elif v <= threshold / 3:
        active = False
print("frames", len(frames), "last", pts[-1])
