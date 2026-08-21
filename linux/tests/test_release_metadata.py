"""Drift protection for cross-platform release and dependency metadata."""

import hashlib
import json
import tomllib
from pathlib import Path

REPOSITORY_ROOT = Path(__file__).parents[2]
CANONICAL_REPOSITORY_URL = "https://github.com/1vecera/Mluva"
APACHE_2_LICENSE_SHA256 = "cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30"  # pragma: allowlist secret


def test_third_party_inventory_tracks_every_resolved_dependency() -> None:
    """Keep public notices aligned with both reproducible dependency resolutions."""
    notices = (REPOSITORY_ROOT / "THIRD_PARTY_NOTICES.md").read_text(encoding="utf-8")
    with (REPOSITORY_ROOT / "linux" / "pyproject.toml").open("rb") as file:
        linux_manifest = tomllib.load(file)
    swift_resolution = json.loads((REPOSITORY_ROOT / "Package.resolved").read_text(encoding="utf-8"))

    for requirement in linux_manifest["project"]["dependencies"]:
        name, version = requirement.split("==", maxsplit=1)
        assert f"| {name} | {version} |" in notices

    for pin in swift_resolution["pins"]:
        identity = pin["identity"]
        state = pin["state"]
        version = state.get("version") or state.get("branch") or state["revision"]
        assert identity in notices
        assert version in notices


def test_release_tree_has_no_removed_shortcut_wrapper_dependency() -> None:
    """Prevent the AGPL-only convenience wrapper from re-entering the public dependency graph."""
    release_files = [
        REPOSITORY_ROOT / "linux" / "pyproject.toml",
        REPOSITORY_ROOT / "linux" / "uv.lock",
        REPOSITORY_ROOT / "THIRD_PARTY_NOTICES.md",
    ]

    for path in release_files:
        assert "global-shortcut-portal" not in path.read_text(encoding="utf-8")


def test_release_uses_exact_apache_2_license() -> None:
    """Keep the approved repository license complete and byte-for-byte canonical."""
    license_bytes = (REPOSITORY_ROOT / "LICENSE").read_bytes()

    assert hashlib.sha256(license_bytes).hexdigest() == APACHE_2_LICENSE_SHA256


def test_public_surfaces_use_canonical_repository_url() -> None:
    """Prevent public installation, security, and desktop metadata from returning to the archive URL."""
    public_surfaces = [
        REPOSITORY_ROOT / "README.md",
        REPOSITORY_ROOT / "SECURITY.md",
        REPOSITORY_ROOT / "docs" / "launch-kit.md",
        REPOSITORY_ROOT / "linux" / "gnome-extension" / "recording-status@voicescribe.local" / "metadata.json",
        REPOSITORY_ROOT / "linux" / "resources" / "voice-scribe-input@.service",
    ]
    archive_repository_url = "https://github.com/1vecera/" + "VoiceScribeMac"

    for path in public_surfaces:
        contents = path.read_text(encoding="utf-8")
        assert CANONICAL_REPOSITORY_URL in contents
        assert archive_repository_url not in contents


def test_linux_package_declares_public_license_and_urls() -> None:
    """Keep package metadata aligned with the approved public release identity."""
    with (REPOSITORY_ROOT / "linux" / "pyproject.toml").open("rb") as file:
        project = tomllib.load(file)["project"]

    assert project["license"] == "Apache-2.0"
    assert project["urls"] == {
        "Homepage": CANONICAL_REPOSITORY_URL,
        "Repository": CANONICAL_REPOSITORY_URL,
        "Issues": f"{CANONICAL_REPOSITORY_URL}/issues",
    }
