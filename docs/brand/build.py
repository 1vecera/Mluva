# /// script
# requires-python = ">=3.11"
# dependencies = ["svgpathtools>=1.6", "pillow>=10"]
# ///
"""Build the Mluva logo asset set from the committed sources in docs/brand/source.

Run from anywhere: ``uv run docs/brand/build.py``. Needs ``rsvg-convert`` (librsvg) and
``magick`` (ImageMagick 7) on PATH for the PNG, ICO and preview outputs.

Sources
- glossy-red-blob.svg: the vector mark (reusable gradients, masks, one soft blur).
- glossy-red-blob.png: the 1254 px raster master of the same mark; all mark PNGs and the .ico come from it.
- figma-wordmark-outlines.svg: "l", "uva" outlines and the M placement box from the Figma export.
- m-glyph-trace.svg: potrace outline of the raster M that the Figma export embedded.
"""

from __future__ import annotations

import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image
from svgpathtools import CubicBezier, Line, QuadraticBezier, parse_path
from svgpathtools import Path as SvgPath

BRAND = Path(__file__).resolve().parent
SOURCE = BRAND / "source"
SVG_DIR = BRAND / "svg"
PNG_DIR = BRAND / "png"

INK_ON_LIGHT = "#171717"  # Figma wordmark ink
INK_ON_DARK = "#F5F5F5"
FLAT_RED = "#E91B27"  # mean colour of the rendered glossy mark

# Mark height divided by M height, and the gap between them as a share of the mark height.
LOCKUPS = {
    "": (1.05, 0.30),  # default: the mark reads like a letter; 5% optical compensation for the round shape
    "medium-mark": (1.50, 0.28),  # icon-plus-name lockup
    "large-mark": (2.25, 0.25),  # mark-dominant, for about screens and cards
}
ICON_MARGIN = 0.04  # share of the icon canvas left free on each side of the mark
MARK_PNG_SIZES = (16, 32, 48, 64, 96, 128, 192, 256, 512, 1024)
ICO_SIZES = (16, 24, 32, 48, 64, 128, 256)
LOCKUP_PNG_WIDTH = 1200
PNG_OPTS = ("-depth", "8", "-strip", "-define", "png:compression-level=9")  # Q16 magick would write 16-bit PNGs


def num(value: float) -> str:
    """Format a coordinate with at most two decimals and no trailing zeros."""
    text = f"{value:.2f}".rstrip("0").rstrip(".")
    return "0" if text in ("-0", "") else text


def path_d(path: SvgPath, dx: float = 0.0, dy: float = 0.0) -> str:
    """Emit absolute commands with two decimals; svgpathtools' own writer is verbose."""
    shift = complex(dx, dy)
    parts: list[str] = []
    for sub in path.continuous_subpaths():
        start = sub[0].start + shift
        parts.append(f"M{num(start.real)} {num(start.imag)}")
        for seg in sub:
            if isinstance(seg, Line):
                e = seg.end + shift
                parts.append(f"L{num(e.real)} {num(e.imag)}")
            elif isinstance(seg, CubicBezier):
                c1, c2, e = seg.control1 + shift, seg.control2 + shift, seg.end + shift
                parts.append(
                    f"C{num(c1.real)} {num(c1.imag)} {num(c2.real)} {num(c2.imag)} {num(e.real)} {num(e.imag)}"
                )
            elif isinstance(seg, QuadraticBezier):
                c, e = seg.control + shift, seg.end + shift
                parts.append(f"Q{num(c.real)} {num(c.imag)} {num(e.real)} {num(e.imag)}")
            else:
                raise TypeError(f"unsupported segment {type(seg).__name__}")
        if sub.isclosed():
            parts.append("Z")
    return "".join(parts)


class Blob:
    """The glossy red mark: its defs, drawing group, silhouette and visual bounding box (1254 px space)."""

    def __init__(self) -> None:
        """Split the source SVG into reusable parts and measure the silhouette."""
        text = (SOURCE / "glossy-red-blob.svg").read_text()
        self.defs = re.search(r"<defs>(.*)</defs>", text, re.S).group(1)
        self.body = re.search(r'(<g id="glossy-red-blob".*</g>)</svg>', text, re.S).group(1)
        self.shape = re.search(r'<path id="s" d="([^"]+)"', text).group(1)
        xmin, xmax, ymin, ymax = parse_path(self.shape).bbox()
        self.xmin, self.ymin, self.w, self.h = xmin, ymin, xmax - xmin, ymax - ymin

    def icon_box(self) -> tuple[float, float, float]:
        """Square canvas around the mark with ICON_MARGIN free on each side: (x, y, side)."""
        side = max(self.w, self.h) / (1 - 2 * ICON_MARGIN)
        cx, cy = self.xmin + self.w / 2, self.ymin + self.h / 2
        return cx - side / 2, cy - side / 2, side

    def placed(self, x: float, y: float, height: float) -> str:
        """Return the glossy group translated so its visual box starts at (x, y) with the given height."""
        k = height / self.h
        tx, ty = x - self.xmin * k, y - self.ymin * k
        return f'<g transform="translate({tx:.4f} {ty:.4f}) scale({k:.6f})">{self.body}</g>'


class Wordmark:
    """'Mluva' in wordmark coordinates: M top-left at (0, 0), baseline at ``self.baseline``."""

    def __init__(self) -> None:
        """Place the traced M exactly where the Figma export placed its raster, then normalise to the origin."""
        outlines = (SOURCE / "figma-wordmark-outlines.svg").read_text()
        box = re.search(r'<rect id="m-image-box" x="([^"]+)" y="([^"]+)" width="([^"]+)" height="([^"]+)"', outlines)
        bx, by, bw, bh = (float(v) for v in box.groups())
        l_rect = re.search(r'<rect id="l" x="([^"]+)" y="([^"]+)" width="([^"]+)" height="([^"]+)"', outlines)
        lx, ly, lw, lh = (float(v) for v in l_rect.groups())
        uva = parse_path(re.search(r'<path id="uva" d="([^"]+)"', outlines).group(1))

        trace = (SOURCE / "m-glyph-trace.svg").read_text()
        size = float(re.search(r'viewBox="0 0 ([\d.]+)', trace).group(1))
        tr = re.search(r'transform="translate\(([^,]+),([^)]+)\) scale\(([^,]+),([^)]+)\)"', trace)
        ttx, tty, tsx, tsy = (float(v) for v in tr.groups())
        m = parse_path(re.search(r'<path d="([^"]+)"', trace, re.S).group(1))
        m = m.scaled(tsx, tsy).translated(complex(ttx, tty))  # potrace units -> image pixels
        m = m.scaled(bw / size, bh / size).translated(complex(bx, by))  # image pixels -> Figma design units

        mx0, mx1, my0, my1 = m.bbox()
        ux0, ux1, uy0, uy1 = uva.bbox()
        self.width = ux1 - mx0
        self.height = max(uy1, ly + lh) - my0
        self.m_width, self.m_height = mx1 - mx0, my1 - my0
        self.baseline = ly + lh - my0
        self.m_center_y = self.m_height / 2
        self.m_d = path_d(m, -mx0, -my0)
        self.uva_d = path_d(uva, -mx0, -my0)
        self.l = (lx - mx0, ly - my0, lw, lh)

    def m_only(self, color: str, dx: float = 0.0, dy: float = 0.0) -> str:
        """Return the M monogram path."""
        return f'<path fill="{color}" transform="translate({num(dx)} {num(dy)})" d="{self.m_d}"/>'

    def group(self, color: str, dx: float = 0.0, dy: float = 0.0) -> str:
        """Return the full wordmark group (M, l, uva) in one fill colour."""
        lx, ly, lw, lh = self.l
        return (
            f'<g fill="{color}" transform="translate({num(dx)} {num(dy)})">'
            f'<path d="{self.m_d}"/>'
            f'<rect x="{num(lx)}" y="{num(ly)}" width="{num(lw)}" height="{num(lh)}"/>'
            f'<path d="{self.uva_d}"/></g>'
        )


def svg_document(
    view: tuple[float, float, float, float], display_height: int, desc: str, body: str, defs: str = ""
) -> str:
    """Wrap a body in an accessible SVG root with a tight viewBox and an intrinsic display size."""
    vx, vy, vw, vh = view
    display_width = round(display_height * vw / vh)
    defs_block = f"<defs>{defs}</defs>" if defs else ""
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{display_width}" height="{display_height}" '
        f'viewBox="{num(vx)} {num(vy)} {num(vw)} {num(vh)}" color-interpolation="sRGB" '
        f'role="img" aria-labelledby="title description">'
        f'<title id="title">Mluva</title><desc id="description">{desc}</desc>'
        f"{defs_block}{body}</svg>\n"
    )


def build_svgs(blob: Blob, word: Wordmark) -> dict[str, str]:
    """Compose every SVG variant: three lockup ratios, wordmark, M monogram and three marks."""
    out: dict[str, str] = {}
    margin = 0.02
    tones = (("on-light", INK_ON_LIGHT), ("on-dark", INK_ON_DARK))

    # Horizontal lockups: glossy mark left, wordmark right, both centred on the M.
    for name, (ratio, gap_ratio) in LOCKUPS.items():
        mark_h = ratio * word.m_height
        mark_w = blob.w * mark_h / blob.h
        gap = gap_ratio * mark_h
        mark_top = word.m_center_y - mark_h / 2
        top = min(mark_top, 0.0)
        bottom = max(mark_top + mark_h, word.height)
        pad = margin * (bottom - top)
        width = mark_w + gap + word.width + 2 * pad
        height = bottom - top + 2 * pad
        mark = blob.placed(pad, mark_top - top + pad, mark_h)
        for tone, ink in tones:
            body = mark + word.group(ink, pad + mark_w + gap, pad - top)
            desc = (
                f"Mluva logo: glossy red mark at {ratio:g} times the M height, wordmark to the right, "
                f"for {tone.replace('-', ' ')} backgrounds."
            )
            file = f"mluva-logo{'-' + name if name else ''}-{tone}.svg"
            out[file] = svg_document((0, 0, width, height), 128, desc, body, blob.defs)

    # Wordmark and M monogram.
    pad = margin * word.height
    for tone, ink in tones:
        out[f"mluva-wordmark-{tone}.svg"] = svg_document(
            (-pad, -pad, word.width + 2 * pad, word.height + 2 * pad),
            64,
            f"Mluva wordmark with the soft M, for {tone.replace('-', ' ')} backgrounds.",
            word.group(ink),
        )
        side = max(word.m_width, word.m_height) / (1 - 2 * ICON_MARGIN)
        out[f"mluva-m-{tone}.svg"] = svg_document(
            ((word.m_width - side) / 2, (word.m_height - side) / 2, side, side),
            128,
            f"Mluva M monogram, for {tone.replace('-', ' ')} backgrounds.",
            word.m_only(ink),
        )

    # Marks: glossy, flat red, symbolic (theme foreground colour).
    ix, iy, side = blob.icon_box()
    out["mluva-mark.svg"] = svg_document(
        (ix, iy, side, side), 512, "Mluva glossy red mark, square icon framing.", blob.body, blob.defs
    )
    out["mluva-mark-flat.svg"] = svg_document(
        (ix, iy, side, side), 128, "Mluva mark as one flat red shape.", f'<path fill="{FLAT_RED}" d="{blob.shape}"/>'
    )
    out["mluva-mark-symbolic.svg"] = svg_document(
        (ix, iy, side, side),
        128,
        "Mluva mark silhouette in the current foreground colour.",
        f'<path fill="currentColor" d="{blob.shape}"/>',
    )
    return out


def run(*cmd: str) -> None:
    """Run a command and fail loudly."""
    subprocess.run(cmd, check=True, stdout=subprocess.DEVNULL)


def render_png(svg: Path, png: Path, *, width: int | None = None, height: int | None = None) -> None:
    """Render an SVG with librsvg, the renderer GTK uses."""
    size = ["-w", str(width)] if width else ["-h", str(height)]
    run("rsvg-convert", *size, str(svg), "-o", str(png))


def build_rasters(blob: Blob, svgs: dict[str, str], tmp: Path) -> None:
    """Render lockup PNGs from the SVGs; cut mark PNGs and the .ico from the raster master."""
    for name in svgs:
        if name.startswith(("mluva-logo", "mluva-wordmark")):
            render_png(SVG_DIR / name, PNG_DIR / name.replace(".svg", ".png"), width=LOCKUP_PNG_WIDTH)
    for name in ("mluva-m-on-light.svg", "mluva-m-on-dark.svg"):
        render_png(SVG_DIR / name, PNG_DIR / name.replace(".svg", ".png"), height=512)

    # Same framing as mluva-mark.svg, so vector and raster icons line up.
    ix, iy, side = blob.icon_box()
    crop = f"{round(side)}x{round(side)}+{round(ix)}+{round(iy)}"
    master = SOURCE / "glossy-red-blob.png"

    def resized(size: int, target: Path) -> None:
        run(
            "magick", str(master), "-crop", crop, "+repage", "-filter", "Lanczos",
            "-resize", f"{size}x{size}", *PNG_OPTS, str(target),
        )  # fmt: skip

    for size in MARK_PNG_SIZES:
        resized(size, PNG_DIR / f"mluva-mark-{size}.png")

    frames = []
    for size in ICO_SIZES:
        part = tmp / f"ico-{size}.png"
        resized(size, part)
        frames.append(Image.open(part).convert("RGBA"))
    # Pillow stores every ICO entry PNG-compressed, roughly a third of the raw BMP size. The base image must be
    # the largest one: Pillow silently drops requested sizes bigger than the base frame.
    largest = frames.pop()
    largest.save(BRAND / "mluva.ico", format="ICO", sizes=[(s, s) for s in ICO_SIZES], append_images=frames)


def build_preview(tmp: Path) -> None:
    """Compose preview.png: on-light assets on a light panel beside on-dark assets on a black panel."""
    column_w, inner_w, gap = 760, 640, 36

    def column(tone: str, bg: str) -> Path:
        pieces = [
            PNG_DIR / f"mluva-logo-{tone}.png",
            PNG_DIR / f"mluva-logo-medium-mark-{tone}.png",
            PNG_DIR / f"mluva-logo-large-mark-{tone}.png",
            PNG_DIR / f"mluva-wordmark-{tone}.png",
        ]
        m_small = tmp / f"m-{tone}.png"
        run("magick", str(PNG_DIR / f"mluva-m-{tone}.png"), "-resize", "128x128", str(m_small))
        icons = tmp / f"icons-{tone}.png"
        run(
            "magick", "-background", "none", str(m_small),
            *[str(PNG_DIR / f"mluva-mark-{s}.png") for s in (128, 64, 32, 16)],
            "-gravity", "South", "+append", str(icons),
        )  # fmt: skip
        pieces.append(icons)
        heights = []
        for piece in pieces:
            info = subprocess.run(
                ["magick", "identify", "-format", "%w %h", str(piece)], check=True, capture_output=True, text=True
            ).stdout.split()
            w, h = int(info[0]), int(info[1])
            heights.append(round(h * inner_w / w) if w > inner_w else h)
        total_h = sum(heights) + gap * (len(pieces) + 1)
        cmd = ["magick", "-size", f"{column_w}x{total_h}", f"xc:{bg}"]
        y = gap
        for piece, h in zip(pieces, heights, strict=True):
            cmd += [
                "(", str(piece), "-resize", f"{inner_w}x{h}>", ")",
                "-gravity", "NorthWest", "-geometry", f"+{(column_w - inner_w) // 2}+{y}", "-composite",
            ]  # fmt: skip
            y += h + gap
        out = tmp / f"column-{tone}.png"
        run(*cmd, str(out))
        return out

    light = column("on-light", "#F4F4F4")
    dark = column("on-dark", "#000000")  # black is the default background when one is needed
    run("magick", str(light), str(dark), "-background", "none", "+append", *PNG_OPTS, str(BRAND / "preview.png"))


def main() -> int:
    """Build all assets and print the SVG sizes and wordmark metrics."""
    for tool in ("rsvg-convert", "magick"):
        if shutil.which(tool) is None:
            print(f"missing tool: {tool}", file=sys.stderr)
            return 1
    blob, word = Blob(), Wordmark()
    for directory in (SVG_DIR, PNG_DIR):  # regenerate from scratch so renamed variants leave no stale files
        directory.mkdir(exist_ok=True)
        for stale in directory.iterdir():
            stale.unlink()
    svgs = build_svgs(blob, word)
    for name, text in svgs.items():
        (SVG_DIR / name).write_text(text)
    with tempfile.TemporaryDirectory() as tmp_name:
        tmp = Path(tmp_name)
        build_rasters(blob, svgs, tmp)
        build_preview(tmp)
    for name in sorted(svgs):
        print(f"{(SVG_DIR / name).stat().st_size:>7} B  svg/{name}")
    print(f"{(BRAND / 'mluva.ico').stat().st_size:>7} B  mluva.ico")
    print(f"M height {word.m_height:.2f}, wordmark {word.width:.2f} x {word.height:.2f}, baseline {word.baseline:.2f}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
