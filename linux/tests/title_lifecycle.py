"""Exercise automatic-title workers through real JSONL subprocesses and GTK completion gates."""

import sys
import time
from dataclasses import replace
from pathlib import Path
from unittest.mock import patch

from gi.repository import GLib

from mluva_linux.app import MluvaApplication
from mluva_linux.codex_client import CodexAppServerClient
from mluva_linux.conversation_titles import fallback_title


def exercise_titles(application: MluvaApplication) -> None:
    """Cover ordered completion, navigation, preference changes, rename, deletion and Incognito."""
    workspace = application.conversation_workspace
    application.automatic_titles_switch.set_active(True)

    def import_text(text: str) -> None:
        """Use the same new-conversation navigation as the real paste composer."""
        workspace.show_conversation(None, [])
        application._start_pasted_conversation(text)

    def factory(**kwargs: object) -> CodexAppServerClient:
        return CodexAppServerClient(
            command=(sys.executable, str(Path(__file__).with_name("fake_app_server.py")), "--title"), **kwargs
        )

    def settle() -> None:
        deadline = time.monotonic() + 10
        while (application.title_client is not None or application.title_queue) and time.monotonic() < deadline:
            GLib.MainContext.default().iteration(False)
            time.sleep(0.01)
        assert application.title_client is None and not application.title_queue

    with patch("mluva_linux.app.CodexAppServerClient", side_effect=factory) as calls:
        import_text("Připravit páteční vydání. Zkontrolovat nový vzhled a poznámky.")
        first = workspace.entry
        assert first.title == fallback_title(first.raw_text)
        workspace.prompt.get_buffer().set_text("Unsent follow-up")
        application._queue_conversation_title(first)
        assert calls.call_count == 1
        settle()
        assert workspace.entry.title == "Plán pátečního vydání"
        assert workspace.title_label.get_label() == workspace.entry.title
        assert workspace.prompt_text() == "Unsent follow-up"
        assert workspace.result_widgets[0].get_text() == first.raw_text

        import_text("A note with a title draft in the archive")
        editing = workspace.entry
        import_text("A second completion waits in the title queue")
        queued = workspace.entry
        assert len(application.title_queue) == 1
        settle()
        assert application.history_store.find(editing.identifier).title == "Plán pátečního vydání"
        assert application.history_store.find(queued.identifier).title == "Plán pátečního vydání"
        import_text("An asynchronous title must preserve the archive editor")
        editing = workspace.entry
        application._open_history()
        field, _previous = application.history_page.title_entries[editing.identifier]
        field.set_text("An unsaved manual title")
        settle()
        assert application.history_page.title_entries[editing.identifier][0] is field
        assert field.get_text() == "An unsaved manual title"

        for manual in ("My title", None):
            import_text("Source that will be renamed by a person")
            entry = workspace.entry
            application.history_store.update_title(entry.identifier, manual)
            workspace.show_conversation(first, [])
            settle()
            assert application.history_store.find(entry.identifier).title == manual
            assert workspace.entry.identifier == first.identifier
            assert workspace.prompt_text() == "Unsent follow-up"

        import_text("Delete while a title is pending")
        deleted = workspace.entry.identifier
        application.history_store.delete(deleted)
        application._history_changed()
        settle()
        assert not application.conversation_store.search("Delete while")

        import_text("Stop title requests when privacy changes")
        entry = workspace.entry
        client = application.title_client
        application.incognito_switch.set_active(True)
        assert application.title_client is None and not application.title_queue
        application._title_finished(client, entry.identifier, entry.title, "Must be discarded")
        assert application.history_store.find(entry.identifier).title == entry.title
        count = calls.call_count
        import_text("Private text must never be queued")
        assert calls.call_count == count
        application.incognito_switch.set_active(False)

        import_text("Keep the fallback when automatic titles are turned off")
        entry = workspace.entry
        client = application.title_client
        application.automatic_titles_switch.set_active(False)
        assert application.title_client is None
        application._title_finished(client, entry.identifier, entry.title, "Must be discarded")
        assert application.history_store.find(entry.identifier).title == entry.title

    application.config = replace(application.config, automatic_titles=True)
    with patch(
        "mluva_linux.app.CodexAppServerClient",
        return_value=CodexAppServerClient(command=("/nonexistent-mluva-provider",)),
    ):
        import_text("Provider failure keeps this local title")
        entry = workspace.entry
        settle()
        assert application.history_store.find(entry.identifier).title == entry.title
        assert workspace.result_widgets[0].get_text() == entry.raw_text
    application.config = replace(application.config, automatic_titles=False)
