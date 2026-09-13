"""Rebuild the custom Mluva lettering from the bundled JetBrains Mono Medium outlines.

Run with: uv run --with fonttools python dev/sculpt_wordmark.py
The narrowed l and optical spacing belong to this wordmark, not to a modified font.
"""

import hashlib
import json
from pathlib import Path

from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont

root = Path(__file__).resolve().parents[1]
font_path = root / "linux/quickshell/mluva.dictation/fonts/JetBrainsMono-Medium.ttf"
font = TTFont(font_path)
glyphs = font.getGlyphSet()
mapping = font.getBestCmap()
pen = SVGPathPen(glyphs)
scale = 0.112
# Five 600-unit cells, tightened optically. The l keeps its unmistakable mono foot.
positions = (0, 51.5, 95, 148, 199)
for character, x in zip("Mluva", positions, strict=True):
    width_scale = 0.87 if character == "l" else 1.0
    glyphs[mapping[ord(character)]].draw(
        TransformPen(pen, (scale * width_scale, 0, 0, -scale, x, 80))
    )
path = pen.getCommands()
source = root / "docs/assets/brand-source"
(source / "wordmark-outline.svg").write_text(
    '<svg xmlns="http://www.w3.org/2000/svg" width="272" height="84" viewBox="0 0 272 84">\n'
    "  <title>Mluva</title>\n"
    "  <desc>JetBrains Mono Medium outlines with a narrowed l and optical letter spacing.</desc>\n"
    f'  <path d="{path}"/>\n</svg>\n'
)
(source / "wordmark-provenance.json").write_text(
    json.dumps(
        {
            "text": "Mluva",
            "font": "JetBrains Mono Medium",
            "font_sha256": hashlib.sha256(font_path.read_bytes()).hexdigest(),
            "upstream_commit": "19371302b95d218af43299bce79ddbddd0bc364d",
            "source": "wordmark-outline.svg",
            "generator": "dev/sculpt_wordmark.py",
            "license": "../../../linux/quickshell/mluva.dictation/fonts/OFL.txt",
            "refinement": "Optical spacing and an 87% width l; original contours retained in all other letters.",
            "production": "Editable paths, transparent background; application text uses the unchanged bundled font.",
        },
        indent=2,
    )
    + "\n"
)
