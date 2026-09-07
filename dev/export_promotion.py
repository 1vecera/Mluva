"""Export real desktop recordings to promotion assets with auditable media metadata."""

import argparse
import hashlib
import json
import shutil
import subprocess
from pathlib import Path


def inspect_media(path: Path) -> dict:
    """Verify dimensions, codec and duration through the actual encoded stream."""
    result = subprocess.check_output(
        [
            "ffprobe",
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,codec_name,pix_fmt,r_frame_rate:format=duration,size",
            "-of",
            "json",
            str(path),
        ],
        text=True,
    )
    return json.loads(result)


def main() -> None:
    """Keep original screenshots intact; encode portable, silent H.264 videos for sharing."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("captures", type=Path, help="Parent containing dark/, portrait/ and light/ captures")
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    args.destination.mkdir(parents=True, exist_ok=False)
    captures = {name: args.captures / name for name in ("dark", "portrait", "light")}
    receipts = {name: json.loads((path / "capture.json").read_text()) for name, path in captures.items()}
    for receipt in receipts.values():
        assert receipt["real_rewrite_action"] and receipt["original_preserved"] and receipt["copy_action"]
    screenshots = {
        "hero-dark.png": ("dark", "hero.png"),
        "hero-light.png": ("light", "hero.png"),
        "workflow-dark.png": ("dark", "conversation.png"),
        "vertical-poster.png": ("portrait", "conversation.png"),
        "workspace-dark.png": ("dark", "workspace.png"),
        "workspace-light.png": ("light", "workspace.png"),
        "widget-recording.png": ("dark", "widget-recording.png"),
        "widget-review.png": ("dark", "widget-review.png"),
    }
    for name, (scenario, source) in screenshots.items():
        shutil.copyfile(captures[scenario] / source, args.destination / name)
    encoding = [
        "-c:v",
        "libx264",
        "-threads",
        "2",
        "-preset",
        "medium",
        "-crf",
        "20",
        "-pix_fmt",
        "yuv420p",
        "-movflags",
        "+faststart",
        "-an",
    ]
    for name, scenario in (("mluva-omarchy-demo.mp4", "dark"), ("mluva-omarchy-vertical.mp4", "portrait")):
        subprocess.run(
            [
                "ffmpeg",
                "-hide_banner",
                "-loglevel",
                "error",
                "-i",
                str(captures[scenario] / "capture.webm"),
                *encoding,
                str(args.destination / name),
            ],
            check=True,
        )
    subprocess.run(
        [
            "ffmpeg",
            "-hide_banner",
            "-loglevel",
            "error",
            "-ss",
            "4",
            "-t",
            "23",
            "-i",
            str(captures["dark"] / "capture.webm"),
            "-i",
            str(args.captures / "footer.png"),
            "-filter_complex",
            "[0:v]crop=500:186:710:870,scale=1000:372:flags=lanczos[widget];[widget][1:v]vstack=inputs=2[out]",
            "-map",
            "[out]",
            *encoding,
            str(args.destination / "mluva-widget-closeup.mp4"),
        ],
        check=True,
    )
    inventory = {}
    for path in sorted(args.destination.iterdir()):
        media = inspect_media(path)
        stream = media["streams"][0]
        assert stream["width"] > 100 and stream["height"] > 100
        if path.suffix == ".mp4":
            assert stream["codec_name"] == "h264" and stream["pix_fmt"] == "yuv420p"
            assert float(media["format"]["duration"]) >= 20
        inventory[path.name] = {"sha256": hashlib.sha256(path.read_bytes()).hexdigest(), **media}
    manifest = {
        "release": "v0.1.1",
        "captured_application_commit": receipts["dark"]["source_commit"],
        "disclosure": receipts["dark"]["disclosure"],
        "capture_style": "Private X11 desktop; configured Inter font; portrait widget raised above the footer.",
        "cloud_processing": "Real recognition uses ElevenLabs; optional rewrites and generated titles use Codex.",
        "captures": receipts,
        "files": inventory,
    }
    (args.destination / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"Exported {len(inventory)} verified media files to {args.destination}")


if __name__ == "__main__":
    main()
