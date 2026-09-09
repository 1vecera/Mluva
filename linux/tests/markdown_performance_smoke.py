"""Bound native Markdown formatting and wrapping at provider and editable-document sizes."""

import json
import os
import time
from pathlib import Path

import gi

from voice_scribe_linux.markdown_view import MarkdownTextView
from voice_scribe_linux.theme import ThemeController

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, GLib, Gtk  # noqa: E402


def main() -> int:
    """Exercise actual GTK tags and end-of-buffer layout only inside the private X11 runner."""
    assert "OFFSCREEN_SESSION_ROOT" in os.environ and os.environ.get("GDK_BACKEND") == "x11"
    assert os.environ["DISPLAY"] == f":{os.environ['OFFSCREEN_DISPLAY_NUMBER']}"
    output = Path(os.environ["OFFSCREEN_ARTIFACT_DIR"])
    Adw.init()
    theme = ThemeController()
    theme.apply()
    view = MarkdownTextView(editable=False)
    window = Gtk.Window(default_width=640, default_height=480)
    window.set_child(Gtk.ScrolledWindow(child=view))
    window.present()
    context = GLib.MainContext.default()
    deadline = time.monotonic() + 5
    while not (window.get_mapped() and view.get_width() > 0) and time.monotonic() < deadline:
        context.iteration(False)
        time.sleep(0.005)
    assert window.get_mapped() and view.get_width() > 0
    paragraph = (
        "## A clear next step\n\nKeep **the original meaning** while making the next action easy to find. "
        + "The draft preserves the speaker's language and useful details without adding new facts. " * 8
        + "\n\n"
    )
    rows = []
    for size in (40_000, 120_000):
        sources = {
            "rich-note": (paragraph * (size // len(paragraph) + 1))[:size],
            "unmatched-ticks": "x " + "`" * (size - 2),
            "escaped-ticks": "\\`" * (size // 2),
            "unmatched-emphasis": ("*a " * (size // 3 + 1))[:size],
            "many-matched-spans": ("*a* " * (size // 4 + 1))[:size],
            "many-paragraphs": ("# Brief\n\n**word**\n\n" * (size // 19 + 1))[:size],
            "nested-budget": "A " * ((size - 4080) // 2) + "*a " * 680 + " z*" * 680,
            "figure-space": ("word\u2007" * (size // 5 + 1))[:size],
            "word-joiner": ("word \u2060" * (size // 6 + 1))[:size],
            "unicode-paragraphs": ("Brief\u2029" * (size // 6 + 1))[:size],
        }
        for name, source in sources.items():
            started = time.monotonic()
            view.get_buffer().set_text(source)
            tagged = time.monotonic()
            view.document_height(420)
            measured = time.monotonic()
            painted = 0

            def tick(_widget: Gtk.Widget, _clock: object) -> bool:
                """Include deferred GTK allocation and drawing after the source replacement."""
                nonlocal painted
                painted += 1
                return painted < 2

            view.add_tick_callback(tick)
            while painted < 2 and time.monotonic() - started < 2:
                context.iteration(False)
                time.sleep(0.002)
            assert painted >= 2
            ending = view.get_iter_location(view.get_buffer().get_end_iter())
            finished = time.monotonic()
            assert view.get_text() == source
            if name == "rich-note":
                assert view.spans and not view.formatting_limited
                assert view.get_wrap_mode() == Gtk.WrapMode.WORD_CHAR
                assert len(view.get_buffer().get_text(*view.get_buffer().get_bounds(), False)) < len(source)
            if name in {"escaped-ticks", "many-matched-spans", "many-paragraphs"}:
                assert view.formatting_limited and not view.spans
                assert view.get_buffer().get_text(*view.get_buffer().get_bounds(), False) == source
            if name in {"unmatched-ticks", "escaped-ticks", "figure-space", "word-joiner"}:
                assert view.get_wrap_mode() == Gtk.WrapMode.CHAR
            row = {
                "case": name,
                "characters": len(source),
                "formatting_spans": len(view.spans),
                "plain_markdown_fallback": view.formatting_limited,
                "tag_seconds": tagged - started,
                "measurement_seconds": measured - tagged,
                "native_layout_seconds": finished - measured,
                "total_seconds": finished - started,
                "native_end_y": ending.y,
                "raw_source_lossless": True,
            }
            rows.append(row)
            print(json.dumps(row), flush=True)
            (output / "receipt.json").write_text(json.dumps(rows, indent=2))
            assert finished - started < 2, row
    # A dense pasted document cannot leave later ordinary edits permanently unformatted.
    ordinary = "## Žluťoučký kůň\n\nKeep **every word**.\n"
    view.set_editable(True)
    view.get_buffer().set_text(ordinary)
    view.get_buffer().insert(view.get_buffer().get_end_iter(), "\n*Edit kept*\n")
    assert view.get_text() == ordinary + "\n*Edit kept*\n"
    assert view.spans and not view.formatting_limited and view.get_wrap_mode() == Gtk.WrapMode.WORD_CHAR
    window.destroy()
    theme.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
