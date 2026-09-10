"""Package continuous installed-build captures, immutable text and exact harnesses for the launch edit."""

import argparse
import json
import shutil
import subprocess
import sys
from pathlib import Path

from compose_delight import fingerprint, probe


def main() -> None:
    """Keep a small explicit publication boundary around the private desktop's evidence."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("kind", choices=("real", "features"))
    parser.add_argument("capture", type=Path)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    capture, target = args.capture.resolve(strict=True), args.destination.resolve()
    capture.relative_to(root / "tmp")
    target.relative_to(root)
    receipt = json.loads((capture / "capture.json").read_text())
    if receipt["errors"] or receipt["display"] != ":203" or not (capture / "installation.json").is_file():
        raise RuntimeError("Only successful installed launch captures may be packaged")
    if args.kind == "real":
        subprocess.run([sys.executable, str(root / "dev/export_campaign.py"), str(capture), str(target)], check=True)
    else:
        target.mkdir(parents=True, exist_ok=False)
        media = probe(capture / "workflow.mkv")
        video = next(stream for stream in media["streams"] if stream["codec_type"] == "video")
        subprocess.run(
            [
                "ffmpeg",
                "-nostdin",
                "-v",
                "error",
                "-i",
                str(capture / "workflow.mkv"),
                "-map",
                "0:v:0",
                "-an",
                "-c:v",
                "libx264",
                "-preset",
                "medium",
                "-crf",
                "18",
                "-threads",
                "4",
                "-pix_fmt",
                "yuv420p",
                "-fps_mode",
                "cfr",
                "-r",
                video["r_frame_rate"],
                "-movflags",
                "+faststart",
                str(target / "workflow.mp4"),
            ],
            check=True,
        )
    for pattern in (
        "*.png",
        "capture.json",
        "installation.json",
        "runtime-files.json",
        "installed-files.json",
        "runtime-verification.json",
        "runtime.patch",
        "seed-take.json",
        "original.txt",
        "recognition.json",
        "replies.json",
    ):
        for source in capture.glob(pattern):
            shutil.copy2(source, target / source.name)
    shutil.copytree(capture / "harness", target / "harness")
    for name in ("package_delight.py", "export_campaign.py", "compose_delight.py"):
        shutil.copy2(root / "dev" / name, target / "harness" / name)
    subprocess.run(
        ["ffmpeg", "-nostdin", "-v", "error", "-i", str(target / "workflow.mp4"), "-f", "null", "-"], check=True
    )
    source_video = capture / ("screen.mkv" if args.kind == "real" else "workflow.mkv")
    packaged = {
        "schema_version": 1,
        "kind": args.kind,
        "source_capture": str(capture),
        "source_video": fingerprint(source_video),
        "source_video_media": probe(source_video),
        "output_video_media": probe(target / "workflow.mp4"),
        "timing": "Complete source duration; no editorial cuts or speed changes. Constant-frame-rate H.264 encoding.",
        "audio": "Measured original PCM alignment; see export.json" if args.kind == "real" else "Silent native fixture",
        "publication_boundary": "Listed media, text, receipts and harnesses only; no private session or provider state",
        "files": {
            str(path.relative_to(target)): fingerprint(path) for path in sorted(target.rglob("*")) if path.is_file()
        },
        "packager": fingerprint(Path(__file__)),
        "complete_decode_passed": True,
    }
    (target / "package.json").write_text(json.dumps(packaged, indent=2) + "\n")
    print(json.dumps({"packaged": str(target), "kind": args.kind, "decoded": True}), flush=True)


if __name__ == "__main__":
    main()
