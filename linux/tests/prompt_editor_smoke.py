"""Real native prompt controls and editor behavior on an isolated X11 display."""

import ctypes
import json
import os
import sys
import traceback
from dataclasses import replace
from pathlib import Path
from unittest.mock import patch

from conversation_ui_smoke import IsolatedApplication
from gi.repository import Adw, GLib, Gtk
from live_workspace_smoke import paint, settle

from mluva_linux.command_palette import application_commands
from mluva_linux.pipewire import PipeWireDeviceCatalog
from mluva_linux.prompts import PromptStore


class PrivateInput:
    """Scope keyboard and pointer input to the runner's private X11 server."""

    def __init__(self, window):
        """Bind only the private X11 server already validated by the fixture."""
        self.window = window
        self.x11 = ctypes.CDLL("libX11.so.6")
        self.xtest = ctypes.CDLL("libXtst.so.6")
        self.x11.XOpenDisplay.restype = ctypes.c_void_p
        self.x11.XSetInputFocus.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_ulong]
        self.x11.XStringToKeysym.argtypes = [ctypes.c_char_p]
        self.x11.XStringToKeysym.restype = ctypes.c_ulong
        self.x11.XKeysymToKeycode.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
        self.x11.XKeysymToKeycode.restype = ctypes.c_uint
        self.x11.XSync.argtypes = [ctypes.c_void_p, ctypes.c_int]
        self.x11.XCloseDisplay.argtypes = [ctypes.c_void_p]
        self.xtest.XTestFakeKeyEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
        self.xtest.XTestFakeMotionEvent.argtypes = [
            ctypes.c_void_p,
            ctypes.c_int,
            ctypes.c_int,
            ctypes.c_int,
            ctypes.c_ulong,
        ]
        self.x11.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
        self.x11.XDefaultRootWindow.restype = ctypes.c_ulong
        self.x11.XTranslateCoordinates.argtypes = [
            ctypes.c_void_p,
            ctypes.c_ulong,
            ctypes.c_ulong,
            ctypes.c_int,
            ctypes.c_int,
            ctypes.POINTER(ctypes.c_int),
            ctypes.POINTER(ctypes.c_int),
            ctypes.POINTER(ctypes.c_ulong),
        ]
        self.display = self.x11.XOpenDisplay(None)
        assert self.display

    def key(self, *names):
        """Deliver a physical key sequence inside the disposable display."""
        self.x11.XSetInputFocus(self.display, self.window.get_surface().get_xid(), 1, 0)
        codes = [self.x11.XKeysymToKeycode(self.display, self.x11.XStringToKeysym(name.encode())) for name in names]
        for code in codes:
            self.xtest.XTestFakeKeyEvent(self.display, code, True, 0)
        for code in reversed(codes):
            self.xtest.XTestFakeKeyEvent(self.display, code, False, 0)
        self.x11.XSync(self.display, False)

    def hover(self, widget):
        """Translate native widget bounds into the private screen coordinate space."""
        valid, bounds = widget.compute_bounds(self.window)
        assert valid
        x, y, child = ctypes.c_int(), ctypes.c_int(), ctypes.c_ulong()
        self.x11.XTranslateCoordinates(
            self.display,
            self.window.get_surface().get_xid(),
            self.x11.XDefaultRootWindow(self.display),
            int(bounds.get_x() + bounds.get_width() / 2),
            int(bounds.get_y() + bounds.get_height() / 2),
            ctypes.byref(x),
            ctypes.byref(y),
            ctypes.byref(child),
        )
        self.xtest.XTestFakeMotionEvent(self.display, -1, x.value, y.value, 0)
        self.x11.XSync(self.display, False)

    def close(self):
        """Release this fixture client without affecting any other display."""
        self.x11.XCloseDisplay(self.display)


def respond(dialog, label):
    """Click an actual AlertDialog response button rather than synthesizing its response signal."""

    def find(widget):
        if isinstance(widget, Gtk.Button) and widget.get_label() == label:
            return widget
        child = widget.get_first_child()
        while child is not None:
            found = find(child)
            if found is not None:
                return found
            child = child.get_next_sibling()
        return None

    button = find(dialog)
    assert button is not None, label
    button.emit("clicked")


def main():
    """Exercise production UI with synthetic documents and no microphone/provider/clipboard access."""
    if "OFFSCREEN_SESSION_ROOT" not in os.environ or os.environ.get("GDK_BACKEND") != "x11":
        raise RuntimeError("Use dev/run-isolated.sh")
    output = Path(os.environ["OFFSCREEN_ARTIFACT_DIR"])
    width, height = int(os.environ.get("MLUVA_UI_WIDTH", "1060")), int(os.environ.get("MLUVA_UI_HEIGHT", "780"))
    if os.environ.get("MLUVA_PROMPT_SCENARIO") == "bad-config" and "--restart-check" not in sys.argv:
        config_path = Path(os.environ["XDG_CONFIG_HOME"]) / "mluva/config.json"
        config_path.parent.mkdir(parents=True, exist_ok=True)
        config_path.write_text("{malformed configuration")
    app = IsolatedApplication()
    errors = []
    sys.excepthook = lambda *error: errors.append("".join(traceback.format_exception(*error)))

    def exercise():
        native = None
        try:
            Gtk.Settings.get_default().set_property("gtk-enable-animations", False)
            Adw.StyleManager.get_default().set_color_scheme(
                Adw.ColorScheme.FORCE_LIGHT
                if os.environ.get("MLUVA_UI_THEME") == "light"
                else Adw.ColorScheme.FORCE_DARK
            )
            app.window.set_default_size(width, height)
            app.window.set_size_request(width, height)
            settle(app.window.get_mapped)
            paint(app.window, output / "sizing.png")
            app.window.set_size_request(
                width + width - app.window.get_width(), height + height - app.window.get_height()
            )
            paint(app.window, output / "sizing-adjusted.png")
            settle(lambda: app.window.get_width() == width and app.window.get_height() == height)
            if "--restart-check" in sys.argv:
                app._open_prompt_editor("live-grilling")
                editor = app.prompt_editor
                settle(editor.editor.get_mapped)
                assert editor.text() == "Next recording instructions"
                assert editor.identity.get_label() == "Local override"
                paint(app.window, output / "restart.png")
                editor.force_close()
                (output / "restart.json").write_text(json.dumps({"passed": True, "pid": os.getpid()}))
                return GLib.SOURCE_REMOVE
            native = PrivateInput(app.window)
            workspace = app.conversation_workspace
            assert not workspace.quick_polish.is_sensitive()
            empty_edit = workspace.quick_polish.get_parent().get_last_child()
            assert empty_edit.is_sensitive()
            empty_edit.emit("clicked")
            assert app.prompt_editor.identifier == "rewrite-polish"
            app.prompt_editor.force_close()
            settle(lambda: app.prompt_editor is None)
            entry = app.history_store.add("Synthetic speech", "Synthetic speech", "dictation", "eng", None, "ready")
            workspace.show_conversation(entry, [])
            style = app.personalization_store.save_style("Synthetic brief", "Keep a concise brief.")
            app._refresh_style_controls()
            assert any("Synthetic brief" in command.title for command in application_commands(app))
            edit_button = workspace.quick_polish.get_parent().get_last_child()
            workspace.prompt.grab_focus()
            native.hover(app.header_bar)
            settle(lambda: edit_button.get_opacity() == 0)
            paint(app.window, output / "no-hover.png")
            native.hover(workspace.quick_polish)
            settle(lambda: edit_button.get_opacity() == 1)
            paint(app.window, output / "hover.png")
            native.hover(app.header_bar)
            edit_button.grab_focus()
            settle(edit_button.has_focus)
            with patch.object(workspace, "request_rewrite") as request:
                native.key("space")
                settle(lambda: app.prompt_editor is not None)
                assert app.prompt_editor.identifier == "rewrite-polish"
                request.assert_not_called()
            app.prompt_editor.cancel_button.emit("clicked")
            settle(lambda: app.prompt_editor is None)
            native.key("Control_L", "p")
            settle(lambda: app.command_palette is not None)
            panel = app.command_palette
            panel.search.set_text("Edit prompt Live Grilling")
            settle(lambda: len(panel.rows) == 1)
            native.key("Return")
            settle(lambda: app.prompt_editor is not None)
            editor = app.prompt_editor
            assert editor.identifier == "live-grilling"
            settle(editor.editor.get_mapped)
            paint(app.window, output / "editor.png")
            editor.editor.get_buffer().set_text(" ")
            assert not editor.save_button.get_sensitive()
            assert "cannot be empty" in editor.status.get_label()
            text = "Ask one useful question.\n\n## Architecture\nKeep exact Markdown and Žluťoučký text.\n"
            editor.editor.get_buffer().set_text(text)
            editor.cancel_button.emit("clicked")
            settle(lambda: editor.discard_dialog is not None)
            respond(editor.discard_dialog, "Keep editing")
            settle(lambda: editor.discard_dialog is None)
            assert editor.text() == text
            editor.save_button.emit("clicked")
            settle(lambda: app.prompt_editor is None)
            assert PromptStore(app.prompt_store.directory).read("live-grilling").text == text
            app._open_prompt_editor("live-grilling")
            editor = app.prompt_editor
            editor.reset_button.emit("clicked")
            assert app.prompt_store.read("live-grilling").text == text
            editor.cancel_button.emit("clicked")
            settle(lambda: editor.discard_dialog is not None)
            respond(editor.discard_dialog, "Discard")
            settle(lambda: app.prompt_editor is None)
            assert app.prompt_store.read("live-grilling").text == text
            # Every Live hover control opens its own prompt without changing selection.
            template_before = app.config.live_rewrite_template
            for template, choice in app.live_template_buttons.items():
                choice.get_parent().get_last_child().emit("clicked")
                assert app.prompt_editor.identifier == "live-" + template
                assert app.config.live_rewrite_template == template_before
                app.prompt_editor.force_close()
                settle(lambda: app.prompt_editor is None)
            app._show_settings(app.settings_button)
            app.settings_dialog.set_visible_page(app.prompts_page)
            settle(app.prompts_page.get_mapped)
            row = next(row for row in app.prompts_page.rows if row.get_title() == "Style · Synthetic brief")
            row.emit("activated")
            assert app.prompt_editor.identifier == "style-" + style.identifier.lower()
            app.prompt_editor.force_close()
            settle(lambda: app.prompt_editor is None)
            app.settings_dialog.close()
            settle(lambda: not app.settings_dialog.get_mapped())
            # An external edit must not be overwritten by an already-open editor.
            app._open_prompt_editor("live-grilling")
            editor = app.prompt_editor
            editor.editor.get_buffer().set_text("Unsaved local draft")
            app.prompt_store.path("live-grilling").write_text("Externally edited")
            editor.save_button.emit("clicked")
            assert "changed locally" in editor.status.get_label() and editor.text() == "Unsaved local draft"
            paint(app.window, output / "conflict.png")
            editor.force_close()
            settle(lambda: app.prompt_editor is None)
            app._open_prompt_editor("live-grilling")
            editor = app.prompt_editor
            editor.reset_button.emit("clicked")
            editor.save_button.emit("clicked")
            settle(lambda: app.prompt_editor is None)
            assert not app.prompt_store.path("live-grilling").exists()
            # Freeze instruction changes for the current recording, including a Live toggle.
            app.config = replace(app.config, live_rewrite_enabled=True)
            app.pending_session_identifier = "synthetic-recording"
            app.pending_mode, app.pending_incognito = "dictation", False
            app._start_live_rewrite()
            old = dict(app.live_prompts)
            workspace.live_draft_text.get_buffer().set_text(
                "## Questions\n- Keep my manual question\n\n## Architecture\nManual draft"
            )
            draft = workspace.live_draft()
            revision = app.live_revision
            app._open_prompt_editor("live-grilling")
            editor = app.prompt_editor
            editor.editor.get_buffer().set_text("Next recording instructions")
            editor.save_button.emit("clicked")
            settle(lambda: app.prompt_editor is None)
            assert app.live_prompts == old and workspace.live_draft() == draft and app.live_revision == revision
            app._start_live_rewrite(preserve_draft=True)
            assert app.live_prompts == old and workspace.live_draft() == draft
            app.pending_session_identifier = "next-recording"
            app._start_live_rewrite()
            assert app.live_prompts["live-grilling"] == "Next recording instructions"
            app._cancel_live_rewrite()
            # The saved-prompt menu's gear edits the exact UUID without executing it.
            saved_row = workspace.saved_prompts.get_popover().get_child().get_last_child()
            saved_row.get_last_child().emit("clicked")
            assert app.prompt_editor.identifier == "style-" + style.identifier.lower()
            app.prompt_editor.force_close()
            settle(lambda: app.prompt_editor is None)
            # Saved style execution resolves a direct disk edit at click time.
            key = "style-" + style.identifier.lower()
            app.prompt_store.save(key, "External style instructions", None)
            with patch.object(workspace, "request_rewrite") as request:
                workspace.request_prompt(key)
                request.assert_called_once_with("External style instructions")
            app.config = replace(app.config, incognito_mode=True)
            app._open_prompt_editor("live-custom")
            editor = app.prompt_editor
            editor.editor.get_buffer().set_text("Private draft")
            assert not editor.save_button.get_sensitive()
            editor._save(None)
            assert not app.prompt_store.path("live-custom").exists()
            editor.force_close()
            settle(lambda: app.prompt_editor is None)
            if app.config_load_error:
                assert config_path.read_text() == "{malformed configuration"
            (output / "result.json").write_text(
                json.dumps(
                    {
                        "passed": True,
                        "display": os.environ["DISPLAY"],
                        "width": width,
                        "height": height,
                        "checks": (
                            "hover, keyboard focus, Ctrl+P deep link, save/cancel/reset, validation, "
                            "external conflict, "
                            "restart store, live freeze, manual draft, saved style, Incognito"
                        ),
                    },
                    indent=2,
                )
            )
        except Exception:
            errors.append(traceback.format_exc())
        finally:
            if native is not None:
                native.close()
            app._cancel_live_rewrite()
            app.quit()
        return GLib.SOURCE_REMOVE

    def activated(_app):
        app.disconnect(activation)
        GLib.timeout_add(300, exercise)

    activation = app.connect("activate", activated)
    with (
        patch("mluva_linux.app.PipeWireDeviceCatalog.from_system", return_value=PipeWireDeviceCatalog()),
        patch("mluva_linux.app.FocusedTextTargetTracker", return_value=None),
    ):
        app.run([])
    if errors:
        print("\n".join(errors), file=sys.stderr)
        raise RuntimeError("\n".join(errors))


if __name__ == "__main__":
    main()
