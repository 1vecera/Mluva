"""Exercise continuation, three Live modes and thinking controls in a private native app."""

import os
import sys
import traceback
from dataclasses import replace
from pathlib import Path
from unittest.mock import patch

from conversation_ui_smoke import IsolatedApplication
from gi.repository import GLib
from live_workspace_smoke import paint, settle

from mluva_linux.codex_client import CodexAppServerClient
from mluva_linux.config import load_config
from mluva_linux.delivery import DeliveryReceipt
from mluva_linux.elevenlabs import TranscriptionResult
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
                settle(lambda: app.recorder.process is not None)
                assert not app.capture_allows_auto_paste
                assert not workspace.continue_button.get_sensitive()
                app._toggle_recording(app.record_button)
                settle(lambda: not app.capture_processing)
                assert workspace.entry.identifier == source.identifier
                assert app.conversation_store.source_text(source) == "My edited original.\n\nMore words."
                assert workspace.result_widgets[-1].get_text() == "My edited rewrite.\n\nMore words."
                assert copies == ["My edited rewrite.\n\nMore words."]
                assert len(app.history_store.recent()) == 2
                assert len(app.conversation_store.search()) == 1

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
            (output / "result.txt").write_text(
                "PASS: append/edit/raw recovery, widget continuation, cancellation, "
                "Once/Continuous, thinking, narrow layout\n"
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
