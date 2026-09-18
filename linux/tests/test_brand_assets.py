"""Drift protection for generated Mluva brand assets."""

import tomllib
from pathlib import Path

from mluva_linux.brand import PRODUCT_VERSION
from mluva_linux.brand_assets import SYMBOLIC_PATH, copied_assets, generated_assets
from mluva_linux.theme import build_shell_stylesheet

REPOSITORY_ROOT = Path(__file__).parents[2]


def test_release_version_matches_package_manifest() -> None:
    """Keep the runtime and package release versions aligned."""
    with (REPOSITORY_ROOT / "linux" / "pyproject.toml").open("rb") as file:
        linux_manifest = tomllib.load(file)

    assert linux_manifest["project"]["version"] == PRODUCT_VERSION


def test_committed_icon_matches_the_token_generated_asset() -> None:
    """Prevent a hand-edited icon or a stale copy from drifting away from the logo set in docs/brand."""
    for path, expected in generated_assets().items():
        assert path.read_text(encoding="utf-8") == expected, path
    for destination, source in copied_assets().items():
        assert destination.read_bytes() == source.read_bytes(), destination
    assert (SYMBOLIC_PATH.parent / "stylesheet.css").read_text(encoding="utf-8") == build_shell_stylesheet()
