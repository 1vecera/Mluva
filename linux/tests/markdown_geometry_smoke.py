"""Keep complete saved Markdown reachable with GTK's actual text layout and shared scrolling."""

import json
import os
import time
from pathlib import Path

import gi

from voice_scribe_linux.conversation_view import DocumentEditor
from voice_scribe_linux.theme import ThemeController

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, GLib, Gtk  # noqa: E402


def main() -> int:
    """Compare the allocated editor and next document with real end-of-buffer coordinates."""
    assert "OFFSCREEN_SESSION_ROOT" in os.environ and os.environ.get("GDK_BACKEND") == "x11"
    assert os.environ["DISPLAY"] == f":{os.environ['OFFSCREEN_DISPLAY_NUMBER']}"
    output = Path(os.environ["OFFSCREEN_ARTIFACT_DIR"])
    Adw.init()
    theme = ThemeController()
    theme.apply()
    context = GLib.MainContext.default()
    wordline = "A quiet first line. " * 30
    cases = {
        "nested-lists": ("  - " + wordline + "\n") * 20,
        "tabbed-lists": ("\t- " + wordline + "\n") * 20,
        "quotes": ("> " + wordline + "\n") * 20,
        "headings": ("# " + wordline + "\n###### " + wordline + "\n") * 10,
        "mixed-styles": ("## " + "**Keep _this_ meaning** with `source_code()` " * 20 + "\n") * 15,
        "tabbed-code": "```\n" + "\tprint('Žluťoučký 🐴')\t# exact source\n" * 50 + "```\n",
        "unicode": ("## Čeština 日本語 🙂\n\n" + "**Žluťoučký kůň** and `data_🐴` repeated. " * 12 + "\n") * 10,
        "many-soft-lines": "Brief\u2028" * 20_000,
        "many-hard-lines": "Brief\r\n" * 17_000,
    }
    for name, separator in (("lf", "\n"), ("crlf", "\r\n"), ("cr", "\r"), ("ls", "\u2028"), ("ps", "\u2029")):
        cases[name] = ("## First" + separator + "A quiet **line**. " * 8 + separator) * 30
    rows = []
    for width in (300, 420, 700):
        window = Gtk.Window(default_width=width, default_height=300)
        window.present()
        for name, content in cases.items():
            source = content + "\nEND-OF-DOCUMENT"
            for source_visible in (False, True):
                editor = DocumentEditor(source)
                editor.set_editable(False)
                editor.styles["syntax"].set_property("invisible", not source_visible)
                editor.queue_resize()
                column = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
                column.append(editor)
                following = Gtk.Label(label="FOLLOWING DOCUMENT")
                column.append(following)
                window.set_child(Gtk.ScrolledWindow(child=column))
                deadline = time.monotonic() + 0.10
                while time.monotonic() < deadline:
                    while context.pending():
                        context.iteration(False)
                    time.sleep(0.001)
                assert editor.get_width() == width
                ending = editor.get_iter_location(editor.get_buffer().get_end_iter())
                found, following_bounds = following.compute_bounds(column)
                row = {
                    "case": name,
                    "source_visible": source_visible,
                    "width": width,
                    "editor_height": editor.get_height(),
                    "native_text_bottom": ending.y + ending.height,
                    "following_top": round(following_bounds.get_y()),
                }
                assert ending.y > 0 and ending.height > 0 and found, row
                assert row["native_text_bottom"] <= row["editor_height"] <= row["following_top"], row
                # Extra room must not accumulate with every paragraph after measurement rounding.
                assert row["editor_height"] - row["native_text_bottom"] <= 32, row
                assert editor.get_text() == source
                rows.append(row)
                print(json.dumps(row), flush=True)
                (output / "receipt.json").write_text(json.dumps(rows, indent=2))
        window.destroy()
    theme.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
