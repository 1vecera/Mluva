# /// script
# requires-python = ">=3.13"
# dependencies = ["pillow>=11,<13", "numpy>=2,<3"]
# ///
"""Compare browser output against untouched capture pixels over the same black backing."""

import hashlib
import json
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw

root = Path(__file__).resolve().parents[1]
manifest = json.loads((root / "reference/captures.json").read_text())
results = []


def backed(path):
    source = Image.open(path).convert("RGBA")
    return Image.alpha_composite(
        Image.new("RGBA", source.size, "black"), source
    ).convert("RGB")


def compare(name, expected, actual):
    if expected.size != actual.size:
        raise ValueError(f"Size mismatch: {name}")
    difference = np.abs(
        np.asarray(expected, dtype=np.int16) - np.asarray(actual, dtype=np.int16)
    )
    result = {
        "state": name,
        "maximum_channel_error": int(difference.max()),
        "mean_absolute_error": float(difference.mean()),
        "pixels_over_one_level": int((difference.max(axis=2) > 1).sum()),
    }
    results.append(result)
    return difference


for capture in manifest["captures"]:
    source = root / "public/ui" / capture["file"]
    if hashlib.sha256(source.read_bytes()).hexdigest() != capture["sha256"]:
        raise ValueError(f"Source hash differs: {capture['file']}")
    expected = backed(source)
    actual = Image.open(root / "out/fidelity" / capture["file"]).convert("RGB")
    compare(capture["file"], expected, actual)

x, y, w, h = json.loads((root / "reference/clock.json").read_text())
box = tuple(round(v * 2) for v in (x, y, x + w, y + h))
for clock in [0, 8, 12]:
    expected = backed(root / "public/ui/grilling.png")
    # Independently use the untouched full native clock capture, not the imported patch.
    clock_source = backed(root / f"public/ui/clock-{clock:02}.png")
    expected.paste(clock_source.crop(box), box)
    actual = Image.open(root / f"out/fidelity/grilling-clock-{clock}.png").convert(
        "RGB"
    )
    compare(f"grilling-clock-{clock}", expected, actual)

report = {
    "reference": "Untouched native PNG, independently composited over black with Pillow; no browser reference component.",
    "scope": "Settled pixels at capture resolution and three timer substitutions. Does not establish provider latency, independent vector redrawing, or perceptual motion/audio quality.",
    "states": results,
}
(root / "out/fidelity/results.json").write_text(json.dumps(report, indent=2) + "\n")
sheet = Image.new("RGB", (1440, 3 * 360), "#0b111a")
draw = ImageDraw.Draw(sheet)
for row, name in enumerate(["polished", "grilling", "provider-settings"]):
    expected = backed(root / f"public/ui/{name}.png")
    actual = Image.open(root / f"out/fidelity/{name}.png").convert("RGB")
    difference = abs(
        np.asarray(expected, dtype=np.int16) - np.asarray(actual, dtype=np.int16)
    )
    heat = Image.fromarray(np.minimum(difference * 30, 255).astype(np.uint8))
    for col, (label, im) in enumerate(
        [
            ("Native capture", expected),
            ("Remotion reconstruction", actual),
            ("Difference x30", heat),
        ]
    ):
        im.thumbnail((480, 315))
        sheet.paste(im, (col * 480, row * 360 + 36))
        draw.text((col * 480 + 14, row * 360 + 12), f"{name}: {label}", fill="white")
sheet.save(root / "out/fidelity/comparison.png")
print(
    json.dumps(
        {
            "comparisons": len(results),
            "max_channel_error": max(r["maximum_channel_error"] for r in results),
            "pixels_over_one_level": sum(r["pixels_over_one_level"] for r in results),
        }
    )
)
if any(r["pixels_over_one_level"] for r in results):
    raise SystemExit(
        "Fidelity failed: inspect results and the independent source comparison."
    )
