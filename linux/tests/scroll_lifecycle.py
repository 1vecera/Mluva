"""Exercise real GTK scrolling during incremental recognition and rewrite updates."""

import json
import time
from dataclasses import replace
from pathlib import Path

import gi

from mluva_linux.app import MluvaApplication

gi.require_version("Gtk", "4.0")
from gi.repository import GLib, Gtk  # noqa: E402


def exercise_scrolling(application: MluvaApplication, output: Path) -> None:
    """Prove intermediate positions, manual reading, corrected tails and document changes."""
    workspace = application.conversation_workspace
    workspace.set_config(replace(application.config, smooth_scrolling=True, scroll_duration_ms=800))
    Gtk.Settings.get_default().set_property("gtk-enable-animations", True)

    def frames(adjustment: Gtk.Adjustment, seconds: float = 1.05) -> list[float]:
        """Collect actual adjustment positions while the private GTK frame clock runs."""
        positions = []
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            while GLib.MainContext.default().pending():
                GLib.MainContext.default().iteration(False)
            positions.append(adjustment.get_value())
            time.sleep(0.01)
        return positions

    def at_end(adjustment: Gtk.Adjustment) -> bool:
        """Check the native viewport against the measured content extent."""
        return abs(adjustment.get_value() + adjustment.get_page_size() - adjustment.get_upper()) < 1

    source = "\n".join(f"A steady line of dictated text {index}." for index in range(48))
    workspace.set_live("Recording", source)
    live = workspace.live_scroll.get_vadjustment()
    frames(live)
    assert at_end(live)
    before = live.get_value()
    source += "\nOne more line arrives.\nThen the next complete thought."
    workspace.set_live("Recording", source)
    live_motion = frames(live)
    assert at_end(live)
    assert any(before + 1 < value < live.get_value() - 1 for value in live_motion), live_motion
    assert workspace.live_text.get_bottom_margin() >= workspace.config.scroll_lookahead_lines * 20
    live.set_value(90)
    assert not workspace.live_follower.following
    source += "\nThe reader stays where they scrolled."
    workspace.set_live("Recording", source)
    frames(live)
    assert abs(live.get_value() - 90) < 1
    corrected = source.removesuffix("scrolled.") + "stopped."
    workspace.set_live("Recording", corrected)
    frames(live)
    buffer = workspace.live_text.get_buffer()
    assert buffer.get_text(buffer.get_start_iter(), buffer.get_end_iter(), False) == corrected
    assert abs(live.get_value() - 90) < 1
    live.set_value(live.get_upper() - live.get_page_size())
    workspace.set_live("Recording", corrected + "\nFollowing resumes at the bottom.")
    frames(live)
    assert at_end(live)
    workspace.finish_live()
    assert buffer.get_char_count() == 0 and not workspace.live_column.get_visible()

    workspace.set_live("Recording", source)
    workspace.show_live_draft(source)
    draft = workspace.live_draft_scroll.get_vadjustment()
    workspace.live_draft_follower.follow(snap=True)
    frames(draft)
    assert at_end(draft)
    before = draft.get_value()
    workspace.show_live_draft(source + "\nThe completed draft grows.\nIts latest words stay visible.")

    def queue_same_destination():
        """Repeated word updates without another wrap must not restart the whole easing curve."""
        workspace.live_draft_follower.queue()
        return GLib.SOURCE_CONTINUE

    repeated = GLib.timeout_add(30, queue_same_destination)
    draft_motion = frames(draft)
    GLib.source_remove(repeated)
    assert at_end(draft) and workspace.live_draft_follower.following
    assert any(before + 1 < value < draft.get_value() - 1 for value in draft_motion), draft_motion
    draft.set_value(80)
    workspace.show_live_draft(source + "\nA complete replacement preserves the reader's position.")
    frames(draft)
    assert abs(draft.get_value() - 80) < 1 and not workspace.live_draft_follower.following
    for mode in ("smooth-off", "duration-zero", "reduced-motion"):
        workspace.set_config(
            replace(
                application.config,
                smooth_scrolling=mode != "smooth-off",
                scroll_duration_ms=0 if mode == "duration-zero" else 800,
            )
        )
        Gtk.Settings.get_default().set_property("gtk-enable-animations", mode != "reduced-motion")
        workspace.live_draft_follower.follow(snap=True)
        frames(draft, 0.1)
        workspace.show_live_draft(source + "\nNew text appears without animation.\n" + mode)
        positions = frames(draft, 0.15)
        assert at_end(draft), (mode, positions)
    workspace.finish_live()
    workspace.set_config(application.config)
    Gtk.Settings.get_default().set_property("gtk-enable-animations", True)

    entry = application.history_store.add(source, source, "dictation", "eng", None, "copied")
    workspace.show_conversation(entry, [])
    conversation = workspace.scroll.get_vadjustment()
    frames(conversation)
    workspace.set_rewrite_preview(entry.identifier, "The rewritten result begins here.")
    frames(conversation)
    before = conversation.get_value()
    workspace.set_rewrite_preview(entry.identifier, "The rewritten result begins here.\nIt grows across two lines.")
    rewrite_motion = frames(conversation)
    assert at_end(conversation)
    assert any(before + 1 < value < conversation.get_value() - 1 for value in rewrite_motion), rewrite_motion
    conversation.set_value(100)
    workspace.set_rewrite_preview(
        entry.identifier, "The rewritten result begins here.\nA longer result arrives.\nMore."
    )
    frames(conversation)
    assert abs(conversation.get_value() - 100) < 1
    workspace.show_conversation(None, [])
    frames(conversation)
    assert conversation.get_value() == 0
    workspace.set_live("Recording", "A fresh short capture.")
    frames(live)
    assert live.get_value() == 0 and workspace.live_follower.following
    workspace.finish_live()
    workspace.clear_rewrite_preview()
    application.history_store.delete(entry.identifier)
    Gtk.Settings.get_default().set_property("gtk-enable-animations", False)
    (output / "scroll-lifecycle.json").write_text(
        json.dumps(
            {
                "dictation_positions": live_motion,
                "rewrite_positions": rewrite_motion,
                "draft_positions": draft_motion,
                "manual_reading_preserved": True,
                "reduced_motion_and_smooth_settings": True,
            }
        )
    )
