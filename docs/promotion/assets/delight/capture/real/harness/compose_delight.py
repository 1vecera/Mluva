"""Render the short Mluva launch film with crisp software type and native product footage."""

import argparse
import hashlib
import json
import math
import subprocess
from fractions import Fraction
from pathlib import Path


def run(command: list[str]) -> str:
    """Run media tools without shell interpolation."""
    return subprocess.check_output(command, text=True)


def fingerprint(path: Path) -> dict:
    """Retain exact source identity for review and reproduction."""
    return {"sha256": hashlib.sha256(path.read_bytes()).hexdigest(), "bytes": path.stat().st_size}


def probe(path: Path) -> dict:
    """Read the encoded file rather than trusting the requested output geometry."""
    return json.loads(run(["ffprobe", "-v", "error", "-show_streams", "-show_format", "-of", "json", str(path)]))


def quote(text: str) -> str:
    """Escape one literal FFmpeg filter argument."""
    return "'" + text.replace("\\", "\\\\").replace("'", "'\\''").replace(":", "\\:") + "'"


def main() -> None:
    """Compose separate editable shots, mix narration, and verify a portable H.264 file."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("plan", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--preview", action="store_true", help="Render only the opening for visual iteration")
    parser.add_argument("--font", type=Path, help="Explicit capture font when composing on a different host")
    parser.add_argument("--encoder", choices=("libx264", "libopenh264"), default="libx264")
    args = parser.parse_args()
    plan_path = args.plan.resolve(strict=True)
    plan = json.loads(plan_path.read_text())
    if not args.preview and (
        plan.get("status", "ready") != "ready"
        or round(sum(float(clip["duration"]) for clip in plan["clips"]) * 60) != 3300
    ):
        raise ValueError("The 55-second capture-backed edit is pending; use --preview only for the opening study")
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    scratch = Path.cwd() / "tmp/delight-compose" / output.stem
    scratch.mkdir(parents=True, exist_ok=True)
    font = (
        args.font.resolve(strict=True)
        if args.font
        else Path(run(["fc-match", "-f", "%{file}", "Adwaita Sans:style=Semibold"]).strip())
    )
    sources, receipts, shots = {}, [], []
    total = 0.0

    def source(relative: str) -> Path:
        if Path(relative).is_absolute():
            raise ValueError("The edit plan must use relative source paths")
        path = (plan_path.parent / relative).resolve(strict=True)
        sources[relative] = fingerprint(path)
        return path

    def title(text: str, key: str, size: int, x: str, y: str, *, alpha="1", color="0xeceff4") -> str:
        path = scratch / (key + ".txt")
        path.write_text(text)
        return (
            f"drawtext=fontfile={quote(str(font))}:textfile={quote(str(path))}:expansion=none:"
            f"fontsize={size}:fontcolor={color}:x='{x}':y='{y}':alpha='{alpha}'"
        )

    clips = plan["clips"][:1] if args.preview else plan["clips"]
    for index, clip in enumerate(clips):
        seconds = float(clip["duration"])
        start = float(clip.get("source_start", 0))
        speed = float(clip.get("speed", 1))
        if not math.isfinite(seconds + start + speed) or min(seconds, speed) <= 0 or start < 0:
            raise ValueError("Invalid clip duration, speed or start")
        path = source(clip["source"])
        media = probe(path)
        if start + seconds * speed > float(media["format"]["duration"]) + 0.08:
            raise ValueError(f"Clip {clip['id']} exceeds its source")
        target = scratch / f"shot-{index:02}.mp4"
        command = ["ffmpeg", "-nostdin", "-v", "error", "-y", "-ss", str(start), "-i", str(path)]
        filters = []
        if clip["kind"] in {"intro", "outro"}:
            filters.append(
                "[0:v]crop=1152:648:0:60,scale=1920:1080:flags=lanczos,fps=60,"
                "setpts=PTS-STARTPTS,colorchannelmixer=rr=0.53:gg=0.53:bb=0.53[back]"
            )
            logo = source(plan["logo"])
            png = scratch / f"lockup-{fingerprint(logo)['sha256'][:12]}.png"
            if not png.exists():
                subprocess.run(["rsvg-convert", "-w", "1100", "-o", str(png), str(logo)], check=True)
            command.extend(["-loop", "1", "-framerate", "60", "-i", str(png)])
            if clip["kind"] == "intro":
                filters.append(
                    "[1:v]format=rgba,scale=w='760+100*pow(max(0,1-t/0.9),3)':h=-1:eval=frame,"
                    "fade=t=in:d=0.65:alpha=1[brand]"
                )
                filters.append("[back][brand]overlay=x=(W-w)/2:y='286+85*pow(max(0,1-t/0.9),3)'[branded]")
                hook = title(
                    "The most delightful dictation app",
                    f"hook-{index}",
                    49,
                    "(w-tw)/2",
                    "595",
                    alpha="min(max((t-0.65)/0.65,0),1)",
                )
                hook2 = title(
                    "for Omarchy.", f"hook2-{index}", 49, "(w-tw)/2", "657", alpha="min(max((t-1.1)/0.6,0),1)"
                )
                text_filters = [hook, hook2]
            else:
                filters.append("[1:v]format=rgba,scale=540:-1[brand]")
                filters.append("[back][brand]overlay=x=(W-w)/2:y=332[branded]")
                text_filters = [
                    title("A little more delight.", f"finish-{index}", 48, "(w-tw)/2", "540"),
                    title("github.com/1vecera/Mluva", f"link-{index}", 27, "(w-tw)/2", "639", color="0xc6d4e0"),
                ]
            filters.append("[branded]" + ",".join(text_filters) + "[v]")
        else:
            zoom_a, zoom_b = clip.get("zoom", [1, 1])
            focus_x, focus_y = clip.get("focus", [960, 540])
            frames = round(seconds * 60)
            progress = f"min(on/{max(1, frames - 1)},1)"
            ease = f"(({progress})*({progress})*(3-2*({progress})))"
            filters.append(
                f"[0:v]setpts=(PTS-STARTPTS)/{speed},fps=60,scale=3840:2160:flags=lanczos,"
                f"zoompan=z='{zoom_a}+({zoom_b}-{zoom_a})*{ease}':"
                f"x='max(0,min(iw-iw/zoom,{focus_x * 2}-iw/zoom/2))':"
                f"y='max(0,min(ih-ih/zoom,{focus_y * 2}-ih/zoom/2))':d=1:s=1920x1080:fps=60[native]"
            )
            captions = []
            if clip.get("caption"):
                # This small lane sits above the native window, not over its text.
                captions.append(
                    title(
                        clip["caption"],
                        f"caption-{index}",
                        45,
                        "64",
                        "64+16*pow(max(0,1-t/0.45),3)",
                        alpha="min(t/0.35,1)",
                    )
                )
            filters.append("[native]" + (",".join(captions) or "null") + "[v]")
        fade_in = float(clip.get("fade_in", 0.1))
        if not math.isfinite(fade_in) or not 0 <= fade_in < seconds:
            raise ValueError("Invalid edge reveal duration")
        reveal = f",fade=t=in:d={fade_in}:color=0x202630" if fade_in else ""
        filters.append(f"[v]trim=duration={seconds},setsar=1{reveal},format=yuv420p[vout]")
        (scratch / f"shot-{index:02}.filters").write_text(";\n".join(filters) + "\n")
        command.extend(
            [
                "-filter_complex_threads",
                "2",
                "-filter_complex",
                ";".join(filters),
                "-map",
                "[vout]",
                "-an",
                "-c:v",
                args.encoder,
                "-pix_fmt",
                "yuv420p",
                *(["-preset", "fast", "-crf", "17"] if args.encoder == "libx264" else ["-b:v", "12M"]),
                "-threads",
                "4",
                "-t",
                str(seconds),
                "-movflags",
                "+faststart",
                str(target),
            ]
        )
        subprocess.run(command, check=True)
        shots.append(target)
        receipts.append({**clip, "timeline_start": round(total, 4), "timeline_end": round(total + seconds, 4)})
        total += seconds
        print(json.dumps({"shot": clip["id"], "duration": seconds}), flush=True)
    if total >= 60:
        raise ValueError("The launch edit must remain under one minute")
    playlist = scratch / "concat.txt"
    playlist.write_text("".join("file " + quote(str(path)) + "\n" for path in shots))
    silent = scratch / "silent.mp4"
    subprocess.run(
        [
            "ffmpeg",
            "-nostdin",
            "-v",
            "error",
            "-y",
            "-f",
            "concat",
            "-safe",
            "0",
            "-i",
            str(playlist),
            "-c",
            "copy",
            str(silent),
        ],
        check=True,
    )
    if args.preview:
        shutil_command = ["ffmpeg", "-nostdin", "-v", "error", "-y", "-i", str(silent), "-c", "copy", str(output)]
        subprocess.run(shutil_command, check=True)
    else:
        voice_inputs = []
        voice_sources = {}
        audio_filters = []
        for index, line in enumerate(plan["voice_cuts"]):
            voice_name = line.get("source", plan["narration"])
            if voice_name not in voice_sources:
                voice_sources[voice_name] = len(voice_sources) + 1
                voice_inputs.extend(["-i", str(source(voice_name))])
            input_index = voice_sources[voice_name]
            cut_duration = line["source_end"] - line["source_start"]
            audio_filters.append(
                f"[{input_index}:a]atrim=start={line['source_start']}:end={line['source_end']},"
                f"asetpts=PTS-STARTPTS,aformat=sample_rates=48000:channel_layouts=stereo,afade=t=in:d=0.015,"
                f"afade=t=out:st={max(0, cut_duration - 0.025)}:d=0.025,"
                f"adelay={round(line['at'] * 1000)}:all=1[a{index}]"
            )
        audio_inputs = "".join(f"[a{i}]" for i in range(len(plan["voice_cuts"])))
        audio_count = len(plan["voice_cuts"])
        bed_inputs = []
        if plan.get("sound_bed"):
            bed = source(plan["sound_bed"])
            bed_inputs = ["-i", str(bed)]
            bed_index = len(voice_sources) + 1
            audio_filters.append(f"[{bed_index}:a]atrim=duration={total},asetpts=PTS-STARTPTS[synth]")
            audio_inputs += "[synth]"
            audio_count += 1
        audio_filters.append(
            audio_inputs + f"amix=inputs={audio_count}:normalize=0,"
            f"apad=whole_dur={total},atrim=duration={total},volume=0.85,"
            "alimiter=limit=0.89:level=false:latency=true[audio]"
        )
        subprocess.run(
            [
                "ffmpeg",
                "-nostdin",
                "-v",
                "error",
                "-y",
                "-i",
                str(silent),
                *voice_inputs,
                *bed_inputs,
                "-filter_complex",
                ";".join(audio_filters),
                "-map",
                "0:v",
                "-map",
                "[audio]",
                "-c:v",
                "copy",
                "-c:a",
                "aac",
                "-b:a",
                "192k",
                "-ar",
                "48000",
                "-ac",
                "2",
                "-t",
                str(total),
                "-movflags",
                "+faststart",
                str(output),
            ],
            check=True,
        )
    subprocess.run(["ffmpeg", "-nostdin", "-v", "error", "-i", str(output), "-f", "null", "-"], check=True)
    output_media = probe(output)
    video = next(stream for stream in output_media["streams"] if stream["codec_type"] == "video")
    if (video["width"], video["height"], Fraction(video["avg_frame_rate"])) != (1920, 1080, 60):
        raise RuntimeError("The rendered launch film is not 1080p60")
    if video["pix_fmt"] != "yuv420p":
        raise RuntimeError("The rendered launch film is not a portable 4:2:0 video")
    if int(video["nb_frames"]) != round(total * 60):
        raise RuntimeError("The rendered launch film does not match its declared frame count")
    receipt = {
        "schema_version": 1,
        "duration_seconds": total,
        "clips": receipts,
        "voice_cuts": [] if args.preview else plan["voice_cuts"],
        "sources": sources,
        "plan": fingerprint(plan_path),
        "composer": fingerprint(Path(__file__)),
        "encoder": args.encoder,
        "ffmpeg": run(["ffmpeg", "-version"]).splitlines()[0],
        "font": {"file": font.name, **fingerprint(font)},
        "output": fingerprint(output),
        "media": output_media,
        "transitions": "Explicit per-shot edge reveals or direct cuts; continuous eased camera motion",
        "audio": None
        if args.preview
        else "Synthetic Fish narration with phrase cuts; quiet original oscillator bed; no captured provider audio",
        "audio_mix": None
        if args.preview
        else {"sample_rate": 48000, "channels": 2, "gain": 0.85, "limiter": 0.89, "limiter_auto_level": False},
    }
    output.with_suffix(".edit.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps({"output": str(output), "duration": total, "decoded": True}), flush=True)


if __name__ == "__main__":
    main()
