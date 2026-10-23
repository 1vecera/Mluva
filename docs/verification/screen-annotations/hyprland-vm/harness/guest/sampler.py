#!/usr/bin/env python3
"""sampler.py OUT_DIR SECONDS REF.ppm [X0 Y0 X1 Y1]: grim frames back to back for SECONDS, each stamped with
CLOCK_MONOTONIC (the probe's clock); prints one JSON line per frame with red and changed pixels
inside the box (default: the left half, where the probe is tiled), then keeps the frames for evidence."""

import json
import os
import subprocess
import sys
import time

out, seconds = sys.argv[1], float(sys.argv[2])
ref = sys.argv[3]
os.makedirs(out, exist_ok=True)
box = sys.argv[4:8] if len(sys.argv) > 7 else ["10", "10", "954", "1070"]
frames, end, i = [], time.monotonic() + seconds, 0
while time.monotonic() < end:
    path = f"{out}/f{i:03d}.ppm"
    t0 = time.monotonic()
    subprocess.run(["grim", "-t", "ppm", path], check=True)
    frames.append((t0, time.monotonic(), path))
    i += 1
here = os.path.dirname(os.path.abspath(__file__))
for t0, t1, path in frames:
    red = int(
        subprocess.run(
            ["python3", f"{here}/redcount.py", path, *box],
            capture_output=True,
            text=True,
            check=True,
        ).stdout
    )
    changed = (
        int(
            subprocess.run(
                ["python3", f"{here}/diffcount.py", path, ref, *box],
                capture_output=True,
                text=True,
                check=True,
            ).stdout
        )
        if ref
        else None
    )
    print(
        json.dumps(
            {
                "t0": round(t0, 3),
                "t1": round(t1, 3),
                "frame": os.path.basename(path),
                "red": red,
                "changed": changed,
            }
        ),
        flush=True,
    )
