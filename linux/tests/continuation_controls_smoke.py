"""Exercise continuation, three Live modes and thinking controls in a private native app."""

import os
import sys
import traceback
from dataclasses import replace
from pathlib import Path
from unittest.mock import patch

from conversation_ui_smoke import IsolatedApplication
from gi.repository import Adw, GLib, Gtk
from live_workspace_smoke import paint, settle

from mluva_linux.codex_client import CodexAppServerClient
from mluva_linux.config import load_config
from mluva_linux.delivery import DeliveryReceipt
from mluva_linux.elevenlabs import TranscriptionResult
from mluva_linux.realtime import RealtimePreview, RealtimeSessionResult
from mluva_linux.shell_bridge import project_state
from mluva_linux.workflow import DictationWorkflow


class SyntheticRecorder:
    """Replace microphone IO while running the production capture and completion lifecycle."""

    process = None
    audio_level = 0
    path = None

    def start(self, path, _callback=None):
        """Create an owner-local audio stand-in without touching sound devices."""
        self.path = path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b"synthetic audio")
        self.process = object()

    def stop(self):
        """Hand the same finalized path to the real workflow."""
        self.process = None
        return self.path

    def cancel(self):
        """Erase only this fixture's current audio."""
        self.process = None
        if self.path:
            self.path.unlink(missing_ok=True)


class SyntheticSpeech:
    """Return a complete transcript without uploading audio."""

    def transcribe(self, *_args, **_kwargs):
        """Add predictable words for each continuation."""
        return TranscriptionResult("More words.", "eng", None, None)


class SyntheticPreview:
    """Supply changing recognition snapshots without a microphone or provider connection."""

    is_healthy = True
    bytes_sent = 0
    text = ""

    def snapshot(self):
        """Keep each provisional revision separate from the previous saved speech."""
        return RealtimePreview("", self.text)

    def finish(self):
        """Finalize only this recording's words."""
        return RealtimeSessionResult(TranscriptionResult(self.text, "eng", None, None), 0.01)

    def cancel(self):
        """Accept cancellation without touching a device."""


def descendants(widget):
    """Inspect the actual nested history controls and their displayed document."""
    yield widget
    child = widget.get_first_child()
    while child is not None:
        yield from descendants(child)
        child = child.get_next_sibling()


def main():
    """Verify the actual GTK controls, SQLite writes and asynchronous completion gates."""
    if "OFFSCREEN_SESSION_ROOT" not in os.environ:
        raise RuntimeError("Use dev/run-isolated.sh")
    app = IsolatedApplication()
    output = Path(os.environ["OFFSCREEN_ARTIFACT_DIR"])
    errors, copies = [], []
    sys.excepthook = lambda *error: errors.append("".join(traceback.format_exception(*error)))

    def exercise():
        try:
            workspace = app.conversation_workspace
            source_updates = []
            workspace.live_text.get_buffer().connect(
                "changed", lambda _buffer: source_updates.append(workspace.live_text.get_text())
            )
            app.config = replace(app.config, auto_copy_dictation=True, auto_copy_rewrite=False)
            app.recorder = SyntheticRecorder()
            app.workflow = DictationWorkflow(
                app.config, SyntheticSpeech(), None, app.history_store, app.codex_workspace
            )
            app.cleanup_switch.set_active(False)
            source = app.history_store.add("First raw.", "First.", "dictation", "eng", None, "copied")
            reply = app.conversation_store.append(source.identifier, "Polish", "First polished.", "fixture")
            workspace.show_conversation(source, [reply])
            workspace.edit_drafts[(source.identifier, None)] = "My edited original."
            workspace.edit_drafts[(source.identifier, reply.identifier)] = "My edited rewrite."
            assert workspace.continue_button.get_visible()

            def prepare(session, path, *_args):
                """Signal readiness through the real main-thread capture callback."""
                GLib.idle_add(app._capture_prepared, session, path, None, None, None, None, None, 0.01)

            def client(_config=None, **_kwargs):
                """Keep rewriting and model discovery on a local protocol fixture."""
                return CodexAppServerClient(
                    command=(sys.executable, str(Path(__file__).with_name("fake_app_server.py")))
                )

            def copy(text, **kwargs):
                """Observe delivery without changing even the private session's clipboard."""
                assert kwargs["auto_paste"] is False
                copies.append(text)
                return DeliveryReceipt(True, False, "Copied.")

            with (
                patch.object(app, "_prepare_capture", side_effect=prepare),
                patch.object(app, "_configure_realtime_provider"),
                patch.object(app, "_new_rewrite_client", side_effect=client),
                patch("mluva_linux.app.deliver_text", side_effect=copy),
                patch("mluva_linux.workflow.deliver_text", side_effect=AssertionError("Segment delivered too early")),
            ):
                workspace.continue_button.emit("clicked")
                assert workspace.live_text.get_text() == "My edited original.", (
                    "Continue must retain the original while preparing the next recording"
                )
                assert project_state(app.recording_overlay_publisher._shell_parameters, True)["preview"] == (
                    "My edited original."
                )
                settle(lambda: app.recorder.process is not None)
                assert workspace.live_text.get_text() == "My edited original.", (
                    "Batch-only capture must keep the original visible while waiting for speech"
                )
                assert not app.capture_allows_auto_paste
                assert not workspace.continue_button.get_sensitive()
                preview = SyntheticPreview()
                app.realtime_session = preview
                app.pending_realtime_fallback_reason = None
                for words in ("", "More", "More words."):
                    preview.text = words
                    app._update_capture_status()
                    expected = "My edited original." + ("\n\n" + words if words else "")
                    assert workspace.live_text.get_text() == expected
                    assert project_state(app.recording_overlay_publisher._shell_parameters, True)["preview"] == (
                        " ".join(expected.split())
                    )
                paint(app.window, output / "continue-recording.png")
                app._toggle_recording(app.record_button)
                assert app.capture_processing
                assert workspace.live_text.get_text() == "My edited original.\n\nMore words."
                assert project_state(app.recording_overlay_publisher._shell_parameters, True)["preview"] == (
                    "My edited original. More words."
                )
                settle(lambda: not app.capture_processing)
                assert workspace.entry.identifier == source.identifier
                assert app.conversation_store.source_text(source) == "My edited original.\n\nMore words."
                assert workspace.result_widgets[-1].get_text() == "My edited rewrite.\n\nMore words."
                assert copies == ["My edited rewrite.\n\nMore words."]
                assert len(app.history_store.recent()) == 2
                assert len(app.conversation_store.search()) == 1
                assert list(app.history_page.entry_rows) == [source.identifier]
                history_row = app.history_page.entry_rows[source.identifier]
                assert history_row.get_subtitle() == "Dictation · 2 recordings"
                current = next(
                    widget
                    for widget in descendants(history_row)
                    if isinstance(widget, Adw.ActionRow) and widget.get_title() == "Current text"
                )
                assert current.get_subtitle() == "My edited rewrite.\n\nMore words."
                with patch.object(app.history_page, "copy_text") as history_copy:
                    next(
                        widget
                        for widget in descendants(current)
                        if isinstance(widget, Gtk.Button) and widget.get_label() == "Copy"
                    ).emit("clicked")
                    history_copy.assert_called_once_with("My edited rewrite.\n\nMore words.")
                app.history_page.focus_entry(app.history_store.continuations(source.identifier)[0].identifier)
                assert app.history_page.focused_identifier == source.identifier
                app._navigate_to_page("history")
                paint(app.window, output / "continued-history.png")
                recordings = next(
                    widget
                    for widget in descendants(app.history_page.entry_rows[source.identifier])
                    if isinstance(widget, Adw.ExpanderRow) and widget.get_title() == "Original recordings"
                )
                recordings.set_expanded(True)
                raw_rows = [
                    widget
                    for widget in descendants(recordings)
                    if isinstance(widget, Adw.ActionRow) and widget.get_title() == "Raw transcript"
                ]
                assert {row.get_subtitle() for row in raw_rows} == {"First raw.", "More words."}
                app._navigate_to_page("capture")

                app.live_mode_switch.set_active(True)
                assert app.config.live_rewrite_enabled and not app.config.live_rewrite_continuous
                app._publish_review(source.identifier)
                app._review_action(None, GLib.Variant("(sss)", ("continue", source.identifier, "")))
                settle(lambda: app.recorder.process is not None)
                assert app.live_schedule is not None
                assert workspace.live_draft() == "My edited rewrite.\n\nMore words."
                app._toggle_recording(app.record_button)
                settle(lambda: not app.capture_processing and app.live_schedule is None)
                assert not load_config(app.config_path).live_rewrite_enabled
                assert workspace.entry.identifier == source.identifier
                assert app.conversation_store.replies(source.identifier)[-1].text == "Clean text."
                assert app.conversation_store.source_text(source).endswith("More words.\n\nMore words.")
                assert app.history_store.find(source.identifier).raw_text == "First raw."
                assert all(not text or text.startswith("My edited original.") for text in source_updates), (
                    "Preparation, recording and final reconciliation must never replace the old text with a new segment"
                )
                assert list(app.history_page.entry_rows) == [source.identifier]

                app.live_mode_switch.set_active(True)
                app.live_mode_switch.set_active(False)
                assert app.config.live_rewrite_enabled and app.config.live_rewrite_continuous
                workspace.continue_button.emit("clicked")
                settle(lambda: app.recorder.process is not None)
                app._cancel_capture()
                assert app.config.live_rewrite_enabled and app.config.live_rewrite_continuous
                assert len(app.history_store.recent()) == 3
                app.live_mode_switch.set_active(False)
                assert not app.config.live_rewrite_enabled

                app._load_rewrite_models()
                settle(lambda: app.model_catalog_client is None)
                picker = app.rewrite_settings
                assert picker.thinking_row.choices == [None, "low", "high"]
                app.config = replace(app.config, rewrite_model="codex-default")
                picker.set_config(app.config)
                picker.thinking_row.set_selected(2)
                assert load_config(app.config_path).rewrite_reasoning_effort == "high"
                picker.model_row.set_selected(picker.choices.index("gpt-5.4-mini"))
                assert app.config.rewrite_reasoning_effort is None
                assert picker.thinking_row.choices == [None, "low"]
                with patch("mluva_linux.app.save_config", side_effect=OSError("Synthetic failure")):
                    picker.thinking_row.set_selected(1)
                assert picker.thinking_row.get_selected() == 0

                for width, height, name in ((1060, 780, "desktop"), (420, 520, "narrow")):
                    app.window.set_size_request(420, 520)
                    app.window.set_default_size(width, height)
                    workspace.show_conversation(source, app.conversation_store.replies(source.identifier))
                    app.live_mode_switch.set_active(True)
                    app.live_mode_switch.set_active(False)
                    paint(app.window, output / f"continue-{name}.png")
                    for widget in (workspace.continue_button, app.live_mode_switch, app.record_button):
                        success, bounds = widget.compute_bounds(app.window)
                        assert success and bounds.get_x() >= 0
                        assert bounds.get_x() + bounds.get_width() <= app.window.get_width()
                    app.live_mode_switch.set_active(False)
                picker.popup()
                paint(app.window, output / "thinking.png")
                picker.popdown()
                plain = app.history_store.add("First sentence.", "First sentence.", "dictation", "eng", None, "copied")
                for _ in range(2):
                    workspace.show_conversation(source, app.conversation_store.replies(source.identifier))
                    app._publish_review(plain.identifier)
                    app._review_action(None, GLib.Variant("(sss)", ("continue", plain.identifier, "")))
                    assert workspace.entry.identifier == plain.identifier
                    assert workspace.live_text.get_text() == app.conversation_store.source_text(plain)
                    settle(lambda: app.recorder.process is not None)
                    app._toggle_recording(app.record_button)
                    settle(lambda: not app.capture_processing)
                combined = "First sentence.\n\nMore words.\n\nMore words."
                assert workspace.result_widgets[-1].get_text() == combined
                assert copies[-1] == combined
                assert len(app.history_page.entry_rows) == 2
                assert len(app.conversation_store.search()) == 2
                workspace.continue_button.emit("clicked")
                assert workspace.live_text.get_text() == combined
                app._cancel_capture()
                assert workspace.entry.identifier == plain.identifier
                assert workspace.result_widgets[-1].get_text() == combined
                app.live_mode_switch.set_active(True)
                workspace.continue_button.emit("clicked")
                settle(lambda: app.recorder.process is not None)
                with patch.object(
                    SyntheticSpeech, "transcribe", return_value=TranscriptionResult("", "eng", None, None)
                ):
                    app._toggle_recording(app.record_button)
                    settle(lambda: not app.capture_processing)
                assert workspace.entry.identifier == plain.identifier, (
                    "An empty continuation must keep its conversation"
                )
                assert workspace.result_widgets[-1].get_text() == combined
                assert len(app.history_page.entry_rows) == 2
                app._toggle_recording(app.record_button)
                assert "First sentence." not in workspace.live_text.get_text(), "New dictation must start fresh"
                app._cancel_capture()
            (output / "result.txt").write_text(
                "PASS: visible append through preparation/recording/finalization, grouped History and copy, "
                "repeated widget continuation, cancellation, raw recovery, Once/Continuous, thinking, narrow layout\n"
            )
        except Exception:
            errors.append(traceback.format_exc())
        finally:
            app._cancel_live_rewrite()
            app.recorder = None
            app.workflow = None
            app.quit()
        return GLib.SOURCE_REMOVE

    def activated(_app):
        app.disconnect(activation)
        GLib.timeout_add(300, exercise)

    activation = app.connect("activate", activated)
    app.run([])
    sys.excepthook = sys.__excepthook__
    if errors:
        raise RuntimeError("\n".join(errors))


if __name__ == "__main__":
    main()
