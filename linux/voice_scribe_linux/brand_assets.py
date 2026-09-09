"""Render flat production artwork from the selected GPT concept and software wordmark."""

import argparse
import subprocess
from pathlib import Path
from xml.etree import ElementTree

from voice_scribe_linux.brand import LOGO_ACCENT, LOGO_FOREGROUND, LOGO_INK, PRODUCT_NAME

ROOT = Path(__file__).parents[2]
ASSETS = ROOT / "docs/assets"
ICON_PATH = ROOT / "linux/resources/com.voicescribe.Linux.svg"
SYMBOLIC_PATH = ROOT / "linux/gnome-extension/recording-status@voicescribe.local/mluva-symbolic.svg"
HERO_PATH = ASSETS / "mluva-hero.svg"
WORDMARK_SOURCE = ASSETS / "brand-source/wordmark-outline.svg"
# A flat optical redraw of the selected flowing-m silhouette: 20-unit strokes, 16-unit gaps.
MARK_PATH = (
    "M8 90V44a28 28 0 0 1 56 0v28a8 8 0 0 0 16 0V44a28 28 0 0 1 56 0v46"
    "a10 10 0 0 1-20 0V44a8 8 0 0 0-16 0v28a28 28 0 0 1-56 0V44"
    "a8 8 0 0 0-16 0v46a10 10 0 0 1-20 0Z"
)


def _wordmark_path() -> str:
    """Reuse committed font outlines so asset regeneration never depends on a viewer's fonts."""
    source = ElementTree.parse(WORDMARK_SOURCE).getroot()
    path = source.find("{http://www.w3.org/2000/svg}path")
    assert path is not None
    return path.attrib["d"]


def render_mark_svg(color: str = LOGO_ACCENT) -> str:
    """Keep the isolated mark flat and transparent at both toolbar and campaign sizes."""
    return f'''<svg xmlns="http://www.w3.org/2000/svg" width="144" height="116" viewBox="0 0 144 116"
  role="img" aria-labelledby="title description">
  <title id="title">{PRODUCT_NAME}</title>
  <desc id="description">A continuous voice wave forming a lowercase m, with open, even spacing.</desc>
  <path fill="{color}" d="{MARK_PATH}"/>
</svg>
'''


def render_icon_svg() -> str:
    """Frame the production silhouette in an opaque app tile without glow or shadows."""
    return f'''<svg xmlns="http://www.w3.org/2000/svg" width="128" height="128" viewBox="0 0 128 128"
  role="img" aria-labelledby="title description">
  <title id="title">{PRODUCT_NAME}</title>
  <desc id="description">A frost-blue voice wave on a slate app tile.</desc>
  <rect x="4" y="4" width="120" height="120" rx="28" fill="{LOGO_INK}"/>
  <path transform="translate(6.4 17.6) scale(.8)" fill="{LOGO_ACCENT}" d="{MARK_PATH}"/>
</svg>
'''


def render_symbolic_svg() -> str:
    """Give native GTK and GNOME chrome the same mark in the theme's foreground color."""
    return render_mark_svg("currentColor")


def render_wordmark_svg(color: str = LOGO_FOREGROUND) -> str:
    """Set the exact product name with real typography, delivered as portable vector paths."""
    return f'''<svg xmlns="http://www.w3.org/2000/svg" width="288" height="96" viewBox="0 0 288 96"
  role="img" aria-labelledby="title description">
  <title id="title">{PRODUCT_NAME}</title>
  <desc id="description">Mluva wordmark in outlined Adwaita Sans SemiBold.</desc>
  <path transform="translate(16 12.578)" fill="{color}" d="{_wordmark_path()}"/>
</svg>
'''


def _lockup(ink: str, accent: str) -> str:
    """Optically align the wave with the typeset wordmark and retain a clear separation."""
    return f'''<path transform="translate(9 13.25) scale(.875)" fill="{accent}" d="{MARK_PATH}"/>
  <path transform="translate(152 28.578)" fill="{ink}" d="{_wordmark_path()}"/>'''


def render_lockup_svg(*, on_light: bool = False) -> str:
    """Offer a frost/light lockup for dark surfaces and a solid ink version for light ones."""
    ink = LOGO_INK if on_light else LOGO_FOREGROUND
    accent = LOGO_INK if on_light else LOGO_ACCENT
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="424" height="128" viewBox="0 0 424 128"
  role="img" aria-labelledby="title description">
  <title id="title">{PRODUCT_NAME}</title>
  <desc id="description">Mluva voice-wave mark and software-typeset wordmark.</desc>
  {_lockup(ink, accent)}
</svg>
"""


def render_hero_svg() -> str:
    """Introduce the product with an exact wordmark, useful copy and the shared flat mark."""
    return f'''<svg xmlns="http://www.w3.org/2000/svg" width="1280" height="400" viewBox="0 0 1280 400"
  role="img" aria-labelledby="title description">
  <title id="title">Mluva — Speak a rough idea. Shape it into useful text.</title>
  <desc id="description">Open-source dictation and rewriting, built for Omarchy.</desc>
  <rect width="1280" height="400" rx="24" fill="{LOGO_INK}"/>
  <path transform="translate(64 64) scale(1.5)" fill="{LOGO_FOREGROUND}" d="{_wordmark_path()}"/>
  <path transform="translate(886 60) scale(2.25)" fill="{LOGO_ACCENT}" d="{MARK_PATH}"/>
  <g font-family="Adwaita Sans, Inter, Noto Sans, sans-serif" fill="{LOGO_FOREGROUND}">
    <text x="64" y="248" font-size="38" font-weight="500">Speak a rough idea.</text>
    <text x="64" y="294" font-size="38" font-weight="500">Shape it into useful text.</text>
    <text x="64" y="353" font-size="22" fill="{LOGO_ACCENT}">Dictation and rewriting, built for Omarchy.</text>
  </g>
</svg>
'''


def render_brand_sheet_svg() -> str:
    """Make light/dark contrast, small sizes and the font outlines inspectable in one artifact."""
    specimens = []
    x = 64
    for size in (16, 24, 32, 64):
        scale = size / 144
        specimens.append(
            f'<path transform="translate({x} {464 - 100 * scale}) scale({scale})" '
            f'fill="{LOGO_INK}" d="{MARK_PATH}"/>'
            f'<text x="{x}" y="500" font-size="15">{size} px</text>'
        )
        x += size + 56
    return f'''<svg xmlns="http://www.w3.org/2000/svg" width="1000" height="584" viewBox="0 0 1000 584"
  role="img" aria-labelledby="title description">
  <title id="title">Mluva production identity draft</title>
  <desc id="description">Mluva mark and wordmark on light/dark backgrounds, with small-size specimens.</desc>
  <rect width="1000" height="584" fill="#ECEFF4"/>
  <g font-family="Adwaita Sans, Inter, Noto Sans, sans-serif" fill="{LOGO_INK}">
    <text x="40" y="52" font-size="24" font-weight="600">Mluva / production identity</text>
    <text x="40" y="85" font-size="17">A voice wave, refined into a flat mark. Exact software typography.</text>
    <rect x="24" y="120" width="468" height="230" rx="16" fill="{LOGO_INK}"/>
    <rect x="508" y="120" width="468" height="230" rx="16" fill="#FFFFFF"/>
    <g transform="translate(46 165)">{_lockup(LOGO_FOREGROUND, LOGO_ACCENT)}</g>
    <g transform="translate(530 165)">{_lockup(LOGO_INK, LOGO_INK)}</g>
    <text x="48" y="330" font-size="14" fill="{LOGO_FOREGROUND}">Frost + snow on slate</text>
    <text x="532" y="330" font-size="14">Solid ink on white</text>
    <text x="40" y="392" font-size="18" font-weight="600">One silhouette at small sizes</text>
    {"".join(specimens)}
    <text x="40" y="552" font-size="15">Flat vectors. Open spacing. Software typography.</text>
  </g>
</svg>
'''


def generated_assets() -> dict[Path, str]:
    """Keep every integration and reusable media variant tied to the same two source shapes."""
    return {
        ICON_PATH: render_icon_svg(),
        SYMBOLIC_PATH: render_symbolic_svg(),
        HERO_PATH: render_hero_svg(),
        ASSETS / "mluva-mark.svg": render_mark_svg(),
        ASSETS / "mluva-mark-on-light.svg": render_mark_svg(LOGO_INK),
        ASSETS / "mluva-wordmark.svg": render_wordmark_svg(),
        ASSETS / "mluva-wordmark-on-light.svg": render_wordmark_svg(LOGO_INK),
        ASSETS / "mluva-lockup.svg": render_lockup_svg(),
        ASSETS / "mluva-lockup-on-light.svg": render_lockup_svg(on_light=True),
        ASSETS / "mluva-brand-sheet.svg": render_brand_sheet_svg(),
    }


def main() -> None:
    """Regenerate vectors locally; optionally rasterize documentation variants with librsvg."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--png", action="store_true", help="Also render docs SVGs to 2x PNG with rsvg-convert.")
    args = parser.parse_args()
    for path, svg in generated_assets().items():
        path.write_text(svg, encoding="utf-8")
        if args.png and path.parent == ASSETS:
            subprocess.run(
                ["rsvg-convert", "--zoom", "2", "--output", str(path.with_suffix(".png")), str(path)], check=True
            )


if __name__ == "__main__":
    main()
