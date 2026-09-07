"""Semantic theme parsing without reading or changing the user's desktop."""

from pathlib import Path

from voice_scribe_linux.theme import DarkTokens, read_omarchy_palette


def test_theme_maps_light_and_dark_palettes_without_foreign_brand_colors(tmp_path: Path) -> None:
    """Keep every CSS token usable across complete Omarchy palettes and optional roles."""
    palette = tmp_path / "colors.toml"
    for mode, background, foreground in (("dark", "#1a1b26", "#c0caf5"), ("light", "#faf4ed", "#575279")):
        palette.write_text(
            f'mode="{mode}"\nbackground="{background}"\nforeground="{foreground}"\naccent="#286983"\nred="#b4637a"'
        )
        colors, dark = read_omarchy_palette(palette)
        assert set(colors) == set(DarkTokens)
        assert colors["canvas"] == colors["surface"] == background
        assert colors["ink"] == foreground and colors["action"] == "#286983"
        assert colors["danger"] == "#b4637a" and dark == (mode == "dark")
        assert all(len(value) == 7 and value.startswith("#") for value in colors.values())


def test_missing_malformed_or_injected_palette_falls_back(tmp_path: Path) -> None:
    """Treat incomplete theme updates and arbitrary CSS as unusable input."""
    palette = tmp_path / "colors.toml"
    assert read_omarchy_palette(palette) is None
    for text in ("broken = [", 'background="#000000"', 'background="red; }"\nforeground="#ffffff"\naccent="#abcdef"'):
        palette.write_text(text)
        assert read_omarchy_palette(palette) is None
