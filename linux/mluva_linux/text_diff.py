"""Bounded word diffs for visual revisions without changing recognition or Markdown source."""

import re
from dataclasses import dataclass
from difflib import SequenceMatcher

_WORDS = re.compile(r"\s+|\w+|[^\w\s]+")


@dataclass(frozen=True, slots=True)
class TextChange:
    """Describe one changed range in the previous and current strings."""

    old_start: int
    old_end: int
    new_start: int
    new_end: int


def text_changes(previous: str, current: str) -> tuple[TextChange, ...]:
    """Preserve unchanged words between corrections, with a bounded fallback for large documents."""
    if previous == current:
        return ()
    prefix = 0
    for before, after in zip(previous, current, strict=False):
        if before != after:
            break
        prefix += 1
    suffix = 0
    limit = min(len(previous), len(current)) - prefix
    while suffix < limit and previous[-suffix - 1] == current[-suffix - 1]:
        suffix += 1
    old_end, new_end = len(previous) - suffix, len(current) - suffix
    before = list(_WORDS.finditer(previous, prefix, old_end))
    after = list(_WORDS.finditer(current, prefix, new_end))
    if len(before) + len(after) > 2000:
        return (TextChange(prefix, old_end, prefix, new_end),)
    old_offsets = [match.start() for match in before] + [old_end]
    new_offsets = [match.start() for match in after] + [new_end]
    matcher = SequenceMatcher(None, [m.group() for m in before], [m.group() for m in after], autojunk=False)
    return tuple(
        TextChange(old_offsets[a], old_offsets[b], new_offsets[c], new_offsets[d])
        for operation, a, b, c, d in matcher.get_opcodes()
        if operation != "equal"
    )
