"""Generate committed Mluva brand assets from the canonical visual tokens."""

from pathlib import Path

from voice_scribe_linux.brand import BRAND_INK, BRAND_SIGNAL, PRODUCT_NAME

ICON_PATH = Path(__file__).parents[1] / "resources" / "com.voicescribe.Linux.svg"
SYMBOLIC_PATH = Path(__file__).parents[1] / "gnome-extension/recording-status@voicescribe.local/mluva-symbolic.svg"
MARK_PATH = "M36 34H92Q102 34 102 44V76Q102 86 92 86H61L42 100V86H36Q26 86 26 76V44Q26 34 36 34Z"
WAVE_PATH = "M46 55V65M64 46V74M82 55V65"
HERO_PATH = Path(__file__).parents[2] / "docs/assets/mluva-hero.svg"


def render_icon_svg() -> str:
    """Frame a three-bar voice signal in a precise, open speech mark."""
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128" role="img" aria-labelledby="title">
  <title id="title">{PRODUCT_NAME}</title>
  <desc>A voice signal inside a speech bubble, in mint on deep green.</desc>
  <rect x="4" y="4" width="120" height="120" rx="28" fill="{BRAND_INK}"/>
  <path d="{MARK_PATH}" fill="{BRAND_SIGNAL}"/>
  <path d="{WAVE_PATH}" fill="none" stroke="{BRAND_INK}" stroke-width="7" stroke-linecap="round"/>
</svg>
"""


def render_symbolic_svg() -> str:
    """Use the same geometry for a single-color panel mark without an enclosing tile."""
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="20 28 88 80">
  <title>{PRODUCT_NAME}</title>
  <path d="{MARK_PATH}" fill="none" stroke="currentColor" stroke-width="6"
    stroke-linecap="round" stroke-linejoin="round"/>
  <path d="{WAVE_PATH}" fill="none" stroke="currentColor" stroke-width="7" stroke-linecap="round"/>
</svg>
'''


def render_hero_svg() -> str:
    """Keep the repository's visual introduction consistent with the installed icon."""
    return f'''<svg xmlns="http://www.w3.org/2000/svg" width="1280" height="360" viewBox="0 0 1280 360"
  role="img" aria-labelledby="title description">
  <title id="title">Mluva — Speak freely. Write clearly.</title>
  <desc id="description">Open-source dictation and rewriting for Linux.</desc>
  <rect width="1280" height="360" rx="24" fill="{BRAND_INK}"/>
  <g fill="none" stroke="{BRAND_SIGNAL}" opacity="0.10">
    <circle cx="1080" cy="180" r="250"/><circle cx="1080" cy="180" r="190"/>
  </g>
  <g transform="translate(930 48) scale(2.05)">
    <path d="{MARK_PATH}" fill="{BRAND_SIGNAL}"/>
    <path d="{WAVE_PATH}" fill="none" stroke="{BRAND_INK}" stroke-width="7" stroke-linecap="round"/>
  </g>
  <g font-family="Inter, Adwaita Sans, Noto Sans, sans-serif">
    <text x="56" y="65" font-size="25" font-weight="650" fill="{BRAND_SIGNAL}">Mluva</text>
    <text x="56" y="156" font-size="60" font-weight="650" letter-spacing="-2" fill="#FFFFFF">Speak freely.</text>
    <text x="56" y="224" font-size="60" font-weight="650" letter-spacing="-2" fill="#FFFFFF">Write clearly.</text>
    <text x="58" y="299" font-size="22" fill="{BRAND_SIGNAL}">Open-source dictation and rewriting for Linux.</text>
  </g>
</svg>
'''


def main() -> None:
    """Write the generated icon to its committed resource path."""
    ICON_PATH.write_text(render_icon_svg(), encoding="utf-8")
    SYMBOLIC_PATH.write_text(render_symbolic_svg(), encoding="utf-8")
    HERO_PATH.write_text(render_hero_svg(), encoding="utf-8")


if __name__ == "__main__":
    main()
