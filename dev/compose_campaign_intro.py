"""Compose an app-led film from portable, original-speed cuts and software typography."""

import argparse
import hashlib
import json
import math
import subprocess
from pathlib import Path


def run(command: list[str]) -> str:
    """Resolve metadata without shell interpolation."""
    return subprocess.check_output(command, text=True)


def quoted(value: str) -> str:
    """Escape literal paths for FFmpeg's filter grammar."""
    return "'" + value.replace("\\", "\\\\").replace("'", "'\\''").replace(":", "\\:") + "'"


def inspect(path: Path) -> dict:
    """Read encoded geometry, timing and streams."""
    return json.loads(
        run(
            [
                "ffprobe",
                "-v",
                "error",
                "-show_streams",
                "-show_format",
                "-of",
                "json",
                str(path),
            ]
        )
    )


def fingerprint(path: Path) -> dict:
    """Bind every input to the bytes actually rendered."""
    return {
        "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        "bytes": path.stat().st_size,
    }


def main() -> None:
    """Render native UI at large scale with short captions and explicit source time maps."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("plan", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument(
        "--brand",
        type=Path,
        help="Replace the plan's brand JSON without editing the timeline",
    )
    args = parser.parse_args()
    plan_path = args.plan.resolve(strict=True)
    plan = json.loads(plan_path.read_text())
    if plan["schema_version"] != 2:
        raise ValueError("Use a version 2 film plan")
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    scratch = Path.cwd() / "tmp/film-compose" / output.stem
    scratch.mkdir(parents=True, exist_ok=True)
    brand_path = (
        args.brand.resolve(strict=True) if args.brand else (plan_path.parent / plan["brand"]).resolve(strict=True)
    )
    brand = json.loads(brand_path.read_text())
    font = Path(run(["fc-match", "-f", "%{file}", brand["font_family"]]).strip())
    command = ["ffmpeg", "-nostdin", "-hide_banner", "-loglevel", "warning", "-y"]
    filters, receipts = [], []
    inputs, total = 0, 0.0

    def source_path(relative: str, parent: Path = plan_path.parent) -> Path:
        """Require relocatable recipe paths."""
        if Path(relative).is_absolute():
            raise ValueError("Portable plans require relative input paths")
        return (parent / relative).resolve(strict=True)

    def add_input(path: Path, duration: float, still: bool = False) -> int:
        """Bound still images so composition terminates."""
        nonlocal inputs
        index = inputs
        inputs += 1
        if still:
            command.extend(["-loop", "1", "-framerate", "30", "-t", str(duration)])
        command.extend(["-i", str(path)])
        return index

    def text_filter(value: str, name: str, size: int, x: str, y: str, color: str = "0xeceff4") -> str:
        """Render literal text without generated letters or filter expansion."""
        caption = scratch / f"{name}.txt"
        caption.write_text(value)
        return (
            f"drawtext=fontfile={quoted(str(font))}:textfile={quoted(str(caption))}:expansion=none:"
            f"fontsize={size}:fontcolor={color}:x={x}:y={y}"
        )

    for index, clip in enumerate(plan["clips"]):
        source = source_path(clip["source"])
        info = inspect(source)
        start, duration = float(clip["start"]), float(clip["duration"])
        if (
            not math.isfinite(start + duration)
            or start < 0
            or duration < 1
            or abs(duration * 30 - round(duration * 30)) > 1e-5
        ):
            raise ValueError("Cuts require nonnegative starts and whole-frame durations of at least one second")
        still = clip["type"] == "still"
        if not still and start + duration > float(info["format"]["duration"]) + 0.02:
            raise ValueError(f"Cut exceeds source duration: {source.name}")
        stream = next(stream for stream in info["streams"] if stream["codec_type"] == "video")
        x, y, width, height = clip["crop"]
        if min(x, y) < 0 or min(width, height) < 2 or x + width > stream["width"] or y + height > stream["height"]:
            raise ValueError(f"Crop exceeds source frame: {source.name}")
        source_index = add_input(source, duration, still)
        trim = f"[{source_index}:v]trim=start={start}:duration={duration},setpts=PTS-STARTPTS,"
        crop = f"crop={width}:{height}:{x}:{y},setsar=1,fps=30,"
        extra_inputs = {}
        if clip["type"] in {"intro", "outro", "transition"}:
            # Generated scenery is text/UI-free; all typography and app pixels stay separate.
            filters.append(
                trim + crop + "scale=1920:1080:flags=lanczos,"
                f"colorchannelmixer=rr=0.65:gg=0.65:bb=0.65[background{index}]"
            )
            if clip["type"] == "intro":
                motion = clip["intro_motion"]
                ui = source_path(clip["ui"])
                ui_index = add_input(ui, duration, True)
                ux, uy, uw, uh = clip["ui_crop"]
                filters.append(
                    f"[{ui_index}:v]crop={uw}:{uh}:{ux}:{uy},scale=1840:920:flags=lanczos,"
                    f"format=rgba,fade=t=in:st={motion['app_start']}:"
                    f"d={motion['app_duration']}:alpha=1[ui{index}]"
                )
                filters.append(
                    f"[background{index}][ui{index}]overlay=x=40:"
                    f"y='94+1000*pow(max(0,1-(t-{motion['app_start']})/{motion['app_duration']}),3)':"
                    f"shortest=1[body{index}]"
                )
                extra_inputs["ui"] = {"path": clip["ui"], **fingerprint(ui)}
            else:
                filters.append(f"[background{index}]null[body{index}]")
        else:
            filters.append(
                trim + crop + "scale=1920:1000:force_original_aspect_ratio=decrease:flags=lanczos,"
                "pad=1920:1000:(ow-iw)/2:(oh-ih)/2:color=0x242b36,"
                f"pad=1920:1080:0:80:color=0x242b36[body{index}]"
            )

        video = f"body{index}"
        if clip["type"] in {"intro", "outro"}:
            wordmark = source_path(brand["wordmark"], brand_path.parent)
            logo = scratch / f"wordmark-{index}.png"
            raster_width = "600" if clip["type"] == "intro" else "480"
            raster_height = "200" if clip["type"] == "intro" else "160"
            if wordmark.suffix == ".svg":
                subprocess.run(
                    [
                        "rsvg-convert",
                        "--keep-aspect-ratio",
                        "-w",
                        raster_width,
                        "-h",
                        raster_height,
                        "-o",
                        str(logo),
                        str(wordmark),
                    ],
                    check=True,
                )
            else:
                subprocess.run(
                    [
                        "ffmpeg",
                        "-nostdin",
                        "-v",
                        "error",
                        "-y",
                        "-i",
                        str(wordmark),
                        "-vf",
                        f"scale={raster_width}:{raster_height}:force_original_aspect_ratio=decrease",
                        str(logo),
                    ],
                    check=True,
                )
            logo_index = add_input(logo, duration, True)
            if clip["type"] == "intro":
                motion = clip["intro_motion"]
                progress = f"min(max((t-{motion['brand_hold']})/{motion['brand_duration']},0),1)"
                ease = f"(1-pow(1-({progress}),3))"
                hero_width = motion["brand_width"]
                filters.append(
                    f"[{logo_index}:v]scale=w='{hero_width}-({hero_width}-240)*{ease}':h=-1:eval=frame[logo{index}]"
                )
                filters.append(
                    f"[{video}][logo{index}]overlay=x='(W-w)/2+(40-(W-w)/2)*{ease}':"
                    f"y='355+(14-355)*{ease}':shortest=1[branded{index}]"
                )
            else:
                filters.append(f"[{video}][{logo_index}:v]overlay=x=(W-w)/2:y=330:shortest=1[branded{index}]")
            video = f"branded{index}"
            extra_inputs["wordmark"] = {
                "path": brand["wordmark"],
                **fingerprint(wordmark),
            }

        captions = []
        if clip["type"] == "jfk":
            # Cover the initial empty UI, but never advance it against the archival audio.
            end = clip["source_card_seconds"]
            captions.extend(
                [
                    f"drawbox=x=0:y=80:w=iw:h=1000:color=0x242b36:t=fill:enable='lt(t,{end})'",
                    text_filter("JFK at Rice University", f"jfk-{index}", 68, "(w-tw)/2", "405")
                    + f":enable='lt(t,{end})'",
                    text_filter(
                        "12 September 1962 · Original archival audio",
                        f"jfk-date-{index}",
                        30,
                        "(w-tw)/2",
                        "520",
                        "0xb8c4d3",
                    )
                    + f":enable='lt(t,{end})'",
                ]
            )
        if clip["type"] == "outro":
            captions.extend(
                [
                    text_filter(brand["hook"], f"hook-{index}", 52, "(w-tw)/2", "540"),
                    text_filter(
                        brand["link"],
                        f"link-{index}",
                        28,
                        "(w-tw)/2",
                        "650",
                        "0xb8c4d3",
                    ),
                ]
            )
        elif clip["type"] == "intro":
            captions.extend(
                [
                    text_filter(
                        brand["hook"],
                        f"hero-hook-{index}",
                        50,
                        f"'(w-tw)/2+(320-(w-tw)/2)*{ease}'",
                        f"'565+(24-565)*{ease}'",
                    )
                    + ":alpha='min(max(1-(t-2)/0.28,0),1)'",
                    text_filter(brand["hook"], f"header-hook-{index}", 32, "320", "24")
                    + ":alpha='min(max((t-2.35)/0.4,0),1)'",
                    text_filter(clip["label"], f"label-{index}", 23, "w-tw-40", "30", "0xb8c4d3")
                    + ":alpha='min(max((t-2.2)/0.55,0),1)'",
                ]
            )
        elif clip["type"] != "transition":
            captions.extend(
                [
                    "drawbox=x=0:y=0:w=iw:h=80:color=0x242b36@0.95:t=fill",
                    text_filter(clip["title"], f"title-{index}", 32, "40", "24"),
                    text_filter(clip["label"], f"label-{index}", 23, "w-tw-40", "30", "0xb8c4d3"),
                ]
            )
        filters.append(
            f"[{video}]" + (",".join(captions) or "null") + ",format=yuv420p,setsar=1,settb=AVTB,"
            f"fade=t=in:d=0.2:color=0x242b36,fade=t=out:st={duration - 0.2}:d=0.2:color=0x242b36[v{index}]"
        )
        if clip["audio"]:
            if not any(stream["codec_type"] == "audio" for stream in info["streams"]):
                raise ValueError("Requested source audio is absent")
            # Subtract the cut timestamp, not the first sample, preserving measured delay.
            filters.append(
                f"[{source_index}:a]atrim=start={start}:duration={duration},asetpts=PTS-{start}/TB,"
                "aresample=48000:async=1:first_pts=0,aformat=sample_fmts=fltp:channel_layouts=stereo,"
                f"apad=whole_dur={duration},atrim=duration={duration},volume={clip['gain']},"
                f"afade=t=in:d=0.02,afade=t=out:st={duration - 0.02}:d=0.02[a{index}]"
            )
        else:
            filters.append(f"anullsrc=r=48000:cl=stereo,atrim=duration={duration}[a{index}]")
        receipts.append(
            {
                **clip,
                "timeline_start": round(total, 6),
                "timeline_end": round(total + duration, 6),
                "source_sha256": fingerprint(source)["sha256"],
                "source_media": info,
                "extra_inputs": extra_inputs,
            }
        )
        total += duration

    count = len(receipts)
    filters.append("".join(f"[v{i}][a{i}]" for i in range(count)) + f"concat=n={count}:v=1:a=1[outv][outa]")
    (scratch / "filters.txt").write_text(";\n".join(filters) + "\n")
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
            "19",
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
        "schema_version": 2,
        "description": plan["description"],
        "clips": receipts,
        "duration_seconds": round(total, 6),
        "playback_speed": "1x; no source retiming",
        "transitions": "0.2-second software fade at clip edges; time cuts without overlap",
        "audio": "Source time and initial delay preserved; 20 ms edge fades, declared gain, resampling and AAC only",
        "brand": {"input": brand_path.name, **brand, **fingerprint(brand_path)},
        "font": {
            "requested": brand["font_family"],
            "resolved": font.name,
            **fingerprint(font),
        },
        "plan_sha256": fingerprint(plan_path)["sha256"],
        "composer_sha256": fingerprint(Path(__file__))["sha256"],
        "output_file": output.name,
        "output_sha256": fingerprint(output)["sha256"],
        "output_bytes": output.stat().st_size,
    }
    output.with_suffix(".edit.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(
        json.dumps(
            {
                "file": str(output),
                "duration_seconds": total,
                "bytes": output.stat().st_size,
            }
        )
    )


if __name__ == "__main__":
    main()
