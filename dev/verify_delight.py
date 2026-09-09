"""Verify the final launch edit and export review frames plus accurately timed optional captions."""

import argparse
import json
import re
import subprocess
import textwrap
from fractions import Fraction
from pathlib import Path

from compose_delight import fingerprint, probe


def clock(seconds: float) -> str:
    """Format one SubRip timestamp at millisecond precision."""
    millis = round(seconds * 1000)
    hours, remainder = divmod(millis, 3_600_000)
    minutes, remainder = divmod(remainder, 60_000)
    whole, fraction = divmod(remainder, 1000)
    return f"{hours:02}:{minutes:02}:{whole:02},{fraction:03}"


def captions(plan: dict, root: Path) -> tuple[str, list[dict]]:
    """Keep canonical spelling while using measured word timing from each unchanged voice file."""
    entries, checks = [], []
    for cut in plan["voice_cuts"]:
        source = root / cut.get("source", plan["narration"])
        timing_path = source.with_name(source.stem + "-timing.json")
        timing = json.loads(timing_path.read_text())
        if timing["source_sha256"] != fingerprint(source)["sha256"]:
            raise RuntimeError("Word timing does not identify its narration source")
        words = [
            word
            for word in timing["words"]
            if word["type"] == "word" and cut["source_start"] <= word["start"] < cut["source_end"]
        ]
        tokens = cut["text"].split()
        if len(tokens) != len(words) or not words or words[-1]["end"] > cut["source_end"]:
            raise RuntimeError(f"Narration cut splits a word or differs from its canonical caption: {cut['text']}")
        start = 0
        for index, token in enumerate(tokens):
            count = index + 1 - start
            if count >= 8 or (count >= 4 and token.endswith((".", ",", ";"))) or index == len(tokens) - 1:
                begin = cut["at"] + words[start]["start"] - cut["source_start"]
                end = min(
                    cut["at"] + cut["source_end"] - cut["source_start"],
                    cut["at"] + words[index]["end"] - cut["source_start"] + 0.10,
                )
                entries.append((begin, end, "\n".join(textwrap.wrap(" ".join(tokens[start : index + 1]), width=44))))
                start = index + 1
        checks.append(
            {"at": cut["at"], "source": str(source.relative_to(root)), "whole_words": True, "text": cut["text"]}
        )
    result = ""
    for index, (start, end, text) in enumerate(entries):
        if index + 1 < len(entries):
            end = min(end, entries[index + 1][0] - 0.01)
        if end <= start:
            raise RuntimeError("Caption timing overlaps or has no positive duration")
        result += f"{index + 1}\n{clock(start)} --> {clock(end)}\n{text}\n\n"
    return result, checks


def main() -> None:
    """Challenge encoded media, source identity and edit boundaries, then produce a review artifact."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("plan", type=Path)
    parser.add_argument("film", type=Path)
    parser.add_argument("--images", type=Path, required=True)
    parser.add_argument("--poster-at", type=float, default=21)
    args = parser.parse_args()
    plan_path, film = args.plan.resolve(strict=True), args.film.resolve(strict=True)
    plan = json.loads(plan_path.read_text())
    edit_path = film.with_suffix(".edit.json")
    edit = json.loads(edit_path.read_text())
    if fingerprint(plan_path) != edit["plan"] or fingerprint(film) != edit["output"]:
        raise RuntimeError("The film or plan differs from its edit receipt")
    for name, expected in edit["sources"].items():
        if fingerprint((plan_path.parent / name).resolve(strict=True)) != expected:
            raise RuntimeError(f"A source differs from its edit receipt: {name}")
    media = probe(film)
    video = next(stream for stream in media["streams"] if stream["codec_type"] == "video")
    audio = next(stream for stream in media["streams"] if stream["codec_type"] == "audio")
    if (video["codec_name"], video["pix_fmt"], video["width"], video["height"]) != ("h264", "yuv420p", 1920, 1080):
        raise RuntimeError("The launch film is not portable 1080p H.264")
    if Fraction(video["avg_frame_rate"]) != 60 or int(video["nb_frames"]) != 3300:
        raise RuntimeError("The launch film does not contain exactly 3,300 frames at 60 fps")
    if float(media["format"]["duration"]) != 55 or (audio["codec_name"], audio["channels"], audio["sample_rate"]) != (
        "aac",
        2,
        "48000",
    ):
        raise RuntimeError("The launch film duration or stereo audio format is incorrect")
    subprocess.run(["ffmpeg", "-nostdin", "-v", "error", "-i", str(film), "-f", "null", "-"], check=True)
    analysis = subprocess.run(
        [
            "ffmpeg",
            "-nostdin",
            "-hide_banner",
            "-i",
            str(film),
            "-vn",
            "-af",
            "loudnorm=print_format=json",
            "-f",
            "null",
            "-",
        ],
        check=True,
        capture_output=True,
        text=True,
    )
    matches = re.findall(r'\{\s*"input_i".*?\}', analysis.stderr, re.DOTALL)
    if len(matches) != 1:
        raise RuntimeError("Audio loudness analysis did not return one measured result")
    loudness = json.loads(matches[0])
    if not -19 <= float(loudness["input_i"]) <= -13 or float(loudness["input_tp"]) > -1:
        raise RuntimeError("Narration mix is outside the review loudness/headroom bounds")
    subtitle_text, word_checks = captions(plan, plan_path.parent)
    subtitles = film.with_suffix(".srt")
    subtitles.write_text(subtitle_text)
    args.images.mkdir(parents=True, exist_ok=True)
    frames = []
    samples = [("opening", 2.5), ("poster", args.poster_at)] + [
        (f"review-{index:02}", at)
        for index, at in enumerate(
            (0.4, 2.5, 6.8, 11.7, 16.4, 21, 25.2, 28.7, 30.6, 34.5, 39, 43.5, 46.5, 48.5, 51, 53.5)
        )
    ]
    for name, at in samples:
        path = args.images / f"{name}.png"
        subprocess.run(
            ["ffmpeg", "-nostdin", "-v", "error", "-y", "-ss", str(at), "-i", str(film), "-frames:v", "1", str(path)],
            check=True,
        )
        frames.append({"file": path.name, "film_seconds": at, **fingerprint(path)})
    montage = [
        "magick",
        "montage",
        "-background",
        "#151a20",
        "-fill",
        "#eceff4",
        "-font",
        "DejaVu-Sans",
        "-pointsize",
        "18",
    ]
    for item in frames[2:]:
        montage.extend(["-label", f"{item['film_seconds']:.1f} s", str(args.images / item["file"])])
    montage.extend(["-geometry", "480x270+6+8", "-tile", "4x4", str(args.images / "contact-sheet.png")])
    subprocess.run(montage, check=True)
    source_ranges = [
        {
            "clip": clip["id"],
            "timeline": [clip["timeline_start"], clip["timeline_end"]],
            "source": clip["source"],
            "source_seconds": [
                clip.get("source_start", 0),
                clip.get("source_start", 0) + clip["duration"] * clip.get("speed", 1),
            ],
            "speed": clip.get("speed", 1),
        }
        for clip in edit["clips"]
    ]
    qa = {
        "schema_version": 1,
        "media": media,
        "film": fingerprint(film),
        "plan": fingerprint(plan_path),
        "edit_receipt": fingerprint(edit_path),
        "sources_unchanged": True,
        "full_decode_passed": True,
        "frames": 3300,
        "audio_loudness": {
            "integrated_lufs": float(loudness["input_i"]),
            "true_peak_dbtp": float(loudness["input_tp"]),
            "range_lu": float(loudness["input_lra"]),
        },
        "word_timing_checks": word_checks,
        "subtitles": fingerprint(subtitles),
        "source_ranges": source_ranges,
        "review_frames": frames,
        "contact_sheet": fingerprint(args.images / "contact-sheet.png"),
        "verifier": fingerprint(Path(__file__)),
        "review_boundary": (
            "Frame exports require visual inspection; automatic checks do not establish "
            "an auditory listening review or provider quality"
        ),
    }
    film.with_suffix(".qa.json").write_text(json.dumps(qa, indent=2) + "\n")
    print(json.dumps({"film": str(film), "verified": True, "review_frames": str(args.images)}), flush=True)


if __name__ == "__main__":
    main()
