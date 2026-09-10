"""Recognize a restrained Markdown subset without rewriting or executing its source."""

import re
from collections import defaultdict, deque
from dataclasses import dataclass

_FENCE = re.compile(r"^ {0,3}(`{3,}|~{3,})(.*)$")
_HEADING = re.compile(r"^ {0,3}(#{1,6})[ \t]+(?=\S)")
_QUOTE = re.compile(r"^ {0,3}>[ \t]?")
_LIST = re.compile(r"^([ \t]*)(?:[-+*]|\d+[.)])[ \t]+(?=\S)")
_ESCAPABLE = frozenset("\\`*_{}[]()#+.!>~-")
MAX_WORD_WRAP_RUN = 1024


@dataclass(frozen=True)
class MarkdownSpan:
    """Describe character offsets in the unchanged Markdown source."""

    start: int
    end: int
    style: str


@dataclass(frozen=True)
class _Emphasis:
    """Keep one opener on both the nesting stack and its delimiter's lookup stack."""

    marker: str
    start: int
    end: int


def needs_character_wrapping(source: str) -> bool:
    """Avoid Pango's costly repeated word search across enormous unbreakable tokens."""
    run = paragraph_length = 0
    has_word_joiner = False
    for character in source:
        if character in "\r\n\u2028\u2029":
            run = paragraph_length = 0
            has_word_joiner = False
            continue
        paragraph_length += 1
        has_word_joiner = has_word_joiner or character in "\u2060\ufeff"
        if has_word_joiner and paragraph_length > MAX_WORD_WRAP_RUN:
            return True
        # Treat unusual Unicode spacing conservatively. Some whitespace (for
        # example FIGURE SPACE) glues words instead of offering a line break.
        if character in " \t":
            run = 0
        else:
            run += 1
            if run > MAX_WORD_WRAP_RUN:
                return True
    return False


def _inline_spans(text: str, offset: int) -> list[MarkdownSpan]:
    """Consume each delimiter run once; unmatched input never retries shorter prefixes."""
    ticks: dict[int, deque[tuple[int, int]]] = defaultdict(deque)
    index = 0
    while index < len(text):
        start = text.find("`", index)
        if start < 0:
            break
        index = start + 1
        while index < len(text) and text[index] == "`":
            index += 1
        ticks[index - start].append((start, index))

    spans: list[MarkdownSpan] = []
    openers: list[_Emphasis] = []
    matching: dict[str, list[_Emphasis]] = defaultdict(list)

    def pair(start: int, begin: int, end: int, stop: int, *styles: str) -> None:
        """Hide matched delimiters and tag their unchanged source body."""
        spans.append(MarkdownSpan(offset + start, offset + begin, "syntax"))
        spans.extend(MarkdownSpan(offset + begin, offset + end, style) for style in styles)
        spans.append(MarkdownSpan(offset + end, offset + stop, "syntax"))

    index = 0
    while index < len(text):
        character = text[index]
        if character == "\\" and index + 1 < len(text) and text[index + 1] in _ESCAPABLE:
            spans.append(MarkdownSpan(offset + index, offset + index + 1, "syntax"))
            index += 2
            continue
        if character not in "`*_":
            index += 1
            continue
        end = index + 1
        while end < len(text) and text[end] == character:
            end += 1
        length = end - index
        if character == "`":
            candidates = ticks[length]
            while candidates and candidates[0][0] <= index:
                candidates.popleft()
            if candidates:
                close, stop = candidates.popleft()
                pair(index, end, close, stop, "code")
                index = stop
            else:
                index = end
            continue
        # Longer emphasis runs are outside this deliberately small subset.
        if length > 3:
            index = end
            continue
        marker = character * length
        before = text[index - 1] if index else " "
        after = text[end] if end < len(text) else " "
        can_open = not after.isspace()
        can_close = not before.isspace()
        if length == 1:
            can_open = can_open and not (before.isalnum() or before == "_")
            can_close = can_close and not (after.isalnum() or after == "_")
        if can_close and matching[marker]:
            opener = matching[marker][-1]
            # A closer can abandon unmatched inner openers, but each is popped
            # only once across the entire scan; there is no backward searching.
            while openers:
                removed = openers.pop()
                matching[removed.marker].pop()
                if removed is opener:
                    break
            styles = ("strong", "em") if length == 3 else ("strong",) if length == 2 else ("em",)
            pair(opener.start, opener.end, index, end, *styles)
        elif can_open:
            opener = _Emphasis(marker, index, end)
            openers.append(opener)
            matching[marker].append(opener)
        index = end
    return spans


def markdown_spans(source: str) -> tuple[MarkdownSpan, ...]:
    """Style headings, emphasis, lists, quotes and code; leave unsupported syntax literal."""
    spans: list[MarkdownSpan] = []

    offset = 0
    fence = ""
    for line in source.splitlines(keepends=True):
        content = line.rstrip("\r\n\u2028\u2029")
        marker = _FENCE.match(content)
        if fence:
            if marker and marker[1][0] == fence[0] and len(marker[1]) >= len(fence) and not marker[2].strip():
                spans.append(MarkdownSpan(offset, offset + len(content), "syntax"))
                fence = ""
            else:
                spans.append(MarkdownSpan(offset, offset + len(content), "code"))
        elif marker:
            fence = marker[1]
            spans.append(MarkdownSpan(offset, offset + len(line), "syntax"))
        else:
            start = 0
            if heading := _HEADING.match(content):
                start = heading.end()
                spans.append(MarkdownSpan(offset, offset + start, "syntax"))
                spans.append(MarkdownSpan(offset + start, offset + len(content), f"h{len(heading[1])}"))
            elif quote := _QUOTE.match(content):
                start = quote.end()
                spans.append(MarkdownSpan(offset, offset + start, "syntax"))
                spans.append(MarkdownSpan(offset + start, offset + len(content), "quote"))
            elif _LIST.match(content):
                spans.append(MarkdownSpan(offset, offset + len(content), "list"))
            spans.extend(_inline_spans(content[start:], offset + start))
        offset += len(line)
    return tuple(spans)


def visible_markdown(source: str, spans: tuple[MarkdownSpan, ...]) -> tuple[str, tuple[MarkdownSpan, ...]]:
    """Project reading text and style offsets for measurement, retaining source elsewhere."""
    hidden = bytearray(len(source))
    for span in spans:
        if span.style == "syntax":
            hidden[span.start : span.end] = b"\1" * (span.end - span.start)
    offsets = [0]
    for index in range(len(source)):
        offsets.append(offsets[-1] + (not hidden[index]))
    visible = "".join(character for index, character in enumerate(source) if not hidden[index])
    styles = tuple(
        MarkdownSpan(offsets[span.start], offsets[span.end], span.style)
        for span in spans
        if span.style != "syntax" and offsets[span.start] != offsets[span.end]
    )
    return visible, styles
