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
            return "Verified on Omarchy"
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
        "conversations",
        "Rewrite conversations",
        (
            "Originals, Quick Polish, Structured Note, follow-ups and conversation export "
            "form the daily Omarchy workspace."
        ),
        FeatureMaturity.VERIFIED,
    ),
    FeatureCapability(
        "editable_documents",
        "Editable documents and automatic copy",
        "Originals and rewrites can be edited, saved and copied automatically in the Omarchy workflow.",
        FeatureMaturity.VERIFIED,
    ),
    FeatureCapability(
        "provider_choice",
        "Independent rewrite and speech providers",
        "LiteLLM transports and local Voxtype have controlled checks; cloud account compatibility needs acceptance.",
        FeatureMaturity.EXPERIMENTAL,
    ),
    FeatureCapability(
        "live_rewrite",
        "Live structured rewriting",
        "Opt-in task, note, polish and custom drafts expose missing information; model quality needs acceptance.",
        FeatureMaturity.EXPERIMENTAL,
    ),
    FeatureCapability(
        "automatic_paste",
        "Automatic paste",
        (
            "Known limitation: insertion is disabled by default and depends on the target application and "
            "desktop; clipboard delivery is the standard workflow."
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
        "Shell menu and recording bar",
        (
            "The optional GNOME Shell menu and display-only bottom bar have off-screen evidence but still need "
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
        "omarchy_widget",
        "Omarchy recording widget",
        "The themed widget, completed-note rewrites and opening the workspace are used daily on Omarchy.",
        FeatureMaturity.VERIFIED,
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
            "Omarchy is the primary platform: the core dictation, rewrite and widget workflow is tested end to end "
            "and used daily by the maintainer. **Verified on Omarchy** identifies that working set. "
            "**Experimental** features remain available with the limits below. Fedora GNOME compatibility "
            "is retained, but has not been tested for several releases."
        ),
        "",
        (
            "Automatic paste is an explicit known limitation: it depends on the desktop and target application "
            "and is disabled by default. Clipboard delivery is the standard workflow. Automated tests use "
            "isolated sessions; platform acceptance comes from use on the actual desktop."
        ),
        "",
    ]
    for maturity, heading in (
        (FeatureMaturity.VERIFIED, "Verified on Omarchy"),
        (FeatureMaturity.EXPERIMENTAL, "Experimental"),
    ):
        lines.extend((f"## {heading}", "", "| Feature | Current boundary |", "| --- | --- |"))
        lines.extend(
            f"| {capability.title} | {capability.summary} |" for capability in capabilities_with_maturity(maturity)
        )
        lines.append("")
    return "\n".join(lines)
