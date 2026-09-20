# /// script
# requires-python = ">=3.13"
# dependencies = ["pillow>=11,<13"]
# ///
"""Validate one completed native capture before replacing any film assets."""

import argparse
import hashlib
import json
import shutil
import struct
from pathlib import Path

from PIL import Image, ImageChops

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("capture", type=Path)
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
manifest = json.loads((args.capture / "manifest.json").read_text())
if manifest["errors"]:
    raise ValueError("Capture failed; existing assets were not touched.")
for item in manifest["captures"]:
    payload = (args.capture / item["file"]).read_bytes()
    if hashlib.sha256(payload).hexdigest() != item["sha256"]:
        raise ValueError(f"Capture hash differs: {item['file']}")
    if struct.unpack(">II", payload[16:24]) != (item["width"], item["height"]):
        raise ValueError(f"Capture dimensions differ: {item['file']}")
clock_items = [
    item for item in manifest["captures"] if item["file"].startswith("clock-")
]
density = clock_items[0]["density"]
width, height = clock_items[0]["width"], clock_items[0]["height"]
region = (int(width * 0.65), int(45 * density), int(width * 0.83), int(100 * density))
first = Image.open(args.capture / clock_items[0]["file"]).convert("RGB").crop(region)
bounds = []
for item in clock_items[1:]:
    current = Image.open(args.capture / item["file"]).convert("RGB").crop(region)
    difference = (
        ImageChops.difference(first, current)
        .convert("L")
        .point(lambda value: 255 if value > 2 else 0)
    )
    if box := difference.getbbox():
        bounds.append(box)
box = (
    region[0] + min(b[0] for b in bounds) - 2,
    region[1] + min(b[1] for b in bounds) - 2,
    region[0] + max(b[2] for b in bounds) + 2,
    region[1] + max(b[3] for b in bounds) + 2,
)
# Keep CSS coordinates integral at the native 2× density, avoiding a second
# resampling of tiny glyph patches on half-pixel element boundaries.
box = (
    box[0] // density * density,
    box[1] // density * density,
    (box[2] + density - 1) // density * density,
    (box[3] + density - 1) // density * density,
)
clock = [
    box[0] / density,
    box[1] / density,
    (box[2] - box[0]) / density,
    (box[3] - box[1]) / density,
]
(root / "reference/clock.json").write_text(json.dumps(clock) + "\n")
for item in manifest["captures"]:
    shutil.copyfile(args.capture / item["file"], root / "public/ui" / item["file"])
    if item["file"].startswith("clock-"):
        source = Image.open(args.capture / item["file"]).convert("RGBA").crop(box)
        backed = Image.alpha_composite(
            Image.new("RGBA", source.size, "black"), source
        ).convert("RGB")
        destination = root / "public/ui/clocks" / item["file"]
        destination.parent.mkdir(exist_ok=True)
        backed.save(destination)
shutil.copyfile(args.capture / "manifest.json", root / "reference/captures.json")
print(f"Imported {len(manifest['captures'])} validated native states.")
