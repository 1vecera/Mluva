"""Deterministic spoken-structure normalization with immutable raw input."""

import re

_PARAGRAPH_MARKER = "\u0000mluva-paragraph\u0000"
_LINE_MARKER = "\u0000mluva-line\u0000"
_SCRATCH_MARKER = "\u0000mluva-scratch\u0000"

_STRUCTURAL_COMMANDS = (
    (re.compile(r"(?i)\bnew\s+paragraph\b"), _PARAGRAPH_MARKER),
    (re.compile(r"(?i)\bnew\s+line\b"), _LINE_MARKER),
    (re.compile(r"(?i)\bscratch\s+that\b"), _SCRATCH_MARKER),
)

_PUNCTUATION_COMMANDS = (
    (re.compile(r"(?i)\bquestion\s+mark\b"), "?"),
    (re.compile(r"(?i)\bexclamation\s+(?:mark|point)\b"), "!"),
    (re.compile(r"(?i)\bsemi[ -]?colon\b"), ";"),
    (re.compile(r"(?i)\bfull\s+stop\b"), "."),
    (re.compile(r"(?i)\bperiod\b"), "."),
    (re.compile(r"(?i)\bcomma\b"), ","),
    (re.compile(r"(?i)\bcolon\b"), ":"),
)


def normalize_spoken_structure(raw_text: str) -> str:
    """Apply explicit spoken commands without mutating or hiding the raw recognition."""
    prepared = " ".join(raw_text.split())
    for pattern, replacement in _STRUCTURAL_COMMANDS:
        prepared = pattern.sub(f" {replacement} ", prepared)
    for pattern, replacement in _PUNCTUATION_COMMANDS:
        prepared = pattern.sub(replacement, prepared)
    prepared = _apply_scratch_commands(prepared)
    prepared = re.sub(r"\s+([,.;:?!])", r"\1", prepared)
    prepared = re.sub(r"([,;:])(?=\S)", r"\1 ", prepared)
    prepared = re.sub(r"([.?!])(?=[A-Za-z0-9])", r"\1 ", prepared)
    prepared = re.sub(r"[ \t]*" + re.escape(_PARAGRAPH_MARKER) + r"[ \t]*", "\n\n", prepared)
    prepared = re.sub(r"[ \t]*" + re.escape(_LINE_MARKER) + r"[ \t]*", "\n", prepared)
    prepared = re.sub(r"[ \t]+\n", "\n", prepared)
    prepared = re.sub(r"\n[ \t]+", "\n", prepared)
    prepared = re.sub(r"\n{3,}", "\n\n", prepared)
    prepared = re.sub(r"[ \t]{2,}", " ", prepared)
    return prepared.strip()


def _apply_scratch_commands(text: str) -> str:
    """Remove the current uncommitted clause whenever an explicit scratch command occurs."""
    while _SCRATCH_MARKER in text:
        prefix, suffix = text.split(_SCRATCH_MARKER, maxsplit=1)
        punctuation_boundary = max(prefix.rfind("."), prefix.rfind("?"), prefix.rfind("!"))
        line_boundary = max(prefix.rfind(_LINE_MARKER), prefix.rfind(_PARAGRAPH_MARKER))
        if line_boundary > punctuation_boundary:
            marker = _LINE_MARKER if prefix.startswith(_LINE_MARKER, line_boundary) else _PARAGRAPH_MARKER
            retained_prefix = prefix[: line_boundary + len(marker)]
        else:
            retained_prefix = prefix[: punctuation_boundary + 1] if punctuation_boundary >= 0 else ""
        text = f"{retained_prefix} {suffix}"
    return text
