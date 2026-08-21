"""Deterministic coverage for spoken structure and correction commands."""

from voice_scribe_linux.transcript import normalize_spoken_structure


def test_spoken_punctuation_lines_and_paragraphs() -> None:
    """Convert explicit structural phrases while preserving normal word boundaries."""
    raw = "Hello comma world period new paragraph Next line colon new line item one period"
    assert normalize_spoken_structure(raw) == "Hello, world.\n\nNext line:\nitem one."


def test_scratch_that_removes_only_current_uncommitted_clause() -> None:
    """Keep committed sentences and replace the clause after the latest boundary."""
    raw = "First sentence period unwanted second thought scratch that replacement thought period"
    assert normalize_spoken_structure(raw) == "First sentence. replacement thought."


def test_scratch_that_without_boundary_discards_the_current_utterance_prefix() -> None:
    """Treat an initial false start as uncommitted text."""
    assert normalize_spoken_structure("wrong opening scratch that correct opening") == "correct opening"


def test_scratch_that_preserves_a_structural_boundary() -> None:
    """Keep the requested paragraph while dropping only its current false start."""
    raw = "Heading period new paragraph wrong body scratch that corrected body period"
    assert normalize_spoken_structure(raw) == "Heading.\n\ncorrected body."


def test_normalization_is_stable_and_does_not_mutate_raw_input() -> None:
    """Return a stable derived value while the caller keeps immutable recognition."""
    raw = "Path slash tmp slash file comma do not change"
    normalized = normalize_spoken_structure(raw)
    assert raw == "Path slash tmp slash file comma do not change"
    assert normalized == "Path slash tmp slash file, do not change"
    assert normalize_spoken_structure(normalized) == normalized
