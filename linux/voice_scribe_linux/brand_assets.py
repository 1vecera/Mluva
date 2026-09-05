"""Generate committed Mluva brand assets from the canonical visual tokens."""

from pathlib import Path

from voice_scribe_linux.brand import BRAND_ACTION, BRAND_HIGHLIGHT, PRODUCT_NAME

ICON_PATH = Path(__file__).parents[1] / "resources" / "com.voicescribe.Linux.svg"
SYMBOLIC_PATH = Path(__file__).parents[1] / "gnome-extension/recording-status@voicescribe.local/mluva-symbolic.svg"
MARK_PATH = "M30 84V52C30 34 62 34 62 52V76M62 52C62 34 94 34 94 52V76C94 88 86 94 78 98"


def render_icon_svg() -> str:
    """Render a rounded lowercase m whose trailing stroke suggests speech."""
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128" role="img" aria-labelledby="title">
  <title id="title">{PRODUCT_NAME}</title>
  <desc>A rounded m with two open arches and a soft speech tail.</desc>
  <rect x="4" y="4" width="120" height="120" rx="30" fill="{BRAND_ACTION}"/>
  <path
    d="{MARK_PATH}"
    fill="none"
    stroke="{BRAND_HIGHLIGHT}"
    stroke-width="12"
    stroke-linecap="round"
    stroke-linejoin="round"
  />
</svg>
"""


def render_symbolic_svg() -> str:
    """Use the same geometry for a single-color panel mark without an enclosing tile."""
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="16 24 96 88">
  <title>{PRODUCT_NAME}</title>
  <path d="{MARK_PATH}" fill="none" stroke="currentColor" stroke-width="12"
    stroke-linecap="round" stroke-linejoin="round"/>
</svg>
'''


def main() -> None:
    """Write the generated icon to its committed resource path."""
    ICON_PATH.write_text(render_icon_svg(), encoding="utf-8")
    SYMBOLIC_PATH.write_text(render_symbolic_svg(), encoding="utf-8")


if __name__ == "__main__":
    main()
