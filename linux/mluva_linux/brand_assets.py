"""Compose repository, launcher and panel artwork from the final logo set in docs/brand.

``docs/brand`` is the source of truth (``uv run docs/brand/build.py``). This module copies the default
lockup, wordmark and glossy mark into ``docs/assets`` and composes the surfaces the logo set does not
ship: the README hero, the brand sheet, the black launcher tile and the GNOME symbolic icon.
"""

import argparse
import re
import shutil
import subprocess
from pathlib import Path

from mluva_linux.brand import LOGO_BACKGROUND, LOGO_INK_ON_DARK, LOGO_INK_ON_LIGHT, LOGO_RED, PRODUCT_NAME

ROOT = Path(__file__).parents[2]
BRAND = ROOT / "docs/brand"
ASSETS = ROOT / "docs/assets"
ICON_PATH = ROOT / "linux/resources/com.mluva.Linux.svg"
SYMBOLIC_PATH = ROOT / "linux/gnome-extension/recording-status@mluva.local/mluva-symbolic.svg"
HERO_PATH = ASSETS / "mluva-hero.svg"
FAVICON_PATH = ROOT / "docs/favicon.ico"

TAGLINE = ("Speak a rough idea.", "Shape it into useful text.")
DESCRIPTOR = "Dictation and rewriting, built for Omarchy."
TEXT_FONT = "JetBrains Mono, monospace"  # the bundled application typeface; outlines stay in the wordmark
TILE = 128
TILE_INSET = 4
TILE_RADIUS = 28
TILE_MARK = 104  # side of the framed glossy mark inside the tile; the framing keeps a 4% margin itself
SHEET_LIGHT = "#F4F4F4"


def _num(value: float) -> str:
    """Format a coordinate with at most two decimals and no trailing zeros."""
    text = f"{value:.2f}".rstrip("0").rstrip(".")
    return "0" if text in ("-0", "") else text


def _brand_svg(name: str) -> str:
    """Read one committed SVG from the logo set verbatim."""
    return (BRAND / "svg" / name).read_text(encoding="utf-8")


class Artwork:
    """One SVG from docs/brand split into its viewBox, shared defs and drawing body."""

    def __init__(self, name: str) -> None:
        """Parse the committed file; every logo-set SVG has a title, a desc and an optional defs block."""
        text = _brand_svg(name)
        self.view = tuple(float(v) for v in re.search(r'viewBox="([^"]+)"', text).group(1).split())
        defs = re.search(r"<defs>(.*)</defs>", text, re.S)
        self.defs = defs.group(1) if defs else ""
        tail = text.rsplit("</defs>", 1)[-1] if defs else text.split("</desc>", 1)[1]
        self.body = tail[: tail.rfind("</svg>")]

    @property
    def aspect(self) -> float:
        """Width divided by height of the tight viewBox."""
        return self.view[2] / self.view[3]

    def placed(self, x: float, y: float, height: float) -> str:
        """Return the body translated and scaled so its viewBox sits at (x, y) with the given height."""
        vx, vy, _, vh = self.view
        k = height / vh
        return f'<g transform="translate({_num(x - vx * k)} {_num(y - vy * k)}) scale({k:.6f})">{self.body}</g>'


def _document(width: int, height: int, desc: str, body: str, defs: str = "", title: str = PRODUCT_NAME) -> str:
    """Wrap a body in an accessible SVG root with an intrinsic pixel size."""
    defs_block = f"<defs>{defs}</defs>" if defs else ""
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" '
        'color-interpolation="sRGB" role="img" aria-labelledby="title description">'
        f'<title id="title">{title}</title><desc id="description">{desc}</desc>'
        f"{defs_block}{body}</svg>\n"
    )


def render_icon_svg() -> str:
    """Frame the glossy mark on an opaque black launcher tile."""
    mark = Artwork("mluva-mark.svg")
    offset = (TILE - TILE_MARK) / 2
    size = TILE - 2 * TILE_INSET
    body = (
        f'<rect x="{TILE_INSET}" y="{TILE_INSET}" width="{size}" height="{size}" rx="{TILE_RADIUS}" '
        f'fill="{LOGO_BACKGROUND}"/>' + mark.placed(offset, offset, TILE_MARK)
    )
    return _document(TILE, TILE, "The glossy red Mluva mark on a black launcher tile.", body, mark.defs)


def render_symbolic_svg() -> str:
    """Give native GTK and GNOME chrome the mark's silhouette in the theme's foreground color."""
    return _brand_svg("mluva-mark-symbolic.svg")


def render_hero_svg() -> str:
    """Introduce the product with the default lockup and the tagline on black."""
    lockup = Artwork("mluva-logo-on-dark.svg")
    body = (
        f'<rect width="1280" height="400" rx="24" fill="{LOGO_BACKGROUND}"/>'
        + lockup.placed(64, 56, 136)
        + f'<g font-family="{TEXT_FONT}" fill="{LOGO_INK_ON_DARK}">'
        f'<text x="64" y="262" font-size="38" font-weight="500">{TAGLINE[0]}</text>'
        f'<text x="64" y="308" font-size="38" font-weight="500">{TAGLINE[1]}</text>'
        f'<text x="64" y="362" font-size="22" fill="{LOGO_RED}">{DESCRIPTOR}</text></g>'
    )
    return _document(
        1280,
        400,
        "Open-source dictation and rewriting, built for Omarchy.",
        body,
        lockup.defs,
        title=f"{PRODUCT_NAME} — {TAGLINE[0]} {TAGLINE[1]}",
    )


def render_brand_sheet_svg() -> str:
    """Make both tones, the small-size fallbacks and the launcher tile inspectable in one artifact."""
    on_dark, on_light = Artwork("mluva-logo-on-dark.svg"), Artwork("mluva-logo-on-light.svg")
    glossy, flat = Artwork("mluva-mark.svg"), Artwork("mluva-mark-flat.svg")
    symbolic = Artwork("mluva-mark-symbolic.svg")
    panel_w, panel_h, lockup_w = 468, 230, 380
    lockup_h = lockup_w / on_dark.aspect
    lockup_y = 120 + (panel_h - lockup_h) / 2 - 12

    def panel(x: int, fill: str, lockup: Artwork) -> str:
        return f'<rect x="{x}" y="120" width="{panel_w}" height="{panel_h}" rx="16" fill="{fill}"/>' + lockup.placed(
            x + (panel_w - lockup_w) / 2, lockup_y, lockup_h
        )

    specimens = []
    x = 64
    for label, art, size in (
        ("64 px", glossy, 64),
        ("32 px", glossy, 32),
        ("24 px", glossy, 24),
        ("16 px flat", flat, 16),
        ("16 px symbolic", symbolic, 16),
    ):
        specimens.append(art.placed(x, 470 - size, size) + f'<text x="{x}" y="500" font-size="15">{label}</text>')
        x += max(size, 60) + 72
    body = (
        f'<rect width="1000" height="584" fill="{SHEET_LIGHT}"/>'
        f'<g font-family="{TEXT_FONT}" fill="{LOGO_INK_ON_LIGHT}">'
        '<text x="40" y="52" font-size="24" font-weight="600">Mluva / identity</text>'
        '<text x="40" y="85" font-size="17">A glossy red drop of speech beside the soft-M wordmark.</text>'
        + panel(24, LOGO_BACKGROUND, on_dark)
        + panel(508, "#FFFFFF", on_light)
        + f'<text x="48" y="330" font-size="14" fill="{LOGO_INK_ON_DARK}">On dark (default), #F5F5F5 ink</text>'
        '<text x="532" y="330" font-size="14">On light, #171717 ink</text>'
        '<text x="40" y="392" font-size="18" font-weight="600">One mark at small sizes</text>'
        + "".join(specimens)
        + '<text x="40" y="552" font-size="15">Glossy mark to about 24 px; flat or symbolic below. '
        "Rebuilt from docs/brand.</text></g>"
    )
    return _document(
        1000,
        584,
        "Mluva lockup on black and white, with glossy, flat and symbolic marks at small sizes.",
        body,
        on_dark.defs,
        title="Mluva identity sheet",
    )


def generated_assets() -> dict[Path, str]:
    """Keep every integration surface and reusable media variant tied to the logo set in docs/brand."""
    return {
        ICON_PATH: render_icon_svg(),
        SYMBOLIC_PATH: render_symbolic_svg(),
        HERO_PATH: render_hero_svg(),
        ASSETS / "mluva-mark.svg": _brand_svg("mluva-mark.svg"),
        ASSETS / "mluva-wordmark.svg": _brand_svg("mluva-wordmark-on-dark.svg"),
        ASSETS / "mluva-wordmark-on-light.svg": _brand_svg("mluva-wordmark-on-light.svg"),
        ASSETS / "mluva-lockup.svg": _brand_svg("mluva-logo-on-dark.svg"),
        ASSETS / "mluva-lockup-on-light.svg": _brand_svg("mluva-logo-on-light.svg"),
        ASSETS / "mluva-brand-sheet.svg": render_brand_sheet_svg(),
    }


def copied_assets() -> dict[Path, Path]:
    """Binary files the site expects beside index.html, copied unchanged from the logo set."""
    return {FAVICON_PATH: BRAND / "mluva.ico"}


def _png_zoom(svg: str) -> str:
    """Render documentation PNGs at 2x while that stays under 1024 px wide; larger artwork renders at 1x."""
    width = int(re.search(r'\bwidth="(\d+)"', svg).group(1))
    return "2" if 2 * width < 1024 else "1"


def main() -> None:
    """Regenerate vectors locally; optionally rasterize documentation variants with librsvg."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--png", action="store_true", help="Also render docs SVGs to PNG with rsvg-convert.")
    args = parser.parse_args()
    for path, svg in generated_assets().items():
        path.write_text(svg, encoding="utf-8")
        if args.png and path.parent == ASSETS:
            subprocess.run(
                ["rsvg-convert", "--zoom", _png_zoom(svg), "--output", str(path.with_suffix(".png")), str(path)],
                check=True,
            )
    for destination, source in copied_assets().items():
        shutil.copyfile(source, destination)


if __name__ == "__main__":
    main()
