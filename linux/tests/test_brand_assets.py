"""Drift protection for generated Mluva brand assets."""

import plistlib
import tomllib
from pathlib import Path

from voice_scribe_linux.brand import PRODUCT_DESCRIPTOR, PRODUCT_NAME, PRODUCT_VERSION
from voice_scribe_linux.brand_assets import ICON_PATH, render_icon_svg

REPOSITORY_ROOT = Path(__file__).parents[2]


def test_brand_identity_is_canonical() -> None:
    """Keep the reviewed public name and descriptor explicit."""
    assert PRODUCT_NAME == "Mluva"
    assert PRODUCT_DESCRIPTOR == "Open-source AI dictation for Linux, with a source preview for macOS."


def test_release_version_matches_every_platform_manifest() -> None:
    """Keep the public release version aligned across the monorepo."""
    with (REPOSITORY_ROOT / "linux" / "pyproject.toml").open("rb") as file:
        linux_manifest = tomllib.load(file)
    with (REPOSITORY_ROOT / "Resources" / "Info.plist").open("rb") as file:
        macos_manifest = plistlib.load(file)

    assert linux_manifest["project"]["version"] == PRODUCT_VERSION
    assert macos_manifest["CFBundleShortVersionString"] == PRODUCT_VERSION


def test_committed_icon_matches_the_token_generated_asset() -> None:
    """Prevent a hand-edited icon from drifting away from the product tokens."""
    assert ICON_PATH.read_text(encoding="utf-8") == render_icon_svg()
