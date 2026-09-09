# /// script
# requires-python = ">=3.13"
# dependencies = ["numpy>=2,<3"]
# ///
"""Verify film duration, source integrity, decoded pixels and sample-aligned real audio."""

import argparse
import json
import subprocess
from pathlib import Path

import numpy as np
from compose_campaign_intro import fingerprint, inspect


def audio_samples(path: Path) -> np.ndarray:
    """Decode from the media clock origin, including any initial audio-stream delay."""
    raw = subprocess.check_output(
        [
            "ffmpeg",
            "-nostdin",
            "-v",
            "error",
            "-i",
            str(path),
            "-map",
            "0:a:0",
            "-af",
            "aresample=48000:async=1:first_pts=0",
            "-ac",
            "1",
            "-f",
            "f32le",
            "-",
        ]
    )
    return np.frombuffer(raw, dtype="<f4")


def correlation(first: np.ndarray, second: np.ndarray) -> float:
    """Compare waveform shape independently of level and DC offset."""
    first, second = first.astype(np.float64), second.astype(np.float64)
    first -= first.mean()
    second -= second.mean()
    return float(np.dot(first, second) / np.sqrt(np.dot(first, first) * np.dot(second, second)))


def main() -> None:
    """Fail on retiming, a changed source, corrupt frames or a shifted archival/Live voice."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("plan", type=Path)
    parser.add_argument("film", type=Path)
    parser.add_argument(
        "--images",
        type=Path,
        help="Export feature screenshots and a poster into this assets directory",
    )
    args = parser.parse_args()
    plan = json.loads(args.plan.read_text())
    edit = json.loads(args.film.with_suffix(".edit.json").read_text())
    assert edit["plan_sha256"] == fingerprint(args.plan)["sha256"]
    assert edit["output_sha256"] == fingerprint(args.film)["sha256"]
    assert edit["composer_sha256"] == fingerprint(Path(__file__).with_name("compose_campaign_intro.py"))["sha256"]
    brand_path = args.plan.parent / plan["brand"]
    assert edit["brand"]["sha256"] == fingerprint(brand_path)["sha256"]
    assert all("h3-max-transition-raw" not in clip["source"] for clip in plan["clips"])
    for clip in edit["clips"]:
        assert fingerprint(args.plan.parent / clip["source"])["sha256"] == clip["source_sha256"]
        for role, extra in clip["extra_inputs"].items():
            parent = brand_path.parent if role == "wordmark" else args.plan.parent
            assert fingerprint(parent / extra["path"])["sha256"] == extra["sha256"]
    media = inspect(args.film)
    video = next(stream for stream in media["streams"] if stream["codec_type"] == "video")
    audio = next(stream for stream in media["streams"] if stream["codec_type"] == "audio")
    duration = sum(clip["duration"] for clip in plan["clips"])
    assert (video["width"], video["height"], video["codec_name"], video["pix_fmt"]) == (
        1920,
        1080,
        "h264",
        "yuv420p",
    )
    assert video["r_frame_rate"] == "30/1" and int(video["nb_frames"]) == round(duration * 30)
    assert abs(float(media["format"]["duration"]) - duration) < 0.04
    assert audio["codec_name"] == "aac" and audio["sample_rate"] == "48000" and audio["channels"] == 2
    subprocess.run(
        [
            "ffmpeg",
            "-nostdin",
            "-v",
            "error",
            "-xerror",
            "-i",
            str(args.film),
            "-f",
            "null",
            "-",
        ],
        check=True,
    )
    actual = audio_samples(args.film)
    audio_checks = []
    for clip in edit["clips"]:
        # Exclude deliberate clip-edge fades. Saved-result silence is checked as silence below.
        offset = clip["timeline_start"]
        if clip["id"] in {"dictation", "live-rewrite"}:
            reference = audio_samples(args.plan.parent / clip["source"])
            source_start = clip["start"] + 0.5
            film_start = offset + 0.5
            seconds = min(10, clip["duration"] - 1)
            length = round(seconds * 48000)
            source_segment = reference[round(source_start * 48000) : round(source_start * 48000) + length]
            film_segment = actual[round(film_start * 48000) : round(film_start * 48000) + length]
            zero_shift = correlation(source_segment, film_segment)
            # Search +/- 100 ms at 1 ms resolution on a two-second, 8 kHz window.
            source_window = source_segment[:96000:6]
            center = round(film_start * 48000)
            shifts = range(-100, 101)
            scores = {
                shift: correlation(
                    source_window,
                    actual[center + shift * 48 : center + shift * 48 + 96000 : 6],
                )
                for shift in shifts
            }
            best_shift = max(scores, key=scores.__getitem__)
            assert zero_shift > 0.995, (clip["id"], zero_shift)
            assert abs(best_shift) <= 1, (clip["id"], best_shift)
            audio_checks.append(
                {
                    "clip": clip["id"],
                    "correlation_at_zero_shift": zero_shift,
                    "best_shift_ms": best_shift,
                    "window_seconds": seconds,
                    "source_start": source_start,
                    "film_start": film_start,
                }
            )
        elif not clip["audio"] or clip["id"] == "saved-draft":
            quiet = actual[round((offset + 0.1) * 48000) : round((offset + clip["duration"] - 0.1) * 48000)]
            assert float(np.max(np.abs(quiet))) < 0.005, clip["id"]
    images = {}
    if args.images:
        shots = args.images / "polished/screenshots"
        shots.mkdir(parents=True, exist_ok=True)
        frame_offsets = {
            "opening": 3.4,
            "dictation": 12.5,
            "polish": 5,
            "live-rewrite": 8,
            "floating-controls": 2,
            "expanded-editing": 4,
            "history-search": 4,
            "export": 3,
            "provider-choices": 3.4,
        }
        for clip in edit["clips"]:
            if clip["id"] not in frame_offsets:
                continue
            destination = (
                args.images / "mluva-product-poster.png" if clip["id"] == "opening" else shots / (clip["id"] + ".png")
            )
            moment = clip["timeline_start"] + frame_offsets[clip["id"]]
            subprocess.run(
                [
                    "ffmpeg",
                    "-nostdin",
                    "-v",
                    "error",
                    "-y",
                    "-ss",
                    str(moment),
                    "-i",
                    str(args.film),
                    "-frames:v",
                    "1",
                    str(destination),
                ],
                check=True,
            )
            pixels = subprocess.check_output(
                [
                    "ffmpeg",
                    "-nostdin",
                    "-v",
                    "error",
                    "-i",
                    str(destination),
                    "-vf",
                    "scale=192:108",
                    "-pix_fmt",
                    "rgb24",
                    "-f",
                    "rawvideo",
                    "-",
                ]
            )
            assert len(pixels) == 192 * 108 * 3 and np.frombuffer(pixels, dtype=np.uint8).std() > 10
            images[str(destination.relative_to(args.images))] = {
                "film_seconds": moment,
                **fingerprint(destination),
            }
    result = {
        "output_sha256": fingerprint(args.film)["sha256"],
        "duration_seconds": duration,
        "frames": int(video["nb_frames"]),
        "format": "1920x1080 / 30 fps / H.264 yuv420p / AAC 48 kHz stereo",
        "decode": "Complete video and audio decode passed",
        "audio_checks": audio_checks,
        "peak_audio_amplitude": float(np.max(np.abs(actual))),
        "silent_fixture_and_generated_sections": "Passed",
        "source_hashes": "Sources, UI/brand inputs, plan and composer match edit receipt",
        "old_generated_UI_in_timeline": False,
        "screenshots": images,
        "verification_numpy": np.__version__,
        "limits": (
            "Audio alignment is relative to the original captured screen/PCM clock, "
            "not a physical-microphone measurement. Human visual review is separate."
        ),
    }
    args.film.with_suffix(".qa.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
