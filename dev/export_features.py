"""Package uncut UI fixtures and their exact capture harness for a portable film edit."""

import argparse
import json
import shutil
import subprocess
from pathlib import Path

from compose_campaign_intro import fingerprint, inspect


def main() -> None:
    """Remux without retiming and preserve intentional, reviewable evidence."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("captures", type=Path)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    args.destination.mkdir(parents=True, exist_ok=False)
    for scenario in ("polish", "floating", "expanded", "history", "export", "providers"):
        source = args.captures / scenario
        receipt = json.loads((source / "capture.json").read_text())
        if receipt["errors"] or receipt["provider_calls"] != 0 or receipt["display"] != ":195":
            raise ValueError(f"Incomplete or unexpected fixture: {scenario}")
        target = args.destination / scenario
        target.mkdir()
        for pattern in ("*.png", "capture.json", "runtime-files.json", "recognition.json", "replies.json", "export.*"):
            for path in source.glob(pattern):
                shutil.copy2(path, target / path.name)
        shutil.copytree(source / "harness", target / "harness")
        subprocess.run(
            [
                "ffmpeg",
                "-nostdin",
                "-v",
                "error",
                "-i",
                str(source / "workflow.mkv"),
                "-c:v",
                "copy",
                "-an",
                "-movflags",
                "+faststart",
                str(target / "workflow.mp4"),
            ],
            check=True,
        )
        subprocess.run(
            ["ffmpeg", "-nostdin", "-v", "error", "-i", str(target / "workflow.mp4"), "-f", "null", "-"], check=True
        )
        (target / "export-receipt.json").write_text(
            json.dumps(
                {
                    "source_file": "workflow.mkv in the retained capture directory",
                    "source": fingerprint(source / "workflow.mkv"),
                    "output_file": "workflow.mp4",
                    "output": fingerprint(target / "workflow.mp4"),
                    "duration_seconds": float(inspect(target / "workflow.mp4")["format"]["duration"]),
                    "transform": "Complete H.264 stream copied into MP4; no cuts, retiming or audio",
                    "capture_event_clocks": {
                        "before.png": "Setup-process monotonic clock, before the recorder is launched",
                        "action_and_after.png": "Monotonic clock reset after FFmpeg launch; not an exact frame clock",
                        "limit": "Reading holds and local fixture delays are not provider-latency measurements",
                    },
                    "capture_harness": {
                        path.name: fingerprint(path) for path in sorted((target / "harness").iterdir())
                    },
                    "exporter": fingerprint(Path(__file__)),
                },
                indent=2,
            )
            + "\n"
        )
        print(f"Packaged {scenario}", flush=True)


if __name__ == "__main__":
    main()
