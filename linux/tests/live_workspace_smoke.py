"""Exercise editable and live documents in the production GTK app on a private display."""

import json
import os
import sys
import time
import traceback
from dataclasses import replace
from pathlib import Path
from unittest.mock import patch

from conversation_ui_smoke import IsolatedApplication
from gi.repository import Gdk, GLib, Graphene, Gtk

from voice_scribe_linux.codex_client import CodexAppServerClient
from voice_scribe_linux.delivery import DeliveryReceipt


def settle(predicate, timeout: float = 6) -> None:
    """Wait for a bounded asynchronous GTK completion while dispatching its actual callbacks."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        while GLib.MainContext.default().pending():
            GLib.MainContext.default().iteration(False)
        if predicate():
            return
        time.sleep(0.01)
    raise AssertionError("The fixture did not settle")


def paint(window, path):
    """Capture only after GTK has allocated and painted the edited widgets."""
    frames = []

    def tick(_widget, _clock):
        """Count two actual frame-clock updates before retaining pixels."""
        frames.append(True)
        return len(frames) < 3

    window.add_tick_callback(tick)
    settle(lambda: len(frames) >= 3)
    paintable = Gtk.WidgetPaintable.new(window)
    textures = []

    def capture():
        """Wait for a complete render node after a window resize invalidates the previous frame."""
        snapshot = Gtk.Snapshot()
        width, height = window.get_width(), window.get_height()
        paintable.snapshot(snapshot, width, height)
        node = snapshot.to_node()
        if node is None:
            window.queue_draw()
            return False
        viewport = Graphene.Rect().init(0, 0, width, height)
        textures.append(window.get_native().get_renderer().render_texture(node, viewport))
        return True

    settle(capture)
    textures[-1].save_to_png(str(path))


def main() -> int:
    """Cover editing, copy defaults, live races, finalization, cancellation and privacy."""
    if "OFFSCREEN_SESSION_ROOT" not in os.environ or os.environ.get("GDK_BACKEND") != "x11":
        raise RuntimeError("Use the isolated X11 runner")
    output = Path(os.environ["OFFSCREEN_ARTIFACT_DIR"])
    app = IsolatedApplication()
    errors = []
    copies = []
    fixture = Path(__file__).with_name("fake_app_server.py")

    def client(*_args, **_kwargs):
        """Use an independent JSONL subprocess, without authenticated provider access."""
        return CodexAppServerClient(command=(sys.executable, str(fixture), "--conversation"), turn_timeout_seconds=5)

    def copy(text, **_kwargs):
        """Record clipboard intentions without contacting any clipboard service."""
        copies.append(text)
        return DeliveryReceipt(True, False, "Copied")

    def exercise():
        """Operate real widgets and inspect saved records across request races."""
        try:
            workspace = app.conversation_workspace
            app.config = replace(app.config, auto_copy_rewrite=True)
            workspace.set_config(app.config)
            entry = app.history_store.add("Original speech", "Original speech", "dictation", "eng", None, "copied")
            workspace.show_conversation(entry, [])
            workspace.result_widgets[0].get_buffer().set_text("Corrected original")
            workspace.save_buttons[0].emit("clicked")
            assert app.conversation_store.source_text(entry) == "Corrected original"
            assert app.history_store.find(entry.identifier).raw_text == "Original speech"
            with (
                patch.object(app, "_new_rewrite_client", side_effect=client),
                patch("voice_scribe_linux.app.deliver_text", side_effect=copy),
            ):
                app._request_rewrite("Polish")
                assert copies == []
                settle(lambda: app.rewrite_client is None)
                first = app.conversation_store.replies(entry.identifier)[-1]
                assert first.text == "Corrected original\nPolish" and copies == [first.text]
                workspace.result_widgets[-1].get_buffer().set_text("My edited rewrite")
                app._request_rewrite("Shorten")
                settle(lambda: app.rewrite_client is None)
                assert copies[-1] == "My edited rewrite\nShorten"
            workspace.set_config(replace(app.config, show_copy_action=False, show_save_action=False))
            assert all(not button.get_visible() for button in workspace.copy_buttons + workspace.save_buttons)
            workspace.result_widgets[0].get_buffer().set_text("Keyboard-saved source")
            assert workspace._document_key(None, Gdk.KEY_s, 0, Gdk.ModifierType.CONTROL_MASK)
            assert app.conversation_store.source_text(entry) == "Keyboard-saved source"
            workspace.set_config(app.config)
            paint(app.window, output / "editable-documents.png")

            editor = workspace.result_widgets[0]
            long_paragraph = "České odstavce stay editable across many wrapped lines. " * 12
            editor.get_buffer().set_text((long_paragraph + "\n\n") * 30)
            paint(app.window, output / "long-editor.png")
            end = editor.get_iter_location(editor.get_buffer().get_end_iter())
            assert end.y + end.height <= editor.get_height(), (end.y, end.height, editor.get_height())
            editor.get_buffer().set_text("Keyboard-saved source")

            app.config = replace(app.config, live_rewrite_enabled=True, live_rewrite_min_characters=40)
            app.pending_session_identifier = "synthetic-live-session"
            app.pending_incognito = False
            app.pending_mode = "dictation"
            app._start_live_rewrite()
            transcript = "Build a clear task specification and preserve the existing settings."
            workspace.set_live("Recording · synthetic speech", transcript)

            def live_client(*_args, **_kwargs):
                """Return a structured fixture derived from the actual serialized live context."""
                return CodexAppServerClient(command=(sys.executable, str(fixture), "--live"), turn_timeout_seconds=5)

            count = len(copies)
            with (
                patch.object(app, "_new_rewrite_client", side_effect=live_client),
                patch("voice_scribe_linux.app.deliver_text", side_effect=copy),
            ):
                app._maybe_live_rewrite(transcript)
                buffer = workspace.live_draft_text.get_buffer()
                buffer.insert(buffer.get_end_iter(), "\nDeliberate manual edit")
                settle(lambda: app.live_rewrite_client is None)
                assert "Deliberate manual edit" in workspace.live_draft()
                assert "Dictated details" not in workspace.live_draft()
                assert len(copies) == count
                app.live_schedule.last_started = float("-inf")
                app._maybe_live_rewrite(transcript + " Keep the original transcript for recovery.")
                settle(lambda: app.live_rewrite_client is None)
                assert "Deliberate manual edit" in workspace.live_draft()
                assert "Dictated details" in workspace.live_draft()
                assert "[Missing:" in workspace.live_draft()
                assert len(copies) == count
                settle(lambda: app.window.get_mapped())
                paint(app.window, output / "live-rewrite.png")
                app.window.set_default_size(420, 520)
                paint(app.window, output / "live-rewrite-narrow.png")
                assert 400 <= app.window.get_width() <= 420
                assert workspace.live_box.get_orientation().value_nick == "vertical"
                app.window.set_default_size(1060, 780)
                final_entry = app.history_store.add(transcript, transcript, "dictation", "eng", None, "copied")
                app.live_final_entry = final_entry.identifier
                app.live_final_text = transcript + " Final tail: verify restoration after restart."
                app._maybe_live_rewrite(app.live_final_text, final=True)
                settle(lambda: app.live_schedule is None)
                saved = app.conversation_store.replies(final_entry.identifier)
                assert len(saved) == 1 and "Final tail" in saved[0].text
                assert copies[-1] == saved[0].text and len(copies) == count + 1
                app.pending_session_identifier = "cancelled-live-session"
                app._start_live_rewrite()
                app._maybe_live_rewrite(transcript)
                app._cancel_live_rewrite()
                settle(lambda: app.live_rewrite_client is None)
                app.config = replace(app.config, incognito_mode=True)
                app.pending_incognito = True
                app._start_live_rewrite()
                assert app.live_schedule is None
            app.settings_dialog.set_visible_page_name("providers")
            app._show_settings(None)
            assert app.workspace_settings_pages[0].fields["live_rewrite_enabled"]() == app.config.live_rewrite_enabled
            paint(app.window, output / "provider-settings.png")
            app.settings_dialog.close()
            (output / "live-workspace.json").write_text(
                json.dumps(
                    {
                        "editable_source": True,
                        "editable_rewrite": True,
                        "raw_recovery_preserved": True,
                        "complete_long_editor_height": True,
                        "automatic_copy": True,
                        "manual_edit_race_preserved": True,
                        "live_template_gaps": True,
                        "one_final_saved_draft": True,
                        "cancel_and_incognito": True,
                    },
                    indent=2,
                )
            )
        except Exception:
            errors.append(traceback.format_exc())
        app.quit()
        return GLib.SOURCE_REMOVE

    app.connect("activate", lambda _app: GLib.timeout_add(500, exercise))
    app.run([])
    if errors:
        raise RuntimeError("\n".join(errors))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
