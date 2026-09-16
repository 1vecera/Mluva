"""Exercise real onboarding, download gates, skipped rewriting and appearance offscreen."""

import json
import os
import sys
import threading
import traceback
from pathlib import Path
from unittest.mock import patch

from feedback_ui_smoke import FirstRunApplication
from gi.repository import GLib
from live_workspace_smoke import paint, settle

from mluva_linux.config import load_config


def main():
    """Use a private display and synthetic download boundary without touching the desktop."""
    if "OFFSCREEN_SESSION_ROOT" not in os.environ:
        raise RuntimeError("Use dev/run-isolated.sh")
    output = Path(os.environ["OFFSCREEN_ARTIFACT_DIR"])
    app = FirstRunApplication()
    errors = []
    sys.excepthook = lambda *error: errors.append("".join(traceback.format_exception(*error)))

    def exercise():
        try:
            width = int(os.environ.get("MLUVA_UI_WIDTH", "740"))
            height = int(os.environ.get("MLUVA_UI_HEIGHT", "820"))
            app.window.set_default_size(width, height)
            app.window.set_size_request(width, height)
            view = app.welcome_view
            settle(lambda: view.get_mapped())
            paint(app.window, output / "speech.png")
            view.key_entry.set_text("synthetic-key")
            with patch("mluva_linux.settings_view.store_speech_key", side_effect=RuntimeError("Keyring unavailable")):
                view._next()
                settle(lambda: not view.saving_key)
                assert view.step == 0 and "Keyring unavailable" in view.status.get_label()
            view.key_entry.set_text("synthetic-key")
            with patch("mluva_linux.settings_view.store_speech_key") as store:
                view._next()
                settle(lambda: view.step == 1)
                store.assert_called_once_with("synthetic-key")
            assert not view.key_entry.get_text()
            view._back()
            arrived, release = threading.Event(), threading.Event()
            downloaded = set()

            def download(identifier, progress, cancelled, *, gpu=False):
                arrived.set()
                release.wait(5)
                if not cancelled.is_set():
                    downloaded.add(identifier)

            with (
                patch("mluva_linux.local_model_settings.download", download),
                patch("mluva_linux.local_model_settings.ready", lambda value: value in downloaded),
            ):
                view.speech.provider_row.set_selected([p.id for p in view.speech.providers].index("local"))
                assert not arrived.is_set(), "Selecting a model must not start a download"
                view.speech.local.button.emit("clicked")
                settle(arrived.is_set)
                view._refresh()
                assert not view.next.get_sensitive()
                view._next()
                assert view.step == 0
                paint(app.window, output / "download.png")

                release.set()
                settle(lambda: view.speech.local.cancelled is None)
                view._refresh()
                assert view.next.get_sensitive()
                with patch("mluva_linux.local_model_settings.local_gpu.ready", return_value=False):
                    view.speech.local.gpu.set_active(True)
                    assert not view.speech.is_ready()
                    view.speech.local.gpu.set_active(False)
                assert view.speech.is_ready()
                paint(app.window, output / "local-ready.png")
                view._next()
                assert view.step == 1
                view.rewrite.provider_row.set_selected([p.id for p in view.rewrite.providers].index("none"))
                paint(app.window, output / "rewrite-skip.png")
                view._next()
                assert view.step == 2
                view.appearance.lines.set_value(3)
                view.appearance.opacity.set_value(40)
                view.appearance.position.set_selected(0)
                paint(app.window, output / "appearance-three.png")
                view.appearance.lines.set_value(5)
                view.appearance.opacity.set_value(82)
                paint(app.window, output / "appearance-five.png")
                view._next()
            settle(lambda: app.page_stack.get_visible_child_name() == "capture")
            config = load_config(app.config_path)
            assert config.welcome_completed and config.rewrite_provider == "none"
            assert config.widget_lines == 5 and config.widget_opacity == 82 and not config.auto_paste
            assert not app.live_mode_switch.get_sensitive()
            entry = app.history_store.add(
                "Synthetic note to polish", "Synthetic note to polish", "dictation", "eng", None, "ready"
            )
            workspace = app.conversation_workspace
            workspace.show_conversation(entry, [])
            assert workspace.quick_polish.get_visible() and not workspace.quick_polish.get_sensitive()
            assert not workspace.structured_note.get_sensitive()
            assert not workspace.send.get_sensitive()
            app._begin_rewrite(entry.identifier, "Polish")
            assert app.rewrite_client is None
            app._show_welcome()
            assert view.appearance.lines.get_value() == 5
            app._navigate_to_page("capture")
            paint(app.window, output / "workspace-skip.png")
            (output / "result.json").write_text(
                json.dumps({"download_gate": True, "skip": True, "appearance_saved": True})
            )
        except Exception:
            errors.append(traceback.format_exc())
        finally:
            app.quit()
        return GLib.SOURCE_REMOVE

    GLib.timeout_add(350, exercise)
    app.run([])
    if errors:
        raise AssertionError("\n".join(errors))


if __name__ == "__main__":
    main()
