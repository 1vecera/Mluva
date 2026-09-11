"""Exercise editable and live documents in the production GTK app on a private display."""

import json
import os
import sys
import threading
import time
import traceback
from dataclasses import replace
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

from conversation_ui_smoke import IsolatedApplication
from gi.repository import Gdk, GLib, Graphene, Gtk

from mluva_linux.codex_client import CodexAppServerClient
from mluva_linux.delivery import DeliveryReceipt
from mluva_linux.elevenlabs import TranscriptionResult
from mluva_linux.realtime import RealtimePreview
from mluva_linux.workflow import WorkflowResult


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


def exercise_finalization_panel(app, output: Path) -> None:
    """Hold a real workflow completion at the provider boundary and inspect every GTK frame."""
    workspace = app.conversation_workspace
    app.config = replace(app.config, live_rewrite_enabled=True, auto_copy_rewrite=True)
    workspace.set_config(app.config)
    gate = threading.Event()
    contexts, copies, overlays, geometry = [], [], [], []
    fixture = Path(__file__).with_name("fake_app_server.py")
    source = "\n".join(f"Keep the original dictated requirement {index}." for index in range(50))
    draft = "\n".join(f"Editable draft requirement {index}." for index in range(50))

    class HeldClient(CodexAppServerClient):
        """Delay only the synthetic provider, preserving the real app's finalization callbacks."""

        def transform(self, prompt, *args, **kwargs):
            """Expose the request before allowing its independent JSONL provider to reply."""
            contexts.append(json.loads(prompt.split("\n", 1)[1]))
            if not gate.wait(10):
                raise TimeoutError("The test did not release final reconciliation")
            return super().transform(prompt, *args, **kwargs)

    def client(*_args, **_kwargs):
        """Start only the repository's unauthenticated local provider fixture."""
        return HeldClient(command=(sys.executable, str(fixture), "--live"), turn_timeout_seconds=5)

    def sample(_widget=None, _clock=None):
        """Measure the mapped Live panel and native scrollbar while finalization is held."""
        success, bounds = workspace.live_box.compute_bounds(app.window)
        geometry.append(
            {
                "mapped": workspace.live_box.get_mapped() and workspace.live_draft_text.get_mapped(),
                "bounds": [bounds.get_x(), bounds.get_y(), bounds.get_width(), bounds.get_height()] if success else [],
                "reading_position": workspace.live_draft_scroll.get_vadjustment().get_value(),
                "original_reading_position": workspace.live_scroll.get_vadjustment().get_value(),
            }
        )
        return GLib.SOURCE_CONTINUE

    def start(identifier):
        """Enter recording through the production Live session and provide committed recognition at Stop."""
        app.pending_session_identifier = identifier
        app.pending_incognito = False
        app.pending_mode = "dictation"
        app._start_live_rewrite()
        workspace.set_live("Recording  00:40", source + " PROVISIONAL_ONLY")
        workspace.show_live_draft(draft)
        paint(app.window, output / f"{identifier}-recording.png")
        workspace.live_draft_scroll.get_vadjustment().set_value(80)
        workspace.live_scroll.get_vadjustment().set_value(90)
        entry = app.history_store.add(source, source, "dictation", "eng", None, "ready")
        result = WorkflowResult(
            transcription=TranscriptionResult(source, "eng", None, None),
            output_text=source,
            delivery=DeliveryReceipt(False, False, "Dictation ready."),
            history_entry=entry,
            retained_audio_path=None,
            requires_acceptance=False,
            incognito=False,
            mode="dictation",
            recognition_ms=0,
            enhancement_ms=0,
            delivery_ms=0,
            session_identifier=identifier,
            recognition_fallback=False,
            recognition_route="scribe-v2-realtime",
            recognition_fallback_reason=None,
        )
        sample()
        app.capture_processing = True
        app._publish_completion_status("processing", "Finishing your dictation…")
        assert overlays[-1].preview == source + " PROVISIONAL_ONLY"
        overlays.clear()
        app._workflow_finished(result)
        return entry

    publisher = SimpleNamespace(
        publish=lambda state: overlays.append(state) or True, clear=lambda: overlays.append(None)
    )
    with (
        patch.object(app, "_new_rewrite_client", side_effect=client),
        patch.object(app, "recording_overlay_publisher", publisher),
        patch("mluva_linux.app.deliver_text", side_effect=lambda text, **_kw: copies.append(text)),
    ):
        try:
            entry = start("final-panel")
            settle(lambda: bool(contexts))
            tick = app.window.add_tick_callback(sample)
            settle(lambda: len(geometry) >= 30)
            app.window.remove_tick_callback(tick)
            (output / "final-panel-geometry.json").write_text(json.dumps(geometry, indent=2))
            assert all(frame["mapped"] for frame in geometry), "Live editor disappeared during final reconciliation"
            assert len({tuple(frame["bounds"]) for frame in geometry}) == 1, geometry
            assert all(abs(frame["reading_position"] - 80) < 1 for frame in geometry), geometry
            assert all(abs(frame["original_reading_position"] - 90) < 1 for frame in geometry), geometry
            assert workspace.live_cancel.get_mapped()
            assert all(state is not None and state.phase == "rewriting" for state in overlays), overlays
            app._review_action(None, GLib.Variant("(sss)", ("copy", entry.identifier, "")))
            assert copies == []
            paint(app.window, output / "final-panel-pending.png")
            buffer = workspace.live_draft_text.get_buffer()
            buffer.insert(buffer.get_end_iter(), "\nManual edit while finalizing")
            gate.set()
            settle(lambda: app.live_schedule is None)
            saved = app.conversation_store.replies(entry.identifier)
            assert len(saved) == 1 and "Manual edit while finalizing" in saved[0].text
            assert copies == [saved[0].text] and "PROVISIONAL_ONLY" not in contexts[-1]["transcript"]
            assert app.history_store.find(entry.identifier).raw_text == source
            assert not workspace.live_box.get_visible() and overlays[-1].phase == "ready"
            paint(app.window, output / "final-panel-saved.png")

            for failure in ("save", "copy"):
                gate.clear()
                contexts.clear()
                entry = start(f"{failure}-panel")
                settle(lambda: bool(contexts))
                target = (
                    patch.object(type(app.conversation_store), "append", side_effect=OSError("Controlled save failure"))
                    if failure == "save"
                    else patch("mluva_linux.app.deliver_text", side_effect=RuntimeError("Controlled copy failure"))
                )
                with target:
                    gate.set()
                    settle(lambda: app.live_schedule is None)
                assert not workspace.live_box.get_visible() and workspace.scroll.get_visible()
                assert (
                    "Could not save" if failure == "save" else "Automatic copy failed"
                ) in workspace.notice.get_label()
                assert len(app.conversation_store.replies(entry.identifier)) == (0 if failure == "save" else 1)
                assert len(workspace.copy_buttons) == 2 and all(
                    button.get_sensitive() for button in workspace.copy_buttons
                )
                assert len(copies) == 1 and app.history_store.find(entry.identifier).raw_text == source
                assert overlays[-1].phase == ("review-error" if failure == "save" else "ready")

            gate.clear()
            contexts.clear()
            entry = start("cancel-panel")
            settle(lambda: bool(contexts))
            pending, session, revision = app.live_rewrite_client, app.live_session_identifier, app.live_revision
            workspace.live_cancel.emit("clicked")
            assert app.live_schedule is None and not workspace.live_box.get_visible()
            assert overlays[-1].message == "Live rewrite cancelled. Original kept."
            gate.set()
            app._live_rewrite_finished(session, pending, revision, "Late cancelled result", "fake")
            assert app.conversation_store.replies(entry.identifier) == [] and len(copies) == 1
            assert app.history_store.find(entry.identifier).raw_text == source
        finally:
            gate.set()


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
            exercise_finalization_panel(app, output)
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
                patch("mluva_linux.app.deliver_text", side_effect=copy),
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

            app.config = replace(
                app.config, live_rewrite_enabled=True, live_rewrite_min_characters=40, live_rewrite_template="task-spec"
            )
            app.pending_session_identifier = "synthetic-live-session"
            app.pending_incognito = False
            app.pending_mode = "dictation"
            app._start_live_rewrite()
            transcript = "Build a clear task specification and preserve the existing settings."
            workspace.set_live("Recording · synthetic speech", transcript)
            live_contexts = []

            class TrackingLiveClient(CodexAppServerClient):
                """Observe real serialized requests at the independent provider transport boundary."""

                def transform(self, prompt, *args, **kwargs):
                    """Retain synthetic context so provisional input and final reconciliation can be distinguished."""
                    live_contexts.append(json.loads(prompt.split("\n", 1)[1]))
                    return super().transform(prompt, *args, **kwargs)

            def live_client(*_args, **_kwargs):
                """Return a structured fixture derived from the actual serialized live context."""
                return TrackingLiveClient(command=(sys.executable, str(fixture), "--live"), turn_timeout_seconds=5)

            count = len(copies)
            with (
                patch.object(app, "_new_rewrite_client", side_effect=live_client),
                patch("mluva_linux.app.deliver_text", side_effect=copy),
            ):
                callback = app._live_preview_callback(app.live_session_identifier)
                callback(RealtimePreview("", transcript + " PROVISIONAL_ONLY"))
                settle(lambda: app.live_rewrite_client is not None)
                buffer = workspace.live_draft_text.get_buffer()
                buffer.insert(buffer.get_end_iter(), "\nDeliberate manual edit")
                settle(lambda: app.live_rewrite_client is None)
                assert "Deliberate manual edit" in workspace.live_draft()
                assert "Dictated details" not in workspace.live_draft()
                assert live_contexts[0]["transcript"].endswith("PROVISIONAL_ONLY")
                assert live_contexts[0]["transcript_status"].startswith("provisional")
                assert len(copies) == count
                app.live_schedule.last_started = float("-inf")
                app._maybe_live_rewrite(transcript + " Keep the original transcript for recovery.")
                settle(lambda: app.live_rewrite_client is None)
                assert "Deliberate manual edit" in workspace.live_draft()
                assert "Dictated details" in workspace.live_draft()
                assert "[Missing:" in workspace.live_draft()
                assert "provisional until Stop" in workspace.live_draft_status.get_label()
                assert len(copies) == count
                settle(lambda: app.window.get_mapped())
                paint(app.window, output / "live-rewrite.png")
                app.window.set_default_size(420, 520)
                paint(app.window, output / "live-rewrite-narrow.png")
                assert 400 <= app.window.get_width() <= 420
                assert workspace.live_box.get_orientation().value_nick == "vertical"
                app.window.set_default_size(1060, 780)
                final_entry = app.history_store.add(transcript, transcript, "dictation", "eng", None, "copied")
                app.live_schedule.last_started = float("-inf")
                app._maybe_live_rewrite(transcript + " One update is still running at Stop.")
                assert app.live_rewrite_client is not None
                app.live_final_entry = final_entry.identifier
                app.live_final_text = transcript + " Final tail: verify restoration after restart."
                app._maybe_live_rewrite(app.live_final_text, final=True)
                buffer.insert(buffer.get_end_iter(), "\nManual edit during finalization")
                settle(lambda: app.live_schedule is None)
                saved = app.conversation_store.replies(final_entry.identifier)
                assert len(saved) == 1 and "Final tail" in saved[0].text
                assert "Manual edit during finalization" in saved[0].text
                assert app.history_store.find(final_entry.identifier).raw_text == transcript
                assert live_contexts[-1]["transcript_status"] == "final committed recognition"
                assert live_contexts[-1]["transcript"] == app.live_final_text
                assert "PROVISIONAL_ONLY" not in live_contexts[-1]["transcript"]
                assert "Manual edit during finalization" in live_contexts[-1]["current_draft"]
                assert copies[-1] == saved[0].text and len(copies) == count + 1
                app._maybe_live_rewrite(app.live_final_text, final=True)
                assert len(app.conversation_store.replies(final_entry.identifier)) == 1
                app.pending_session_identifier = "cancelled-live-session"
                app._start_live_rewrite()
                app._maybe_live_rewrite(transcript)
                cancelled_client = app.live_rewrite_client
                cancelled_session = app.live_session_identifier
                app._cancel_live_rewrite()
                app._live_rewrite_finished(
                    cancelled_session, cancelled_client, app.live_revision, "Late result", "fake"
                )
                assert not workspace.live_draft_box.get_visible()
                assert len(copies) == count + 1
                app.pending_session_identifier = "failed-live-session"
                app._start_live_rewrite()
                with patch.object(app, "_new_rewrite_client", side_effect=RuntimeError("Controlled provider failure")):
                    app._maybe_live_rewrite(transcript)
                assert app.live_schedule.failed
                failed_entry = app.history_store.add(transcript, transcript, "dictation", "eng", None, "copied")
                app.live_final_entry = failed_entry.identifier
                app._maybe_live_rewrite(transcript, final=True)
                failed = app.conversation_store.replies(failed_entry.identifier)
                assert len(failed) == 1 and "partial draft" in failed[0].instruction
                assert len(copies) == count + 1
                app.pending_session_identifier = "matching-final-session"
                app._start_live_rewrite()
                same_text = "The final recognized words match the provisional preview."
                app._maybe_live_rewrite(same_text)
                settle(lambda: app.live_rewrite_client is None)
                final_entry = app.history_store.add(same_text, same_text, "dictation", "eng", None, "copied")
                app.live_final_entry = final_entry.identifier
                app.live_final_text = same_text
                requests_before_stop = len(live_contexts)
                app._maybe_live_rewrite(app.live_final_text, final=True)
                settle(lambda: app.live_schedule is None)
                assert len(live_contexts) == requests_before_stop + 1
                assert live_contexts[-1]["transcript_status"] == "final committed recognition"
                assert len(app.conversation_store.replies(final_entry.identifier)) == 1
                assert len(copies) == count + 2
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
                        "manual_edit_during_finalization": True,
                        "provisional_input_never_used_as_raw": True,
                        "matching_final_still_reconciled": True,
                        "failed_final_never_copied": True,
                        "cancelled_late_result_discarded": True,
                        "cancel_and_incognito": True,
                    },
                    indent=2,
                )
            )
        except Exception:
            errors.append(traceback.format_exc())
        app.quit()
        return GLib.SOURCE_REMOVE

    def activate_once(_app):
        """Opening Settings reactivates the app; it must not start a second nested test run."""
        app.disconnect(activation)
        GLib.timeout_add(500, exercise)

    activation = app.connect("activate", activate_once)
    app.run([])
    if errors:
        raise RuntimeError("\n".join(errors))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
