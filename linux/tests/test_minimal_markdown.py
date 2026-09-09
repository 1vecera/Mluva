"""Keep formatted reading conservative and the authoritative Markdown source intact."""

import subprocess
import sys
from pathlib import Path

import pytest

from voice_scribe_linux.minimal_markdown import markdown_spans, needs_character_wrapping, visible_markdown


@pytest.mark.parametrize(
    ("source", "visible"),
    [
        ("# Žluťoučký kůň\n**Strong** and *quiet*.\n", "Žluťoučký kůň\nStrong and quiet.\n"),
        ("**Keep _this_ meaning**", "Keep this meaning"),
        ("***Both styles*** and **outer *inner* tail**", "Both styles and outer inner tail"),
        (r"Keep \*literal\* and snake_case_identifiers.", "Keep *literal* and snake_case_identifiers."),
        ("`**literal**` and **emphasis**", "**literal** and emphasis"),
        ("Use ``a ` b`` and `a `` b`.", "Use a ` b and a `` b."),
        (r"Use \`literal\` and `a\` tail.", "Use `literal` and a\\ tail."),
        ("Keep `unclosed and ``mismatched` runs.", "Keep unclosed and ``mismatched runs."),
        ("```python\n# literal\n**unchanged**\n```\n", "# literal\n**unchanged**\n\n"),
        ("> A quiet quote\n- First\n2. Second\n", "A quiet quote\n- First\n2. Second\n"),
        ("A *partial emphasis and **unfinished", "A *partial emphasis and **unfinished"),
        (
            "<script>text</script> ![remote](https://example.invalid/a)",
            "<script>text</script> ![remote](https://example.invalid/a)",
        ),
    ],
)
def test_only_recognized_markdown_syntax_is_hidden(source: str, visible: str) -> None:
    """Unicode offsets, code, literal input and partial streams remain safe and lossless."""
    spans = markdown_spans(source)
    projected, styles = visible_markdown(source, spans)
    assert projected == visible
    assert all(0 <= span.start < span.end <= len(source) for span in spans)
    assert all(0 <= span.start < span.end <= len(visible) for span in styles)


def test_different_or_shorter_fences_do_not_end_literal_code() -> None:
    """Embedded Markdown is never interpreted before the matching closing code fence."""
    source = "````\n```\n# still code\n~~~~\n**still code**\n````"
    visible, spans = visible_markdown(source, markdown_spans(source))
    assert visible == "```\n# still code\n~~~~\n**still code**\n"
    assert {span.style for span in spans} == {"code"}


@pytest.mark.parametrize("separator", ("\n", "\r\n", "\r", "\u2028", "\u2029"))
def test_block_styles_exclude_paragraph_separators(separator: str) -> None:
    """Match native paragraph boundaries without rewriting the original line endings."""
    source = separator.join(("# First", "> A quote", "  - A list", "```", "literal", "```", ""))
    spans = markdown_spans(source)
    assert all(not source[span.start : span.end].endswith(separator) for span in spans if span.style != "syntax")
    visible, styles = visible_markdown(source, spans)
    assert visible == separator.join(("First", "A quote", "  - A list", "literal", "", ""))
    assert all(0 <= span.start < span.end <= len(visible) for span in styles)


def test_character_wrapping_only_changes_unbreakable_runs() -> None:
    """Keep ordinary long prose word-wrapped, including words in other alphabets."""
    assert not needs_character_wrapping("Žluťoučký kůň jumps over a quiet sentence.\n" * 3000)
    assert not needs_character_wrapping("x" * 1024 + "\n" + "y" * 1024)
    assert needs_character_wrapping("`" * 1025)
    assert needs_character_wrapping("word\u00a0" * 1000)
    assert needs_character_wrapping("word\u202f" * 1000)
    assert needs_character_wrapping("word\u2007" * 1000)
    assert needs_character_wrapping("word \u2060" * 1000)
    assert needs_character_wrapping("word \ufeff" * 1000)
    assert not needs_character_wrapping("word \u2060\n" * 1000)


def test_adversarial_delimiters_finish_within_a_bounded_process() -> None:
    """Guard the 120k editable path against hangs without hanging the whole test runner."""
    probe = """
from voice_scribe_linux.minimal_markdown import markdown_spans, visible_markdown

size = 120_000
sources = (
    "x " + "`" * size,
    "x " + "`" * size + "body" + "`" * (size - 1),
    "x " + "\\\\`" * (size // 2),
    "*a " * (size // 3),
    "**a __b " * (size // 8),
    "x " + "*" * size + "_" * size,
    "x " + " ".join("`" * length for length in range(1, 500)),
    "*a* " * (size // 4),
)
for source in sources:
    spans = markdown_spans(source)
    visible, styles = visible_markdown(source, spans)
    assert all(0 <= span.start < span.end <= len(source) for span in spans)
    assert all(0 <= span.start < span.end <= len(visible) for span in styles)
print("all adversarial documents completed")
"""
    completed = subprocess.run(
        [sys.executable, "-c", probe],
        cwd=Path(__file__).parents[1],
        capture_output=True,
        text=True,
        timeout=5,
        check=True,
    )
    assert completed.stdout.strip() == "all adversarial documents completed"
