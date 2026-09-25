# /// script
# requires-python = ">=3.12"
# dependencies = ["fonttools", "skia-pathops"]
# ///
"""Stage the showreel's inputs under public/showreel/ from captures, brand files and the score.

Run from launch-video/ after capture_widget.py has written tmp/showreel/widget and tmp/showreel/themes:

    uv run showreel/prepare.py --audio ../tmp/showreel/audio

Captures are cropped and encoded, never retouched. Brand files are copied from docs/brand unchanged.
"""

import argparse
import hashlib
import json
import shutil
import subprocess
from pathlib import Path

from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

ROOT = Path(__file__).resolve().parents[1]
REPO = ROOT.parent
CAPTURES = REPO / "tmp/showreel"
OUT = ROOT / "public/showreel"
BRAND = ["mluva-mark.svg", "mluva-mark-flat.svg", "mluva-wordmark-on-dark.svg", "mluva-logo-large-mark-on-dark.svg"]
SCORE = "music-1.mp3"
SFX = {
    "impact": "massive-cinematic-sub-bass_k2oyp.mp3",
    "impact-long": "massive-cinematic-sub-bass_VRBuW.mp3",
    "whoosh": "fast-airy-whoosh_8m4uS.mp3",
    "whoosh-soft": "fast-airy-whoosh_hk3nd.mp3",
    "glitch": "short-digital-glitch_KuJyn.mp3",
    "tick": "tiny-soft-ui_A0IpI.mp3",
    "riser": "tense-white-noise-riser_PRkdb.mp3",
    "key": "single-deep-mechanical_qDf6b.mp3",
    "flap": "split-flap-departure-board_qaW6O.mp3",
    "flash": "bright-flash-pop_2eU6P.mp3",
    "jelly": "soft-glossy-jelly_jdBPe.mp3",
    "blips": "rapid-subtle-computer_Ztd92.mp3",
}


def run(*command: str) -> None:
    subprocess.run(command, check=True)


def crop_box(take: dict, phase: str) -> tuple[int, int, int, int]:
    """Return the widget rectangle inside the capture region, in capture pixels."""
    scale, region, state = take["scale"], take["region"], take[phase]
    return (
        round(state["x"] * scale - region["x"]),
        round(state["y"] * scale - region["y"]),
        round(state["width"] * scale),
        round(state["height"] * scale),
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--audio", type=Path, required=True, help="Folder with the score and SFX takes")
    args = parser.parse_args()
    OUT.mkdir(parents=True, exist_ok=True)
    take = json.loads((CAPTURES / "widget/widget-take.json").read_text())

    # Real widget take: every lossless RGB frame as PNG, so the reel shows the capture's exact pixels.
    frames = OUT / "widget"
    shutil.rmtree(frames, ignore_errors=True)
    frames.mkdir()
    run("ffmpeg", "-v", "error", "-i", str(CAPTURES / "widget/widget-take.mkv"), "-start_number", "0",
        str(frames / "%04d.png"))  # fmt: skip
    (OUT / "widget-take.json").write_text(json.dumps(take, indent=2))

    # Real widget stills per Omarchy theme, cropped to the widget, halved for the sphere cards.
    themes = json.loads((CAPTURES / "themes/themes/themes.json").read_text())
    folder = OUT / "themes"
    shutil.rmtree(folder, ignore_errors=True)
    folder.mkdir()
    for phase, suffix in (("recording", "recording"), ("ready", "review")):
        x, y, w, h = crop_box(take, phase)
        for record in themes["themes"]:
            source = CAPTURES / f"themes/themes/{record['theme']}-{suffix}.png"
            run("magick", str(source), "-crop", f"{w}x{h}+{x}+{y}", "+repage", "-resize", "50%",
                str(folder / f"{record['theme']}-{suffix}.png"))  # fmt: skip
    (folder / "themes.json").write_text(json.dumps(themes, indent=2))

    # Outlined display type needs a static Black instance without overlapping contours;
    # -webkit-text-stroke on the variable font draws every overlap as a seam.
    fonts = OUT / "fonts"
    fonts.mkdir(exist_ok=True)
    black = instancer.instantiateVariableFont(
        TTFont(ROOT / "public/fonts/AdwaitaSans-Regular.ttf"), {"wght": 900}, overlap=instancer.OverlapMode.REMOVE
    )
    black.save(fonts / "AdwaitaSans-Black-NoOverlap.ttf")

    brand = OUT / "brand"
    brand.mkdir(exist_ok=True)
    for name in BRAND:
        shutil.copy2(REPO / "docs/brand/svg" / name, brand / name)

    audio = OUT / "audio"
    audio.mkdir(exist_ok=True)
    shutil.copy2(args.audio / SCORE, audio / "score.mp3")
    for key, name in SFX.items():
        shutil.copy2(args.audio / "sfx" / name, audio / f"{key}.mp3")

    manifest = {
        str(path.relative_to(OUT)): hashlib.sha256(path.read_bytes()).hexdigest()
        for path in sorted(OUT.rglob("*"))
        if path.is_file() and path.name != "media.json"
    }
    (OUT / "media.json").write_text(json.dumps(manifest, indent=2))
    print(f"Staged {len(manifest)} files in {OUT}")


if __name__ == "__main__":
    main()
