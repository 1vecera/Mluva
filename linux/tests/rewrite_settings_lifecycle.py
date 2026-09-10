"""Exercise the production model picker, persistence and rewrite transport on the private GTK display."""

import faulthandler
import sys
import time
from dataclasses import replace
from pathlib import Path
from unittest.mock import patch

from gi.repository import GLib

from mluva_linux.app import MluvaApplication
from mluva_linux.codex_client import CodexAppServerClient
from mluva_linux.config import load_config


def exercise_rewrite_settings(application: MluvaApplication) -> None:
    """Prove model changes, Fast compatibility, saving failures and catalog retries using a synthetic subprocess."""
    faulthandler.dump_traceback_later(15, exit=True)
    settings = application.rewrite_settings
    workspace = application.conversation_workspace
    source = application.history_store.add("Synthetic text", "Synthetic text", "dictation", "eng", None, "copied")
    workspace.show_conversation(source, [])
    fake_server = Path(__file__).with_name("fake_app_server.py")

    def factory(**kwargs: object) -> CodexAppServerClient:
        """Keep every catalog and rewrite request local while checking its actual wire parameters."""
        option = "--expect-fast" if application.config.rewrite_fast_mode else "--expect-standard"
        return CodexAppServerClient(command=(sys.executable, str(fake_server), option), **kwargs)

    def settle() -> None:
        """Drain worker completions without waiting on live provider or desktop services."""
        deadline = time.monotonic() + 10
        while (application.model_catalog_client or application.rewrite_client) and time.monotonic() < deadline:
            GLib.MainContext.default().iteration(False)
            time.sleep(0.01)
        assert application.model_catalog_client is None and application.rewrite_client is None

    with patch("mluva_linux.app.CodexAppServerClient", side_effect=factory):
        application._load_rewrite_models()
        settle()
        assert settings.model_row.get_sensitive()
        assert settings.fast_row.get_sensitive() and not settings.fast_row.get_active()
        settings.fast_row.set_active(True)
        assert load_config(application.config_path).rewrite_fast_mode
        application._request_rewrite("Polish")
        settle()
        replies = application.conversation_store.replies(source.identifier)
        assert replies[-1].model == "gpt-5.4"
        assert "first text" in workspace.notice.get_label()

        settings.model_row.set_selected(settings.choices.index("gpt-5.4-mini"))
        assert application.config.rewrite_model == "gpt-5.4-mini"
        assert not settings.fast_row.get_sensitive() and not application.config.rewrite_fast_mode
        application._request_rewrite("Make shorter")
        settle()
        assert application.conversation_store.replies(source.identifier)[-1].model == "gpt-5.4-mini"
        assert application.config.codex_model is None, "Rewrite choices must not change capture or title models"

        with patch("mluva_linux.app.save_config", side_effect=OSError("Synthetic write failure")):
            settings.model_row.set_selected(0)
        assert application.config.rewrite_model == "gpt-5.4-mini"
        assert settings.choices[settings.model_row.get_selected()] == "gpt-5.4-mini"
        assert "Could not save" in settings.status.get_label()

        application.config = replace(application.config, rewrite_fast_mode=True)
        settings.set_config(application.config)
        assert settings.fast_row.get_sensitive() and settings.fast_row.get_active()
        application._request_rewrite("Must not run with an unavailable tier")
        settle()
        assert len(application.conversation_store.replies(source.identifier)) == 2
        assert "Fast mode is unavailable" in workspace.notice.get_label()
        settings.fast_row.set_active(False)
        assert not application.config.rewrite_fast_mode and not settings.fast_row.get_sensitive()

    with patch(
        "mluva_linux.app.CodexAppServerClient",
        return_value=CodexAppServerClient(command=("mluva-synthetic-missing-codex",)),
    ):
        application._load_rewrite_models()
        settle()
    assert "Could not load models" in settings.status.get_label()
    assert application.config.rewrite_model == "gpt-5.4-mini"
    with patch("mluva_linux.app.CodexAppServerClient", side_effect=factory):
        application._load_rewrite_models()
        settle()
    assert "Could not load" not in settings.status.get_label()
    settings.model_row.set_selected(0)
    faulthandler.cancel_dump_traceback_later()


def seed_rewrite_models(application: MluvaApplication) -> None:
    """Populate the real picker from a local protocol server for deterministic screenshots."""
    fake_server = Path(__file__).with_name("fake_app_server.py")
    client = CodexAppServerClient(command=(sys.executable, str(fake_server)))
    try:
        application.rewrite_settings.set_models(client.list_models())
    finally:
        client.close()
