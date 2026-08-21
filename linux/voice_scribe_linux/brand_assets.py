"""Generate committed Mluva brand assets from the canonical visual tokens."""

from pathlib import Path

from voice_scribe_linux.brand import BRAND_ACTION, BRAND_HIGHLIGHT, BRAND_INK, PRODUCT_NAME

ICON_PATH = Path(__file__).parents[1] / "resources" / "com.voicescribe.Linux.svg"


def render_icon_svg() -> str:
    """Render the app icon as one speech trace that resolves into an M."""
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128" role="img" aria-labelledby="title">
  <title id="title">{PRODUCT_NAME}</title>
  <rect x="4" y="4" width="120" height="120" rx="28" fill="{BRAND_INK}"/>
  <path
    d="M20 84H30V43L64 73L98 43V84H108"
    fill="none"
    stroke="{BRAND_ACTION}"
    stroke-width="10"
    stroke-linecap="round"
    stroke-linejoin="round"
  />
  <circle cx="105" cy="27" r="6" fill="{BRAND_HIGHLIGHT}"/>
</svg>
"""


def main() -> None:
    """Write the generated icon to its committed resource path."""
    ICON_PATH.write_text(render_icon_svg(), encoding="utf-8")


if __name__ == "__main__":
    main()
