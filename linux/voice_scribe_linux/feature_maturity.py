"""Canonical human-acceptance labels for public Mluva capabilities."""

from dataclasses import dataclass
from enum import StrEnum
from typing import Final


class FeatureMaturity(StrEnum):
    """Describe whether one public capability passed the current manual acceptance boundary."""

    VERIFIED = "verified"
    EXPERIMENTAL = "experimental"

    @property
    def label(self) -> str:
        """Return the concise label shown in the application and public matrix."""
        if self is FeatureMaturity.VERIFIED:
            return "Verified on Linux"
        return "Experimental"


@dataclass(frozen=True, slots=True)
class FeatureCapability:
    """Keep one feature's label, maturity, and honest acceptance boundary together."""

    identifier: str
    title: str
    summary: str
    maturity: FeatureMaturity


FEATURE_CAPABILITIES: Final[tuple[FeatureCapability, ...]] = (
    FeatureCapability(
        "dictation",
        "Recording and transcription",
        "F9 and the visible record control start, stop, and finalize ordinary dictation.",
        FeatureMaturity.VERIFIED,
    ),
    FeatureCapability(
        "recording_controls",
        "Recording setup controls",
        "Language, microphone, function key, and recording behavior can be adjusted before capture.",
        FeatureMaturity.VERIFIED,
    ),
    FeatureCapability(
        "history",
        "History",
        "Completed captures remain visible and recoverable in the local History surface.",
        FeatureMaturity.VERIFIED,
    ),
    FeatureCapability(
        "saved_styles",
        "Custom saved styles",
        "A user-created writing style can be selected and applied to dictated text.",
        FeatureMaturity.VERIFIED,
    ),
    FeatureCapability(
        "automatic_paste",
        "Automatic paste",
        (
            "Known limitation: insertion is disabled by default and is not reliable in the current Fedora "
            "acceptance setup; completed text remains on the clipboard."
        ),
        FeatureMaturity.EXPERIMENTAL,
    ),
    FeatureCapability(
        "faithful_cleanup",
        "Faithful cleanup",
        "Optional Codex cleanup preserves the raw transcript and falls back safely, but still needs manual acceptance.",
        FeatureMaturity.EXPERIMENTAL,
    ),
    FeatureCapability(
        "spoken_structure",
        "Spoken structure",
        "Punctuation, paragraph, and scratch-that commands still need manual acceptance.",
        FeatureMaturity.EXPERIMENTAL,
    ),
    FeatureCapability(
        "command_mode",
        "Command mode",
        "Spoken editing instructions are previewed before delivery and still need manual acceptance.",
        FeatureMaturity.EXPERIMENTAL,
    ),
    FeatureCapability(
        "notes_mode",
        "Notes mode",
        "Longer acceptance-gated drafts and their relaunch recovery still need manual acceptance.",
        FeatureMaturity.EXPERIMENTAL,
    ),
    FeatureCapability(
        "meeting_mode",
        "Meeting mode",
        "Microphone plus system-audio capture, diarization, review, and archive still need manual acceptance.",
        FeatureMaturity.EXPERIMENTAL,
    ),
    FeatureCapability(
        "desktop_overlay",
        "Desktop recording bar",
        (
            "The optional display-only GNOME Shell overlay has off-screen evidence but still needs "
            "live-desktop acceptance."
        ),
        FeatureMaturity.EXPERIMENTAL,
    ),
    FeatureCapability(
        "dictionary_suggestions",
        "Dictionary and vocabulary suggestions",
        "Local replacements and review-only suggestions still need manual acceptance.",
        FeatureMaturity.EXPERIMENTAL,
    ),
    FeatureCapability(
        "snippets",
        "Snippets",
        "Explicit spoken expansions and portable typed-trigger storage still need manual acceptance.",
        FeatureMaturity.EXPERIMENTAL,
    ),
    FeatureCapability(
        "application_memory",
        "Per-application memory and context",
        "Remembered modes, styles, rules, and bounded context controls still need manual acceptance.",
        FeatureMaturity.EXPERIMENTAL,
    ),
    FeatureCapability(
        "recovery_privacy",
        "Advanced recovery, retention, and Incognito",
        "Retries, reprocessing, exports, retention policies, and Incognito still need manual acceptance.",
        FeatureMaturity.EXPERIMENTAL,
    ),
    FeatureCapability(
        "diagnostics",
        "Diagnostics export",
        "Privacy-safe timing diagnostics still need manual acceptance.",
        FeatureMaturity.EXPERIMENTAL,
    ),
    FeatureCapability(
        "macos_preview",
        "macOS source preview",
        "The source and deterministic tests are available, but the platform has no current accepted release binary.",
        FeatureMaturity.EXPERIMENTAL,
    ),
)

FEATURES_BY_IDENTIFIER: Final[dict[str, FeatureCapability]] = {
    capability.identifier: capability for capability in FEATURE_CAPABILITIES
}
if len(FEATURES_BY_IDENTIFIER) != len(FEATURE_CAPABILITIES):
    raise RuntimeError("Feature capability identifiers must be unique")


def feature_capability(identifier: str) -> FeatureCapability:
    """Return one required capability or fail on an unregistered public surface."""
    try:
        return FEATURES_BY_IDENTIFIER[identifier]
    except KeyError as error:
        raise ValueError(f"Unknown feature capability: {identifier}") from error


def capabilities_with_maturity(maturity: FeatureMaturity) -> tuple[FeatureCapability, ...]:
    """Return capabilities in the reviewed display order for one maturity level."""
    return tuple(capability for capability in FEATURE_CAPABILITIES if capability.maturity is maturity)


def maturity_description(identifier: str, description: str) -> str:
    """Prefix experimental control copy while leaving verified descriptions concise."""
    capability = feature_capability(identifier)
    if capability.maturity is FeatureMaturity.EXPERIMENTAL:
        return f"Experimental — {description}"
    return description


def maturity_title(identifier: str, title: str | None = None) -> str:
    """Append a visible Experimental label while keeping verified titles quiet."""
    capability = feature_capability(identifier)
    visible_title = title or capability.title
    if capability.maturity is FeatureMaturity.EXPERIMENTAL:
        return f"{visible_title} · {capability.maturity.label}"
    return visible_title


def render_feature_maturity_markdown() -> str:
    """Render the checked-in public feature matrix from the application registry."""
    lines = [
        "<!-- Generated by `make linux-feature-maturity`; do not edit by hand. -->",
        "",
        "# Feature maturity",
        "",
        (
            "This matrix records the current manual acceptance on Fedora GNOME. **Verified on Linux** means "
            "the capability worked in that acceptance pass. **Experimental** means it is available for testing but "
            "has not yet earned that claim; automated tests and off-screen evidence do not promote a feature by "
            "themselves."
        ),
        "",
        (
            "Automatic paste is an explicit known limitation: it is not reliable in the current Fedora acceptance "
            "setup and is disabled by default. Mluva keeps completed text recoverable on the clipboard instead of "
            "claiming delivery succeeded."
        ),
        "",
    ]
    for maturity, heading in (
        (FeatureMaturity.VERIFIED, "Verified on Linux"),
        (FeatureMaturity.EXPERIMENTAL, "Experimental"),
    ):
        lines.extend((f"## {heading}", "", "| Feature | Current boundary |", "| --- | --- |"))
        lines.extend(
            f"| {capability.title} | {capability.summary} |" for capability in capabilities_with_maturity(maturity)
        )
        lines.append("")
    return "\n".join(lines)
