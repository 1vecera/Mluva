"""Contract coverage for configurable, honest public feature labels."""

from pathlib import Path

import pytest

from voice_scribe_linux.feature_maturity import (
    FEATURE_CAPABILITIES,
    FeatureMaturity,
    capabilities_with_maturity,
    feature_capability,
    maturity_description,
    maturity_title,
    render_feature_maturity_markdown,
)

REPOSITORY_ROOT = Path(__file__).parents[2]


def test_manual_acceptance_baseline_is_explicit_and_conservative() -> None:
    """Keep only the stated working set above the experimental boundary."""
    verified = {capability.identifier for capability in capabilities_with_maturity(FeatureMaturity.VERIFIED)}

    assert verified == {"dictation", "recording_controls", "history", "saved_styles"}
    assert feature_capability("automatic_paste").maturity is FeatureMaturity.EXPERIMENTAL
    assert "not reliable" in feature_capability("automatic_paste").summary
    assert all(capability.maturity is FeatureMaturity.EXPERIMENTAL for capability in FEATURE_CAPABILITIES[4:])


def test_capability_registry_is_unique_and_fails_closed() -> None:
    """Make new UI surfaces name a reviewed capability rather than inventing labels."""
    identifiers = [capability.identifier for capability in FEATURE_CAPABILITIES]

    assert len(identifiers) == len(set(identifiers))
    with pytest.raises(ValueError, match="Unknown feature capability"):
        feature_capability("unreviewed-surface")


def test_experimental_descriptions_are_labeled_in_place() -> None:
    """Put the maturity boundary beside the control, not only in release notes."""
    assert maturity_description("command_mode", "Review before delivery") == ("Experimental — Review before delivery")
    assert maturity_description("dictation", "Press F9 to record") == "Press F9 to record"
    assert maturity_title("automatic_paste") == "Automatic paste · Experimental"
    assert maturity_title("saved_styles") == "Custom saved styles"


def test_public_feature_matrix_is_generated_from_the_application_registry() -> None:
    """Prevent UI maturity and public copy from drifting into different claims."""
    generated = (REPOSITORY_ROOT / "docs" / "feature-maturity.md").read_text(encoding="utf-8")

    assert generated == render_feature_maturity_markdown()
    assert "Automatic paste is an explicit known limitation" in generated
    for capability in FEATURE_CAPABILITIES:
        assert f"| {capability.title} | {capability.summary} |" in generated
