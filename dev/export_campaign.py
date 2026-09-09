"""Mux captured PCM at its measured screen timestamp, preserving real workflow duration."""

import argparse
import hashlib
import json
import re
import shutil
import subprocess
from pathlib import Path


def inspect(path: Path) -> dict:
    """Read streams and timing from the encoded file."""
    return json.loads(
        subprocess.check_output(
            [
                "ffprobe",
                "-v",
                "error",
                "-show_entries",
                "stream=codec_name,codec_type,width,height,pix_fmt,r_frame_rate,sample_rate,channels,start_time,duration:format=duration,size",
                "-of",
                "json",
                str(path),
            ],
            text=True,
        )
    )


def export_vertical(destination: Path, background: Path) -> None:
    """Place an unchanged-time recording-widget crop on the captured editorial stage."""
    source = destination / "workflow.mp4"
    target = destination / "vertical.mp4"
    if target.exists() or not source.is_file() or not background.is_file():
        raise RuntimeError("Vertical export needs an existing workflow/stage and a fresh output")
    font = subprocess.check_output(["fc-match", "-f", "%{file}", "sans-serif"], text=True).strip()
    if not Path(font).is_file():
        raise RuntimeError("Fontconfig did not resolve an installed font")
    filters = (
        "[0:v]crop=500:170:64:750,scale=952:324:flags=lanczos[widget];"
        "[1:v][widget]overlay=64:850:shortest=1,"
        f"drawtext=fontfile={font}:text=REAL RECORDING WIDGET:x=64:y=780:fontsize=24:fontcolor=0xD8DEE9,"
        f"drawtext=fontfile={font}:text=Continuous desktop crop · original timing and audio:"
        "x=64:y=1245:fontsize=21:fontcolor=0xADB5C4[v]"
    )
    subprocess.run(
        [
            "ffmpeg",
            "-hide_banner",
            "-loglevel",
            "error",
            "-i",
            str(source),
            "-loop",
            "1",
            "-i",
            str(background),
            "-filter_complex",
            filters,
            "-map",
            "[v]",
            "-map",
            "0:a:0",
            "-c:v",
            "libx264",
            "-threads",
            "2",
            "-preset",
            "medium",
            "-crf",
            "19",
            "-pix_fmt",
            "yuv420p",
            "-r",
            "30",
            "-c:a",
            "copy",
            "-t",
            inspect(source)["format"]["duration"],
            "-movflags",
            "+faststart",
            str(target),
        ],
        check=True,
    )
    (destination / "vertical-export.json").write_text(
        json.dumps(
            {
                "disclosure": (
                    "Editorial portrait composition of the real desktop recording widget. "
                    "No timing cuts or speed change; original AAC stream copied. "
                    "This is a crop, not a second portrait recognition session."
                ),
                "source": "workflow.mp4",
                "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
                "background_sha256": hashlib.sha256(background.read_bytes()).hexdigest(),
                "filter": filters,
                "font_file": font,
                "font_sha256": hashlib.sha256(Path(font).read_bytes()).hexdigest(),
                "sha256": hashlib.sha256(target.read_bytes()).hexdigest(),
                "media": inspect(target),
            },
            indent=2,
        )
        + "\n"
    )
    print(f"Exported {target}")


def main() -> None:
    """Export a real take with a receipt; never trim silence, waits or provider failures."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("capture", type=Path)
    parser.add_argument("destination", type=Path)
    parser.add_argument("--vertical-background", type=Path, help="Add a portrait crop to an existing continuous export")
    args = parser.parse_args()
    if args.vertical_background:
        export_vertical(args.destination, args.vertical_background)
        return
    receipt = json.loads((args.capture / "capture.json").read_text())
    if not (args.capture / "captured.wav").is_file():
        raise RuntimeError("Capture has no preserved recorder audio")
    match = re.search(r"Duration: N/A, start: ([0-9.]+)", (args.capture / "ffmpeg.log").read_text())
    if match is None or receipt["first_pcm_epoch"] is None:
        raise RuntimeError("Capture lacks measured video/audio timestamps")
    screen_epoch = float(match[1])
    offset = receipt["first_pcm_epoch"] - screen_epoch
    if not 0 <= offset <= 5:
        raise RuntimeError(f"Unexpected audio offset: {offset}")
    args.destination.mkdir(parents=True, exist_ok=False)
    target = args.destination / "workflow.mp4"
    subprocess.run(
        [
            "ffmpeg",
            "-hide_banner",
            "-loglevel",
            "error",
            "-i",
            str(args.capture / "screen.mkv"),
            "-itsoffset",
            f"{offset:.6f}",
            "-i",
            str(args.capture / "captured.wav"),
            "-map",
            "0:v:0",
            "-map",
            "1:a:0",
            "-c:v",
            "libx264",
            "-threads",
            "2",
            "-preset",
            "medium",
            "-crf",
            "19",
            "-pix_fmt",
            "yuv420p",
            "-fps_mode",
            "cfr",
            "-r",
            "30",
            "-c:a",
            "aac",
            "-b:a",
            "96k",
            "-af",
            "apad",
            "-shortest",
            "-movflags",
            "+faststart",
            str(target),
        ],
        check=True,
    )
    for name in (
        "hero.png",
        "recording.png",
        "live.png",
        "widget-recording.png",
        "widget-review.png",
        "recognition.json",
        "replies.json",
        "capture.json",
        "captured.wav",
    ):
        source = args.capture / name
        if source.is_file():
            shutil.copy2(source, args.destination / name)
    files = {}
    for path in sorted(args.destination.iterdir()):
        files[path.name] = {
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
            "bytes": path.stat().st_size,
        }
        if path.suffix in {".mp4", ".wav", ".png"}:
            files[path.name]["media"] = inspect(path)
    (args.destination / "export.json").write_text(
        json.dumps(
            {
                "source_capture": str(args.capture.resolve()),
                "screen_epoch": screen_epoch,
                "first_pcm_epoch": receipt["first_pcm_epoch"],
                "audio_offset_seconds": offset,
                "timing_edits": (
                    "None. Full screen take; original captured PCM delayed to measured first-sample timestamp, "
                    "AAC encoded and padded with trailing silence to video end."
                ),
                "synchronization_limit": (
                    "PCM callback estimates first-sample wall time from packet duration; "
                    "capture timestamps are not hardware genlock."
                ),
                "files": files,
            },
            indent=2,
        )
        + "\n"
    )
    print(f"Exported {target}")


if __name__ == "__main__":
    main()
