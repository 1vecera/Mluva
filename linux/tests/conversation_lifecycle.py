"""Exercise production conversation callbacks with private stores and a real synthetic JSONL subprocess."""

import json
import sys
import time
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import gi

from voice_scribe_linux.app import MluvaApplication
from voice_scribe_linux.codex_client import CodexAppServerClient
from voice_scribe_linux.conversation import QUICK_POLISH
from voice_scribe_linux.delivery import DeliveryReceipt

gi.require_version("Gtk", "4.0")
from gi.repository import GLib, Gtk  # noqa: E402


def exercise(application: MluvaApplication) -> None:
    """Verify draft races, contextual replies, cancellation, privacy, export and background delivery."""
    workspace = application.conversation_workspace
    copies: list[str] = []
    workspace.copy_text = copies.append
    source = application.history_store.add("Lifecycle source", "Lifecycle source", "dictation", "eng", None, "copied")
    workspace.show_conversation(source, [])
    client_factory = lambda: CodexAppServerClient(  # noqa: E731
        command=(sys.executable, str(Path(__file__).with_name("fake_app_server.py")), "--conversation")
    )

    def settle() -> None:
        """Run GTK completions while bounding the synthetic provider wait."""
        deadline = time.monotonic() + 10
        while application.rewrite_client is not None and time.monotonic() < deadline:
            GLib.MainContext.default().iteration(False)
            time.sleep(0.01)
        assert application.rewrite_client is None, "Synthetic rewrite timed out"

    with patch("voice_scribe_linux.app.CodexAppServerClient", side_effect=client_factory):
        workspace.prompt.get_buffer().set_text("A follow-up typed before Quick Polish")
        application._request_rewrite(QUICK_POLISH)
        settle()
        assert workspace.prompt_text() == "A follow-up typed before Quick Polish"
        workspace.prompt.get_buffer().set_text("A follow-up typed while rewriting")
        first = application.conversation_store.replies(source.identifier)
        assert first[0].text == source.raw_text + "\n" + QUICK_POLISH
        workspace._submit(workspace.send)
        settle()
        replies = application.conversation_store.replies(source.identifier)
        assert replies[-1].text == first[0].text + "\nA follow-up typed while rewriting"
        assert workspace.prompt_text() == ""
        assert copies == []
        workspace.messages.get_last_child().get_first_child().get_last_child().emit("clicked")
        assert copies == [replies[-1].text]
        application._request_rewrite("Cancelled request")
        application._cancel_rewrite()
        time.sleep(0.3)
        while GLib.MainContext.default().pending():
            GLib.MainContext.default().iteration(False)
        assert len(application.conversation_store.replies(source.identifier)) == 2

    workspace.prompt.get_buffer().set_text("Keep this draft")
    workspace.show_conversation(None, [])
    workspace.prompt.get_buffer().set_text("Unsaved pasted source")
    workspace.show_conversation(source, replies)
    assert workspace.prompt_text() == "Keep this draft"
    application.history_store.correct_delivered_text(source.identifier, "Corrected source")
    application._history_changed()
    assert workspace.entry.delivered_text == "Corrected source"
    assert workspace.prompt_text() == "Keep this draft"
    newer = [application.history_store.add("Newer", "Newer", "dictation", "eng", None, "copied") for _ in range(101)]
    application._open_history()
    assert source.identifier in application.history_page.entry_rows
    assert application.history_page.entry_rows[source.identifier].get_expanded()
    application.history_page._save_title(None, source, Gtk.Entry(text="Renamed conversation"))
    assert workspace.entry.title == "Renamed conversation"
    assert application.conversation_store.search("Renamed conversation")[0].identifier == source.identifier
    for entry in newer:
        application.history_store.delete(entry.identifier)
    application._navigate_to_page("capture")

    for export_format in ("json", "markdown"):
        application.history_page._export(None, workspace.entry, export_format)
        extension = "json" if export_format == "json" else "md"
        exported = next(application.history_page.export_directory.glob(f"*{source.identifier[:8]}.{extension}"))
        text = exported.read_text()
        assert source.raw_text in text and "A follow-up typed while rewriting" in text
        if export_format == "json":
            assert len(json.loads(text)["rewrites"]) == 2

    workspace.show_conversation(None, [])
    assert workspace.prompt_text() == "Unsaved pasted source"
    states = []
    application.recording_overlay_publisher = SimpleNamespace(
        publish=lambda state: states.append(state) or True, clear=lambda: True
    )
    for receipt in (
        DeliveryReceipt(True, False, "Copied. Paste in the target application."),
        DeliveryReceipt(True, True, "Inserted once.", True, True),
        DeliveryReceipt(True, False, "Inspect the target before any manual retry.", True, None),
    ):
        application._workflow_finished(
            SimpleNamespace(
                delivery=receipt,
                requires_acceptance=False,
                mode="dictation",
                history_entry=source,
                output_text=source.raw_text,
                recognition_ms=1,
                enhancement_ms=1,
                delivery_ms=1,
            )
        )
        assert workspace.entry is None and workspace.prompt_text() == "Unsaved pasted source"
        assert receipt.guidance in application.status_label.get_label()
    assert states[-1].detail == "Paste unconfirmed. Check the target before pasting again."

    audio_path = application.codex_workspace / "never-recorded.wav"
    application.pending_session_identifier = "failed-start"
    application.audio_path = audio_path
    application._capture_preparation_failed("failed-start", audio_path, "Synthetic preparation failure", 0)
    assert states[-1].phase == "error"
    application.pending_session_identifier = "failed-mic"
    application.audio_path = audio_path
    application.capture_preparing = True
    with patch.object(
        application,
        "recorder",
        SimpleNamespace(
            start=lambda *_args: (_ for _ in ()).throw(OSError("Fixture microphone unavailable")), process=None
        ),
    ):
        application._capture_prepared("failed-mic", audio_path, None, None, None, None, None, 0)
    assert states[-1].phase == "error" and "Microphone" in states[-1].detail

    workspace.show_conversation(source, replies)
    with patch("voice_scribe_linux.app.CodexAppServerClient", side_effect=client_factory):
        application._request_rewrite("Must not persist after privacy changes")
        with patch("voice_scribe_linux.app.save_config", side_effect=OSError("Fixture read-only settings")):
            application.incognito_switch.set_active(True)
        settle()
    assert application.config.incognito_mode
    previous_count = len(application.conversation_store.search())
    application._start_pasted_conversation("Private imported text")
    assert len(application.conversation_store.search()) == previous_count
    assert not workspace.quick_polish.is_sensitive()
    assert len(application.conversation_store.replies(source.identifier)) == 2
    application.incognito_switch.set_active(False)
    application._clear_overlay_timeout()
    application.recording_overlay_publisher = None
    application.history_store.delete(source.identifier)
    application._history_changed()
    assert workspace.entry is None
    assert not application.conversation_store.replies(source.identifier)
    workspace.prompt.get_buffer().set_text("")
    application._start_pasted_conversation("Imported text stays complete")
    assert workspace.entry.raw_text == "Imported text stays complete"
    assert copies == [replies[-1].text]
    application.history_store.delete(workspace.entry.identifier)
    application._history_changed()
