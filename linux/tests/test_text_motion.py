"""Exercise exact multi-edit revisions and speech-rate lookahead without a display."""

import random

from mluva_linux.scroll_forecast import SpeechScrollForecast
from mluva_linux.text_diff import text_changes


def test_separate_corrections_preserve_the_unchanged_middle():
    """Corrections at both ends must not fade the stable sentence between them."""
    before = "Tuesday. Keep this whole thought. Cheers, Alex."
    after = "Wednesday. Keep this whole thought. Thanks, Alex."
    changes = text_changes(before, after)
    unchanged = before.index("Keep this whole thought.")
    assert all(change.old_end <= unchanged or change.old_start > unchanged + 20 for change in changes)
    result = before
    for change in reversed(changes):
        result = result[: change.old_start] + after[change.new_start : change.new_end] + result[change.old_end :]
    assert result == after


def test_unicode_markdown_and_arbitrary_revisions_round_trip():
    """Visual edits must reproduce all source characters, including emoji and Markdown delimiters."""
    generator = random.Random(731)
    alphabet = ["Ahoj ", "mám ", "🌿", "**yes**", "<script>", "\n", "e\u0301", "  "]
    for _ in range(150):
        before = "".join(generator.choices(alphabet, k=30))
        after = "".join(generator.choices(alphabet, k=30))
        result = before
        for change in reversed(text_changes(before, after)):
            result = result[: change.old_start] + after[change.new_start : change.new_end] + result[change.old_end :]
        assert result == after


def test_large_revisions_have_bounded_diff_work():
    """A large provider replacement keeps exact content while limiting expensive matching."""
    before, after = "old word " * 3000, "new word " * 3000
    assert len(text_changes(before, after)) == 1


def test_fast_speech_reserves_the_next_line_earlier():
    """The lead depends on observed pace, rather than a fixed fraction of the current line."""
    slow, fast = SpeechScrollForecast(), SpeechScrollForecast()
    for elapsed in (0, 1, 2, 3):
        slow.observe(elapsed * 8, elapsed)
        fast.observe(elapsed * 40, elapsed)
    assert slow.reserve(50, 0.55, 0.9, 2) == 0
    assert fast.reserve(50, 0.55, 0.9, 2) > 0
    assert fast.reserve(50, 0.55, 0.9, 0) == 0


def test_corrections_do_not_create_false_speech_growth():
    """Contraction and re-expansion of the same transcript cannot inflate the pace estimate."""
    forecast = SpeechScrollForecast()
    forecast.observe(100, 0)
    forecast.observe(80, 1)
    forecast.observe(100, 2)
    assert forecast.reserve(50, 0.9, 1, 2) == 0
