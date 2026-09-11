"""Exercise real Ctrl-P routing and native command dispatch on a private X11 display."""

import ctypes
import json
import os
import traceback
from pathlib import Path
from unittest.mock import patch

from conversation_ui_smoke import IsolatedApplication
from gi.repository import GLib, Gtk
from live_workspace_smoke import paint, settle

from mluva_linux.pipewire import PipeWireDeviceCatalog


def main() -> int:
    """Verify editor shortcuts, dismissal, disabled actions and lossless document commands."""
    if "OFFSCREEN_SESSION_ROOT" not in os.environ or os.environ.get("GDK_BACKEND") != "x11":
        raise RuntimeError("Use the isolated X11 runner")
    app = IsolatedApplication()
    output = Path(os.environ["OFFSCREEN_ARTIFACT_DIR"])
    errors = []
    copies = []

    def key(chord: str) -> None:
        """Send X11 input only to the fixture window on the explicitly private display."""
        x11 = ctypes.CDLL("libX11.so.6")
        xtest = ctypes.CDLL("libXtst.so.6")
        x11.XOpenDisplay.restype = ctypes.c_void_p
        x11.XSetInputFocus.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_ulong]
        x11.XStringToKeysym.argtypes = [ctypes.c_char_p]
        x11.XStringToKeysym.restype = ctypes.c_ulong
        x11.XKeysymToKeycode.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
        x11.XKeysymToKeycode.restype = ctypes.c_uint
        x11.XSync.argtypes = [ctypes.c_void_p, ctypes.c_int]
        x11.XCloseDisplay.argtypes = [ctypes.c_void_p]
        xtest.XTestFakeKeyEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
        display = x11.XOpenDisplay(None)
        assert display, "Private X11 display unavailable"
        try:
            x11.XSetInputFocus(display, app.window.get_surface().get_xid(), 1, 0)
            names = ["Control_L", "p"] if chord == "ctrl+p" else [chord]
            codes = [x11.XKeysymToKeycode(display, x11.XStringToKeysym(name.encode())) for name in names]
            assert all(codes)
            for code in codes:
                assert xtest.XTestFakeKeyEvent(display, code, True, 0)
            for code in reversed(codes):
                assert xtest.XTestFakeKeyEvent(display, code, False, 0)
            x11.XSync(display, False)
        finally:
            x11.XCloseDisplay(display)

    def open_panel():
        """Prove Ctrl-P is reachable even when a native editable TextView owns focus."""
        settle(lambda: app.window.get_visible_dialog() is None)
        key("ctrl+p")
        settle(lambda: app.command_palette is not None and app.command_palette.search.get_mapped())
        return app.command_palette

    def choose(title: str) -> None:
        """Filter native rows and dispatch the selected command through its Enter handler."""
        panel = app.command_palette
        panel.search.set_text(title)
        settle(lambda: any(command.title == title for command in panel.rows.values()))
        panel.results.select_row(next(row for row, command in panel.rows.items() if command.title == title))
        key("Return")
        settle(lambda: app.command_palette is None)

    def exercise() -> bool:
        """Operate production widgets with fake clipboard and device boundaries only."""
        try:
            Gtk.Settings.get_default().set_property("gtk-enable-animations", False)
            workspace = app.conversation_workspace
            workspace.copy_text = copies.append
            panel = open_panel()
            document_actions = {"Copy current text", "Polish text", "Save edits", "Rewrite with an instruction"}
            for row, command in panel.rows.items():
                if command.title in document_actions:
                    assert not row.get_sensitive(), command.title
            key("Escape")
            settle(lambda: app.command_palette is None)
            entry = app.history_store.add("Original speech", "Original speech", "dictation", "eng", None, "copied")
            reply = app.conversation_store.append(entry.identifier, "Polish", "# A note\n**Keep** this.", "fixture")
            workspace.show_conversation(entry, [reply])
            editor = workspace.result_widgets[-1]
            editor.grab_focus()
            buffer = editor.get_buffer()
            buffer.insert(buffer.get_end_iter(), " Edited.")
            expected = "# A note\n**Keep** this. Edited."
            panel = open_panel()
            assert panel.search.has_focus() or app.window.get_focus().is_ancestor(panel.search)
            paint(app.window, output / "commands.png")
            with patch.object(app, "_cancel_capture", return_value=True) as cancel:
                key("Escape")
                settle(lambda: app.command_palette is None)
                cancel.assert_not_called()

            with patch.object(app, "_cancel_capture", return_value=True) as cancel:
                popover = app.live_mode_menu.get_popover()
                popover.popup()
                settle(popover.get_mapped)
                key("Escape")
                settle(lambda: not popover.get_mapped())
                cancel.assert_not_called()

            open_panel()
            choose("Copy current text")
            assert copies == [expected], copies
            open_panel()
            choose("Save edits")
            assert app.conversation_store.replies(entry.identifier)[0].text == expected
            assert app.history_store.find(entry.identifier).raw_text == "Original speech"

            panel = open_panel()
            panel.search.set_text("does-not-exist")
            settle(lambda: panel.empty.get_visible())
            key("Return")
            assert app.command_palette is panel
            key("ctrl+p")
            settle(lambda: app.command_palette is None)

            panel = open_panel()
            panel.search.set_text("Polish text")
            settle(lambda: len(panel.rows) == 1)
            workspace.set_busy(True, "Finishing…")
            with patch.object(workspace, "request_rewrite") as rewrite:
                key("Return")
                settle(lambda: not next(iter(panel.rows)).get_sensitive())
                rewrite.assert_not_called()
                assert app.command_palette is panel
            key("Escape")
            settle(lambda: app.command_palette is None)
            workspace.set_busy(False, "Ready")

            app._navigate_to_page("history")
            panel = open_panel()
            for row, command in panel.rows.items():
                if command.title in document_actions:
                    assert not row.get_sensitive(), command.title
            key("Escape")
            settle(lambda: app.command_palette is None)
            app._navigate_to_page("capture")
            panel = open_panel()
            panel.search.set_text("Copy current text")
            settle(lambda: len(panel.rows) == 1)
            other = app.history_store.add("Other note", "Other note", "dictation", "eng", None, "copied")
            workspace.show_conversation(other, [])
            count = len(copies)
            key("Return")
            settle(lambda: not next(iter(panel.rows)).get_sensitive())
            assert len(copies) == count
            key("Escape")
            settle(lambda: app.command_palette is None)
            workspace.show_conversation(entry, app.conversation_store.replies(entry.identifier))

            open_panel()
            choose("Settings")
            settle(lambda: app.window.get_visible_dialog() is app.settings_dialog)
            key("Escape")
            settle(lambda: app.window.get_visible_dialog() is None)

            workspace.set_live("Recording  00:05", "A provisional phrase")
            panel = open_panel()
            for row, command in panel.rows.items():
                if command.title in {"Copy current text", "Polish text", "Save edits", "Rewrite with an instruction"}:
                    assert not row.get_sensitive(), command.title
            key("Escape")
            settle(lambda: app.command_palette is None)
            workspace.finish_live()
            app.window.set_default_size(480, 680)
            paint(app.window, output / "workspace-narrow.png")
            panel = open_panel()
            paint(app.window, output / "commands-narrow.png")
            assert panel.get_width() <= app.window.get_width()
            scroller = panel.results.get_parent()
            while not isinstance(scroller, Gtk.ScrolledWindow):
                scroller = scroller.get_parent()
            for _ in range(len(panel._available_rows()) - 1):
                previous = panel.results.get_selected_row()
                key("Down")
                settle(lambda previous=previous: panel.results.get_selected_row() is not previous)

            def selected_is_visible() -> bool:
                """Keyboard selection must remain visible while search retains focus."""
                valid, bounds = panel.results.get_selected_row().compute_bounds(scroller)
                return (
                    valid and bounds.get_y() >= -1 and bounds.get_y() + bounds.get_height() <= scroller.get_height() + 1
                )

            settle(selected_is_visible)
            paint(app.window, output / "commands-narrow-last.png")
            panel.close()
            settle(lambda: app.command_palette is None)
            (output / "command-palette.json").write_text(
                json.dumps(
                    {
                        "ctrl_p_from_native_editor": True,
                        "escape_does_not_cancel_capture": True,
                        "popover_escape_does_not_cancel_capture": True,
                        "copy_preserves_markdown_and_edits": True,
                        "empty_workspace_document_actions_disabled": True,
                        "save_preserves_raw_recognition": True,
                        "empty_search_does_not_dispatch": True,
                        "stale_action_is_rechecked": True,
                        "hidden_document_actions_disabled": True,
                        "changed_conversation_cannot_redirect_command": True,
                        "settings_focus_transfer": True,
                        "provisional_live_actions_disabled": True,
                        "narrow_dialog_fits": True,
                        "keyboard_selection_scrolls_into_view": True,
                    },
                    indent=2,
                )
            )
        except Exception:
            errors.append(traceback.format_exc())
        app.quit()
        return GLib.SOURCE_REMOVE

    def activated(_app) -> None:
        """Wait for the initial native window without repeating the scenario on reactivation."""
        app.disconnect(activation)
        GLib.timeout_add(500, exercise)

    activation = app.connect("activate", activated)
    with (
        patch("mluva_linux.app.FocusedTextTargetTracker", return_value=None),
        patch.object(PipeWireDeviceCatalog, "from_system", return_value=PipeWireDeviceCatalog()),
    ):
        app.run([])
    if errors:
        raise RuntimeError("\n".join(errors))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
