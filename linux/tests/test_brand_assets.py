"""Drift protection for generated Mluva brand assets."""

import plistlib
import tomllib
from pathlib import Path

from mluva_linux.brand import PRODUCT_VERSION
from mluva_linux.brand_assets import (
    SYMBOLIC_PATH,
    generated_assets,
)
from mluva_linux.theme import build_shell_stylesheet

REPOSITORY_ROOT = Path(__file__).parents[2]


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
    for path, expected in generated_assets().items():
        assert path.read_text(encoding="utf-8") == expected, path
    assert (SYMBOLIC_PATH.parent / "stylesheet.css").read_text(encoding="utf-8") == build_shell_stylesheet()
