"""Edit existing campaign media into a short film, with explicit source/time provenance."""

import argparse
import hashlib
import json
import subprocess
from pathlib import Path


def run(command: list[str]) -> str:
    """Read media metadata or resolve the installed caption font."""
    return subprocess.check_output(command, text=True)


def quoted(value: str) -> str:
    """Escape a literal path for FFmpeg's filter grammar."""
    return "'" + value.replace("\\", "\\\\").replace("'", "'\\''").replace(":", "\\:") + "'"


def main() -> None:
    """Compose explicit source cuts at original speed and retain their hashes."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("plan", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    plan = json.loads(args.plan.read_text())
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    scratch = output.parent / "intro-edit"
    scratch.mkdir(exist_ok=True)
    font = run(["fc-match", "-f", "%{file}", "sans-serif"]).strip()
    command = ["ffmpeg", "-nostdin", "-hide_banner", "-loglevel", "warning", "-y"]
    filters, receipts, durations = [], [], []
    overlap = 0.4
    for index, clip in enumerate(plan["clips"]):
        source = (args.plan.parent / clip["source"]).resolve(strict=True)
        info = json.loads(
            run(
                [
                    "ffprobe",
                    "-v",
                    "error",
                    "-show_entries",
                    "format=duration:stream=codec_type,width,height,r_frame_rate",
                    "-of",
                    "json",
                    str(source),
                ]
            )
        )
        start, duration = float(clip["start"]), float(clip["duration"])
        assert start >= 0 and duration > overlap and start + duration <= float(info["format"]["duration"]) + 0.05
        assert {stream["codec_type"] for stream in info["streams"]} >= {"video", "audio"}
        command.extend(["-i", str(source)])
        caption = scratch / f"caption-{index}.txt"
        caption.write_text(clip["label"])
        filters.append(
            f"[{index}:v]trim=start={start}:duration={duration},setpts=PTS-STARTPTS,"
            "scale=1920:1080:force_original_aspect_ratio=decrease,"
            "pad=1920:1080:(ow-iw)/2:(oh-ih)/2:color=0x172029,setsar=1,fps=30,format=yuv420p,"
            f"drawtext=fontfile={quoted(font)}:textfile={quoted(str(caption))}:"
            "fontsize=22:fontcolor=0xd8dee9:x=w-tw-48:y=h-56:box=1:boxcolor=0x172029@0.85:boxborderw=10"
            f"[v{index}]"
        )
        filters.append(
            f"[{index}:a]atrim=start={start}:duration={duration},asetpts=PTS-{start}/TB,"
            "aresample=48000:async=1:first_pts=0,aformat=sample_fmts=fltp:channel_layouts=stereo,"
            f"apad=whole_dur={duration},atrim=duration={duration},volume={float(clip.get('volume', 1.0))}[a{index}]"
        )
        durations.append(duration)
        receipts.append(
            {
                **{k: v for k, v in clip.items() if k != "source"},
                "source_file": source.name,
                "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
                "source_media": info,
            }
        )
    count = len(durations)
    duration = 3.5
    title = scratch / "title.txt"
    subtitle = scratch / "subtitle.txt"
    link = scratch / "link.txt"
    title.write_text("Mluva")
    subtitle.write_text("Think aloud. Make it clear.")
    link.write_text("Built for Omarchy. Open source.\ngithub.com/1vecera/Mluva")
    filters.append(
        f"color=c=0x202833:s=1920x1080:r=30:d={duration},format=yuv420p,"
        f"drawtext=fontfile={quoted(font)}:textfile={quoted(str(title))}:fontsize=112:fontcolor=0x9edbc7:x=100:y=252,"
        f"drawtext=fontfile={quoted(font)}:textfile={quoted(str(subtitle))}:fontsize=64:fontcolor=0xeceff4:x=100:y=421,"
        f"drawtext=fontfile={quoted(font)}:textfile={quoted(str(link))}:fontsize=32:line_spacing=20:fontcolor=0xaeb8c8:x=104:y=650"
        f"[v{count}]"
    )
    filters.append(f"anullsrc=r=48000:cl=stereo,atrim=duration={duration}[a{count}]")
    durations.append(duration)
    video, sound, total = "v0", "a0", durations[0]
    for index in range(1, len(durations)):
        next_video, next_sound = f"joinedv{index}", f"joineda{index}"
        filters.append(
            f"[{video}][v{index}]xfade=transition=fade:duration={overlap}:offset={total - overlap}[{next_video}]"
        )
        filters.append(f"[{sound}][a{index}]acrossfade=d={overlap}:c1=tri:c2=tri[{next_sound}]")
        video, sound = next_video, next_sound
        total += durations[index] - overlap
    filters.append(f"[{video}]fade=t=out:st={total - 0.5}:d=0.5[outv]")
    filters.append(f"[{sound}]afade=t=out:st={total - 0.5}:d=0.5[outa]")
    graph = scratch / "filters.txt"
    graph.write_text(";\n".join(filters) + "\n")
    command.extend(
        [
            "-filter_complex_threads",
            "2",
            "-filter_complex",
            ";".join(filters),
            "-map",
            "[outv]",
            "-map",
            "[outa]",
            "-c:v",
            "libx264",
            "-preset",
            "medium",
            "-crf",
            "20",
            "-pix_fmt",
            "yuv420p",
            "-threads",
            "4",
            "-c:a",
            "aac",
            "-b:a",
            "160k",
            "-movflags",
            "+faststart",
            "-t",
            str(total),
            str(output),
        ]
    )
    subprocess.run(command, check=True)
    receipt = {
        "description": (
            "Edited product film: disclosed AI transition, cuts from continuous real screen recordings, "
            "vector title card."
        ),
        "clips": receipts,
        "transitions": {"kind": "crossfade", "seconds": overlap},
        "title_card_seconds": duration,
        "duration_seconds": total,
        "playback_speed": "1x for every source excerpt; no sped-up provider output",
        "audio": (
            "Source timestamps preserved relative to each cut, including initial audio delay; "
            "transition gain as recorded above; silence padding, crossfades, resampling and AAC encoding only."
        ),
        "output_file": output.name,
        "output_sha256": hashlib.sha256(output.read_bytes()).hexdigest(),
        "output_bytes": output.stat().st_size,
    }
    output.with_suffix(".edit.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps({"file": str(output), "duration_seconds": total, "bytes": output.stat().st_size}))


if __name__ == "__main__":
    main()
