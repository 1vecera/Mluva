"""Derive bounded review-only vocabulary suggestions from explicit history edits."""

from __future__ import annotations

import re
import unicodedata
from collections.abc import Iterable, Set
from dataclasses import dataclass

from voice_scribe_linux.history import HistoryEntry
from voice_scribe_linux.personalization import DictionaryReplacement

MAX_CORRECTION_CHARACTERS = 80
MAX_CORRECTION_WORDS = 4
MAX_SUGGESTION_HISTORY_ENTRIES = 1_000
_LEADING_TOKEN_BOUNDARIES = frozenset('"“”‘’([{')
_TRAILING_TOKEN_BOUNDARIES = frozenset('"“”‘’)]},.!?;:')
_WORD_PATTERN = re.compile(r"\S+")


@dataclass(frozen=True, slots=True)
class VocabularySuggestion:
    """Describe one small correction the user may explicitly add or dismiss."""

    identifier: str
    spoken: str
    written: str
    application_identifier: str | None
    occurrences: int


@dataclass(frozen=True, slots=True)
class _Word:
    """Retain one trimmed token and its exact phrase boundaries."""

    text: str
    start: int
    end: int


class VocabularySuggestionEngine:
    """Match the macOS correction heuristic without learning automatically."""

    def suggestions(
        self,
        entries: Iterable[HistoryEntry],
        dictionary: Iterable[DictionaryReplacement] = (),
        dismissed_identifiers: Set[str] = frozenset(),
    ) -> tuple[VocabularySuggestion, ...]:
        """Group focused explicit corrections, then exclude already reviewed phrases."""
        grouped: dict[str, VocabularySuggestion] = {}
        for entry in entries:
            if entry.correction_source_text is None:
                continue
            correction = _focused_correction(entry.correction_source_text, entry.delivered_text)
            if correction is None:
                continue
            spoken, written = correction
            identifier = _suggestion_identifier(spoken, written, entry.application_identifier)
            if identifier in dismissed_identifiers:
                continue
            existing = grouped.get(identifier)
            grouped[identifier] = VocabularySuggestion(
                identifier=identifier,
                spoken=spoken,
                written=written,
                application_identifier=entry.application_identifier,
                occurrences=1 if existing is None else existing.occurrences + 1,
            )

        replacements = tuple(dictionary)
        visible = [
            suggestion for suggestion in grouped.values() if not _covered_by_dictionary(suggestion, replacements)
        ]
        return tuple(
            sorted(
                visible,
                key=lambda suggestion: (
                    -suggestion.occurrences,
                    suggestion.spoken.casefold(),
                    suggestion.written.casefold(),
                    suggestion.identifier,
                ),
            )
        )


def _focused_correction(source: str, corrected: str) -> tuple[str, str] | None:
    """Return the single changed phrase only when the edit is locally bounded."""
    source_words = _words(source)
    corrected_words = _words(corrected)
    if not source_words or not corrected_words:
        return None

    prefix_count = 0
    while (
        prefix_count < min(len(source_words), len(corrected_words))
        and source_words[prefix_count].text == corrected_words[prefix_count].text
    ):
        prefix_count += 1

    suffix_count = 0
    while (
        suffix_count < len(source_words) - prefix_count
        and suffix_count < len(corrected_words) - prefix_count
        and source_words[-suffix_count - 1].text == corrected_words[-suffix_count - 1].text
    ):
        suffix_count += 1

    source_end = len(source_words) - suffix_count
    corrected_end = len(corrected_words) - suffix_count
    source_changed = source_words[prefix_count:source_end]
    corrected_changed = corrected_words[prefix_count:corrected_end]
    if (
        not source_changed
        or not corrected_changed
        or len(source_changed) > MAX_CORRECTION_WORDS
        or len(corrected_changed) > MAX_CORRECTION_WORDS
    ):
        return None

    shared_word_count = prefix_count + suffix_count
    if shared_word_count == 0 and max(len(source_words), len(corrected_words)) > 3:
        return None

    spoken = _phrase(source, source_changed)
    written = _phrase(corrected, corrected_changed)
    if spoken == written or len(spoken) > MAX_CORRECTION_CHARACTERS or len(written) > MAX_CORRECTION_CHARACTERS:
        return None
    return spoken, written


def _words(text: str) -> tuple[_Word, ...]:
    """Split non-whitespace tokens while excluding sentence-edge punctuation."""
    words: list[_Word] = []
    for match in _WORD_PATTERN.finditer(text):
        start, end = match.span()
        while start < end and text[start] in _LEADING_TOKEN_BOUNDARIES:
            start += 1
        while start < end and text[end - 1] in _TRAILING_TOKEN_BOUNDARIES:
            end -= 1
        if start < end:
            words.append(_Word(text[start:end], start, end))
    return tuple(words)


def _phrase(text: str, words: tuple[_Word, ...]) -> str:
    """Recover the exact contiguous source phrase covered by changed tokens."""
    return text[words[0].start : words[-1].end]


def _suggestion_identifier(spoken: str, written: str, application_identifier: str | None) -> str:
    """Build the same cross-platform review identity used by the macOS app."""
    return "\x1f".join(
        (
            application_identifier or "",
            _normalized_phrase(spoken),
            unicodedata.normalize("NFC", written),
        )
    )


def _normalized_phrase(value: str) -> str:
    """Fold case and diacritics for cross-platform phrase comparison."""
    decomposed = unicodedata.normalize("NFD", value.casefold())
    return "".join(character for character in decomposed if not unicodedata.combining(character))


def _covered_by_dictionary(
    suggestion: VocabularySuggestion,
    dictionary: tuple[DictionaryReplacement, ...],
) -> bool:
    """Exclude a phrase when a global or applicable scoped rule already owns it."""
    normalized_spoken = _normalized_phrase(suggestion.spoken)
    for replacement in dictionary:
        if _normalized_phrase(replacement.spoken) != normalized_spoken:
            continue
        if suggestion.application_identifier is None:
            return replacement.application_identifier is None
        if replacement.application_identifier in (None, suggestion.application_identifier):
            return True
    return False
