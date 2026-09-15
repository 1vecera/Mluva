"""Contract coverage for configurable, honest public feature labels."""

import pytest

from mluva_linux.feature_maturity import (
    FEATURE_CAPABILITIES,
    feature_capability,
    maturity_description,
    maturity_title,
)


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
