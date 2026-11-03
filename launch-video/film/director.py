#!/usr/bin/env python3
"""Film one take in the VM: record its whole screen losslessly while key, pointer and guest events fire at Daniel's word times.

    director.py TAKE.json OUT_DIR [--no-record]

The take names events in film seconds, so a key press lands on the word that names it. Keys and pointer reach the guest's USB
keyboard and tablet through QMP and nothing touches the host. Writes `timeline.json` with when each event fired relative to
the recorder's start, and pulls the capture to OUT_DIR.
"""

import importlib.util
import json
import pathlib
import subprocess
import sys
import time
from pathlib import Path

root = pathlib.Path(__file__).resolve().parents[2]
vm = root / "tmp/hyprland-vm/run"
spec = importlib.util.spec_from_file_location(
    "qmp", root / "docs/verification/screen-annotations/hyprland-vm/harness/qmp.py"
)
qmpmod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(qmpmod)

SSH = ["ssh", "-F", str(vm / "ssh_config"), "hyprvm"]
SCALE_SIZE = (1280, 720)  # the tablet's range maps onto the guest's logical layout


def guest(command, check=True):
    return subprocess.run(
        SSH + [". ~/henv.sh; " + command], check=check, capture_output=True, text=True
    ).stdout


def guest_bg(command):
    return subprocess.Popen(
        SSH + [". ~/henv.sh; " + command],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


def smooth_path(points, duration):
    """Hand-paced motion through way points: Catmull-Rom, eased at both ends, sampled at 120 Hz."""
    n = max(2, int(duration * 120))
    out = []
    pts = [points[0]] + list(points) + [points[-1]]
    segs = len(points) - 1
    for i in range(n + 1):
        s = i / n
        s = s * s * (3 - 2 * s) * 0.5 + s * 0.5
        u = min(s * segs, segs - 1e-6)
        k = int(u)
        f = u - k
        p0, p1, p2, p3 = pts[k], pts[k + 1], pts[k + 2], pts[k + 3]
        q = [
            0.5
            * (
                2 * p1[a]
                + (-p0[a] + p2[a]) * f
                + (2 * p0[a] - 5 * p1[a] + 4 * p2[a] - p3[a]) * f * f
                + (-p0[a] + 3 * p1[a] - 3 * p2[a] + p3[a]) * f**3
            )
            for a in (0, 1)
        ]
        out.append((i * duration / n, q[0], q[1]))
    return out


def main():
    take = json.loads(Path(sys.argv[1]).read_text())
    out = pathlib.Path(sys.argv[2])
    out.mkdir(parents=True, exist_ok=True)
    record = "--no-record" not in sys.argv
    q = qmpmod.Qmp(str(vm / "qmp.sock"), SCALE_SIZE)
    if "script" in take:
        guest(
            f"cat > ~/film/script.json <<'J'\n{json.dumps(take['script'])}\nJ\nsudo rm -f ~/film/run/speech.jsonl; ~/start.sh peer; sleep 0.8"
        )
    for command in take.get("before", []):
        print("before:", command[:80], guest(command, check=False).strip()[:200])
    for step in take.get("qmp_before", []):
        apply(q, step)
    lead = take.get("lead", 1.0)
    start = take["start"]
    end = take["end"]
    events = sorted(take["events"], key=lambda e: e["t"])
    capture = None
    if record:
        guest("rm -f /dev/shm/take.mkv")
        capture = guest_bg(
            "exec wf-recorder -y -f /dev/shm/take.mkv -r 60 --no-dmabuf -c libx264rgb -p preset=ultrafast -p qp=0 -x bgr0 > ~/film/run/wf.log 2>&1"
        )
        time.sleep(
            1.5
        )  # the recorder's first frame comes well before this; the frames, not this clock, set the edit
    for step in take.get("prelude", []):
        apply(q, step)
        time.sleep(step.get("hold", 0.9))
    t0 = time.monotonic() + lead - start
    log = []
    for event in events:
        at = t0 + event["t"]
        delay = at - time.monotonic()
        if delay > 0:
            time.sleep(delay)
        fired = time.monotonic()
        apply(q, event, t0)
        log.append(
            {
                "film": event["t"],
                "fired": round(fired - t0, 4),
                **{k: v for k, v in event.items() if k not in ("t", "points")},
            }
        )
    delay = t0 + end - time.monotonic()
    if delay > 0:
        time.sleep(delay)
    if capture:
        # The encoder may lag behind a busy shot; frames queued when it stops would be lost, so let it drain first.
        time.sleep(take.get("drain", 8))
        guest("pkill -INT -x wf-recorder; sleep 1.2; true", check=False)
        capture.wait(timeout=20)
        subprocess.run(
            [
                "scp",
                "-q",
                "-F",
                str(vm / "ssh_config"),
                "hyprvm:/dev/shm/take.mkv",
                str(out / "take.mkv"),
            ],
            check=True,
        )
    speech = guest("cat ~/film/run/speech.jsonl 2>/dev/null", check=False)
    (out / "speech.jsonl").write_text(speech)
    Path(out / "timeline.json").write_text(
        json.dumps(
            {"take": take.get("name"), "start": start, "end": end, "events": log},
            indent=1,
        )
    )
    for command in take.get("after", []):
        print("after:", command[:80], guest(command, check=False).strip()[:300])
    print("done", out)


def apply(q, event, t0=None):
    kind = event["type"]
    if kind == "key":
        q.chord(event["keys"], gap=0.01)
    elif kind == "down":
        q.key(event["key"], True)
    elif kind == "up":
        q.key(event["key"], False)
    elif kind == "type":
        q.type(event["text"])
    elif kind == "move":
        q.move(event["x"], event["y"])
    elif kind == "click":
        q.button("left", True)
        time.sleep(0.05)
        q.button("left", False)
    elif kind == "path":
        begin = time.monotonic()
        for at, x, y in smooth_path(event["points"], event["duration"]):
            delay = begin + at - time.monotonic()
            if delay > 0:
                time.sleep(delay)
            q.move(x, y)
    elif kind == "guest":
        guest_bg(event["command"])
    elif kind == "sleep":
        time.sleep(event["seconds"])
    else:
        raise SystemExit(f"unknown event {kind}")


main()
