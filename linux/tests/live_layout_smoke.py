"""Measure real Live panes, Markdown editing and scrolling on a private X11 display."""

import json
import os
import struct
import subprocess
import time
import traceback
from dataclasses import replace
from pathlib import Path
from unittest.mock import patch

import gi
from conversation_ui_smoke import IsolatedApplication

from mluva_linux.conversation import STRUCTURED_NOTE
from mluva_linux.pipewire import PipeWireDeviceCatalog
from mluva_linux.ui import set_button_content

gi.require_version("Gtk", "4.0")
gi.require_version("GdkX11", "4.0")
from gi.repository import GLib, Gtk  # noqa: E402


def frames(seconds: float = 0.3, adjustment: Gtk.Adjustment | None = None) -> list[float]:
    """Wait for real frame-clock allocation and collect animation positions when requested."""
    positions = []
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        while GLib.MainContext.default().pending():
            GLib.MainContext.default().iteration(False)
        if adjustment is not None:
            positions.append(adjustment.get_value())
        time.sleep(0.01)
    return positions


def bounds(widget: Gtk.Widget, parent: Gtk.Widget) -> tuple[int, int, int, int]:
    """Record settled child geometry in its parent's coordinate system."""
    success, rectangle = widget.compute_bounds(parent)
    assert success
    return tuple(
        round(value) for value in (rectangle.get_x(), rectangle.get_y(), rectangle.get_width(), rectangle.get_height())
    )


def main() -> int:
    """Exercise production widgets with fake external boundaries and synthetic local documents."""
    assert "OFFSCREEN_SESSION_ROOT" in os.environ and os.environ.get("GDK_BACKEND") == "x11"
    assert os.environ["DISPLAY"] == f":{os.environ['OFFSCREEN_DISPLAY_NUMBER']}"
    output = Path(os.environ["OFFSCREEN_ARTIFACT_DIR"])
    portal_config = Path(os.environ["XDG_CONFIG_HOME"]) / "xdg-desktop-portal/portals.conf"
    portal_config.parent.mkdir(parents=True, exist_ok=True)
    portal_config.write_text("[preferred]\ndefault=gtk\n")
    width = int(os.environ.get("MLUVA_UI_WIDTH", "1060"))
    height = int(os.environ.get("MLUVA_UI_HEIGHT", "780"))
    application = IsolatedApplication()
    errors = []
    markdown = (
        "# A calmer workspace\n\n**Keep every word.** Make *small* edits.\n\n"
        "- Dictate naturally\n- Polish and save\n\n> Your voice stays yours.\n\n`Ctrl+P` opens commands.\n"
    )

    def exercise() -> bool:
        """Assert stable allocation across overflow and the same raw source after focus and save."""
        try:
            window = application.window
            window.set_default_size(width, height)
            window.set_size_request(width, height)
            workspace = application.conversation_workspace
            application._navigate_to_page("capture")
            Gtk.Settings.get_default().set_property("gtk-enable-animations", True)
            workspace.set_config(replace(application.config, smooth_scrolling=True, scroll_duration_ms=350))
            for index in range(40):
                application.history_store.add(f"Synthetic note {index}", "", "dictation", "eng", None, "copied")
            entry = application.history_store.add("Original words", "", "dictation", "eng", None, "copied")
            application.conversation_store.append(entry.identifier, STRUCTURED_NOTE, markdown, "fixture")
            workspace.refresh_history()
            workspace.show_conversation(entry, application.conversation_store.replies(entry.identifier))
            workspace.set_live("01:13", "Words arrive naturally.")
            workspace.show_live_draft(markdown, "Live draft")
            workspace.live_draft_follower.follow(snap=True)
            frames()
            panes = (workspace.live_source_box, workspace.live_draft_box)
            initial = [bounds(pane, window) for pane in panes]
            assert application.header_bar.get_title_widget() is workspace.live_header
            assert workspace.live_header.get_mapped() and not workspace.heading_column.get_mapped()
            assert workspace.live_light._tick_id
            header = bounds(workspace.live_header, window)
            assert header[1] + header[3] <= min(pane[1] for pane in initial)
            assert abs(initial[0][2] - initial[1][2]) <= 1, initial
            if workspace.live_box.get_orientation() == Gtk.Orientation.HORIZONTAL:
                assert initial[1][0] - initial[0][0] - initial[0][2] == 24, initial
                assert bounds(workspace.live_scroll, window)[1] == bounds(workspace.live_draft_scroll, window)[1]
            else:
                assert abs(initial[0][3] - initial[1][3]) <= 1, initial
                assert initial[1][1] - initial[0][1] - initial[0][3] == 24, initial
            application._publish_completion_status("processing", "Finishing your dictation.")
            frames()
            assert workspace.live_title.get_label() == "Processing…" and not workspace.live_light._tick_id
            assert [bounds(pane, window) for pane in panes] == initial
            source = "\n".join(f"A steady dictated thought {index}." for index in range(48))
            workspace.set_live("Listening", source)
            workspace.show_live_draft(markdown * 12, "Live draft · provisional until Stop · waiting for speech")
            frames(0.7)
            assert [bounds(pane, window) for pane in panes] == initial
            gutters = {}
            for name, scroll in (
                ("history", workspace.history_scroll),
                ("dictation", workspace.live_scroll),
                ("rewrite", workspace.live_draft_scroll),
            ):
                assert not scroll.get_overlay_scrolling()
                assert scroll.get_policy()[1] == Gtk.PolicyType.ALWAYS
                child = bounds(scroll.get_child(), scroll)
                bar = bounds(scroll.get_vscrollbar(), scroll)
                assert child[0] + child[2] <= bar[0], (name, child, bar)
                gutters[name] = {"content": child, "scrollbar": bar}
            live = workspace.live_scroll.get_vadjustment()
            before = live.get_value()
            workspace.set_live("Listening", source + "\nOne fresh thought.\nAnd one more.")
            positions = frames(0.6, live)
            assert any(before + 1 < value < live.get_value() - 1 for value in positions), positions
            ending = workspace.live_text.get_iter_location(workspace.live_text.get_buffer().get_end_iter())
            visible = workspace.live_text.get_visible_rect()
            lookahead = visible.y + visible.height - ending.y - ending.height
            assert lookahead >= 30, lookahead
            view = workspace.live_text
            line_width = view.get_width() - view.get_left_margin() - view.get_right_margin()
            character_width = view.create_pango_layout("m").get_pixel_size()[0]
            before_count = int(line_width * 0.68 / character_width)
            near_count = int(line_width * 0.94 / character_width)
            workspace.set_live("Listening", source + "\n" + "m" * before_count)
            frames(0.6)
            before_y = view.get_iter_location(view.get_buffer().get_end_iter()).y
            before_wrap = live.get_value()
            workspace.set_live("Listening", source + "\n" + "m" * near_count)
            anticipatory_positions = frames(0.6, live)
            assert view.get_iter_location(view.get_buffer().get_end_iter()).y == before_y
            assert live.get_value() > before_wrap + 1, (before_wrap, live.get_value())
            live.set_value(80)
            workspace.set_live("Listening", source + "\nThe reader stays here.")
            frames()
            assert not workspace.live_follower.following and abs(live.get_value() - 80) < 1
            # Simulate an upward wheel arriving before a queued GTK content measurement.
            workspace.live_follower.follow()
            workspace.live_follower._reader_scrolled(None, 0, -1)
            frames()
            assert not workspace.live_follower.following and abs(live.get_value() - 80) < 1
            for source_text, draft in (("x" * 1200, "# Short\n"), ("Short source", markdown * 3)):
                workspace.set_live("Listening", source_text)
                workspace.show_live_draft(draft, "A different draft status that must not move either pane")
                frames()
                assert [bounds(pane, window) for pane in panes] == initial
            workspace.show_live_draft(markdown)
            frames()
            buffer = workspace.live_draft_text.get_buffer()
            raw = workspace.live_draft()
            assert raw == markdown
            assert buffer.get_text(*buffer.get_bounds(), False).startswith("A calmer workspace")
            workspace.live_draft_text.grab_focus()
            frames()
            assert buffer.get_text(*buffer.get_bounds(), False) == markdown, (
                workspace.live_draft_text.has_focus(),
                workspace.live_draft_text.is_focus(),
                window.get_focus(),
                workspace.live_draft_text.styles["syntax"].get_property("invisible"),
            )
            assert not workspace.live_draft_follower.following
            buffer.insert(buffer.get_end_iter(), "\n**My deliberate edit**\n")
            edited = markdown + "\n**My deliberate edit**\n"
            assert workspace.live_draft() == edited
            selection_start = edited.index("Keep every word")
            buffer.select_range(
                buffer.get_iter_at_offset(selection_start), buffer.get_iter_at_offset(selection_start + 5)
            )
            workspace.show_live_draft(edited + "\nNew speech.\n")
            assert buffer.get_selection_bounds()[0].get_offset() == selection_start
            assert workspace.live_draft().startswith(edited)
            workspace.live_draft_status.grab_focus()
            workspace.live_text.grab_focus()
            frames()
            assert workspace.live_draft() == edited + "\nNew speech.\n"
            # Retain a readable Live fixture for pixel review.
            workspace.show_live_draft(markdown)
            buffer.place_cursor(buffer.get_start_iter())
            workspace.set_live(
                "01:13",
                "I want a small, peaceful dictation app.\n\nEvery word stays visible. "
                "The interface adapts to Omarchy, and the draft takes shape beside my voice.",
            )
            workspace.live_follower.follow(snap=True)
            workspace.live_draft_follower.follow(snap=True)
            application.status_label.set_label("Listening. Press F9 when you're done.")
            set_button_content(application.record_button, "media-playback-stop-symbolic", "Stop")
            frames()
            assert window.get_surface().get_width() == width and window.get_surface().get_height() == height
            subprocess.run(
                ["import", "-window", str(window.get_surface().get_xid()), str(output / "live-layout.png")], check=True
            )
            scale = window.get_surface().get_scale_factor()
            assert struct.unpack(">II", (output / "live-layout.png").read_bytes()[16:24]) == (
                width * scale,
                height * scale,
            )
            workspace.finish_live()
            assert not workspace.live_light._tick_id and not workspace.live_header.get_visible()
            assert application.header_bar.get_title_widget() is not workspace.live_header
            workspace.show_conversation(entry, application.conversation_store.replies(entry.identifier))
            result = workspace.result_widgets[-1]
            assert result.get_text() == markdown
            copied = []
            workspace.copy_text = copied.append
            assert workspace.copy_current_output() and copied == [markdown]
            result.grab_focus()
            frames()
            result.get_buffer().insert(result.get_buffer().get_end_iter(), "\n*An intentional saved edit.*\n")
            assert workspace.save_edits()
            assert application.conversation_store.replies(entry.identifier)[0].text == result.get_text()
            workspace.focus_prompt()
            frames()
            assert result.get_text().startswith(markdown)
            subprocess.run(
                ["import", "-window", str(window.get_surface().get_xid()), str(output / "markdown.png")], check=True
            )
            (output / "receipt.json").write_text(
                json.dumps(
                    {
                        "viewport": [width, height],
                        "display": os.environ["DISPLAY"],
                        "settled": True,
                        "panes": initial,
                        "gutters": gutters,
                        "lookahead_pixels": lookahead,
                        "scroll_positions": positions,
                        "scroll_before_wrap": anticipatory_positions,
                        "manual_reading_preserved": True,
                        "markdown_copy_save_and_selection_lossless": True,
                    },
                    indent=2,
                )
            )
        except Exception:
            errors.append(traceback.format_exc())
        application.quit()
        return GLib.SOURCE_REMOVE

    with (
        patch("mluva_linux.app.FocusedTextTargetTracker", return_value=None),
        patch.object(PipeWireDeviceCatalog, "from_system", return_value=PipeWireDeviceCatalog()),
    ):
        GLib.idle_add(exercise)
        application.run(None)
    if errors:
        raise RuntimeError("; ".join(errors))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
