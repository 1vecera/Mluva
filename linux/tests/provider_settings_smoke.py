"""Exercise the production provider page on a private display with isolated protocol peers."""

import json
import os
import sys
import threading
import traceback
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from unittest.mock import patch

from conversation_ui_smoke import IsolatedApplication
from gi.repository import GLib, Gtk
from live_workspace_smoke import paint, settle

from mluva_linux.codex_client import CodexAppServerClient
from mluva_linux.config import load_config
from mluva_linux.pipewire import PipeWireDeviceCatalog
from mluva_linux.provider_catalog import VoxtypeCatalog


def descendants(widget):
    """Find the real preferences scroll container to verify every control remains reachable."""
    child = widget.get_first_child()
    while child is not None:
        yield child
        yield from descendants(child)
        child = child.get_next_sibling()


def choose_provider(section, provider: str) -> None:
    """Drive a real GTK row through its selection notification."""
    section.provider_row.set_selected([p.id for p in section.providers].index(provider))


def choose_model(section, model: str | None) -> None:
    """Select an advertised model through the same UI path as the user."""
    section.model_row.set_selected(section.choices.index(model))


def type_model(section, model: str) -> None:
    """Reveal manual entry even when discovery failed, then edit its real EntryRow."""
    section.model_row.set_selected(len(section.choices) - 1)
    assert section.model_entry.get_visible()
    section.model_entry.set_text(model)


def main() -> int:
    """Cover persistence, offline selection, stale discovery, native defaults and production pixels."""
    if "OFFSCREEN_SESSION_ROOT" not in os.environ or os.environ.get("GDK_BACKEND") != "x11":
        raise RuntimeError("Use the repository's isolated X11 runner")
    output = Path(os.environ["OFFSCREEN_ARTIFACT_DIR"])
    scenario = os.environ.get("MLUVA_PROVIDER_SCENARIO", "flow")
    width = int(os.environ.get("MLUVA_UI_WIDTH", "1060"))
    height = int(os.environ.get("MLUVA_UI_HEIGHT", "780"))
    if "OFFSCREEN_DISPLAY_NUMBER" in os.environ:
        assert os.environ["DISPLAY"] == ":" + os.environ["OFFSCREEN_DISPLAY_NUMBER"]
    assert "WAYLAND_DISPLAY" not in os.environ
    for name in ("XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME", "XDG_CACHE_HOME"):
        assert Path(os.environ[name]).is_relative_to(output)
    state = {"status": 200, "delay": False, "requests": []}
    arrived, release = threading.Event(), threading.Event()

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_args):
            """Keep request headers and fixture credentials out of logs."""

        def do_GET(self):
            """Serve distinct speech/chat catalogs with a controllable late response."""
            state["requests"].append(self.path)
            if state["delay"]:
                arrived.set()
                release.wait(4)
            self.send_response(state["status"])
            self.end_headers()
            try:
                self.wfile.write(
                    json.dumps(
                        {
                            "data": [
                                {"id": "speech-alias", "model_info": {"mode": "audio_transcription"}},
                                {"id": "writer-alias", "model_info": {"mode": "chat"}},
                            ]
                        }
                    ).encode()
                )
            except (BrokenPipeError, ConnectionResetError):
                pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    base = f"http://127.0.0.1:{server.server_port}"
    voxtype_fixture = output / "voxtype_catalog.py"
    voxtype_fixture.write_text(
        "import json, sys\n"
        "assert sys.argv[1:] == ['info', 'models', '--json', '--engine', 'whisper']\n"
        "print(json.dumps({'engines': {'whisper': {'models': ["
        "{'name': 'small', 'installed': True}, {'name': 'medium', 'installed': False}]}}}))\n"
    )
    native_clients = []

    def native_client(**options):
        """Use the real JSONL client with a separate content-free server fixture."""
        client = CodexAppServerClient(
            command=(sys.executable, str(Path(__file__).with_name("fake_app_server.py"))), **options
        )
        native_clients.append(client)
        return client

    app = IsolatedApplication()
    errors = []
    checks = []

    def exercise():
        """Operate the native settings dialog and check the resulting dotfile and widget states."""
        try:
            Gtk.Settings.get_default().set_property("gtk-enable-animations", False)
            window = app.window
            window.set_default_size(width, height)
            settle(window.get_mapped)
            paint(window, output / "sizing.png")
            # GTK's CSS borders are excluded from get_width/get_height. Request a
            # compensated toplevel so the captured production viewport is exact.
            window.set_default_size(width + width - window.get_width(), height + height - window.get_height())
            settle(lambda: window.get_width() == width and window.get_height() == height)
            app.settings_button.emit("clicked")
            dialog = app.settings_dialog
            dialog.set_visible_page_name("providers")
            page = app.workspace_settings_pages[1]
            speech, rewrite = page.speech, page.rewrite
            settle(lambda: page.get_mapped() and page.get_width() > 0)
            assert speech.provider.id == "elevenlabs" and rewrite.provider.id == "codex"
            assert not speech.advanced.get_visible() and not rewrite.advanced.get_visible()
            assert not speech.model_entry.get_visible() and not rewrite.model_entry.get_visible()
            assert not state["requests"] and not native_clients
            checks.append("Default provider/model controls; no automatic discovery")
            if scenario == "flow":
                rewrite.refresh_button.emit("clicked")
                settle(lambda: rewrite.catalog_client is None)
                assert len(rewrite.models) == 2
                choose_model(rewrite, "gpt-5.4")
                assert rewrite.fast_row.get_sensitive()
                rewrite.fast_row.set_active(True)
                page.apply_button.emit("clicked")
                assert load_config(app.config_path).rewrite_model == "gpt-5.4"
                assert load_config(app.config_path).rewrite_fast_mode
                # The compact picker must also forget a previous provider's catalog
                # and must never offer a nonexistent compatible-server default.
                app.rewrite_settings.set_loading()
                choose_provider(speech, "voxtype")
                assert speech.values["voxtype_model"] is None
                speech.refresh_button.emit("clicked")
                settle(lambda: speech.catalog_client is None)
                assert [m.identifier for m in speech.models] == ["small"]
                choose_model(speech, "small")
                page.apply_button.emit("clicked")
                assert load_config(app.config_path).voxtype_model == "small"
                checks.append(
                    "Real Codex JSONL and local inventory subprocess; native model/Fast and local model saved"
                )
            if scenario in {"flow", "details", "error"}:
                choose_provider(rewrite, "litellm")
                choose_provider(speech, "litellm")
                assert rewrite.advanced.get_visible() and not rewrite.fast_row.get_visible()
                assert speech.advanced.get_visible() and speech.preview.get_visible()
                rewrite.endpoint_entry.set_text(base + "/rewrite")
                speech.endpoint_entry.set_text(base + "/speech")
                rewrite.key_entry.set_text("FIXTURE_REWRITE_KEY")
                speech.key_entry.set_text("FIXTURE_SPEECH_KEY")
                type_model(rewrite, "manual-writer")
                type_model(speech, "manual-speech")
                if scenario == "error":
                    state["status"] = 401
                for section in (speech, rewrite):
                    section.refresh_button.emit("clicked")
                settle(lambda: speech.catalog_client is None and rewrite.catalog_client is None)
                assert speech.values["transcription_remote_model"] == "manual-speech"
                assert rewrite.values["litellm_model"] == "manual-writer"
                if scenario == "details":
                    speech.advanced.set_expanded(True)
                if scenario == "flow":
                    assert [m.identifier for m in speech.models] == ["speech-alias"]
                    assert [m.identifier for m in rewrite.models] == ["writer-alias"]
                    assert set(state["requests"]) == {"/rewrite/models", "/speech/models"}
                    choose_model(rewrite, "writer-alias")
                    choose_model(speech, "speech-alias")
                    page.apply_button.emit("clicked")
                    saved = load_config(app.config_path)
                    assert saved.litellm_model == "writer-alias" and saved.transcription_remote_model == "speech-alias"
                    assert saved.rewrite_model == "gpt-5.4" and saved.voxtype_model == "small"
                    assert saved.litellm_api_key_env == "FIXTURE_REWRITE_KEY"
                    assert app.rewrite_settings.refresh.get_sensitive()
                    assert not app.rewrite_settings.models
                    assert None not in app.rewrite_settings.choices
                    checks.append("Independent HTTP catalogs; provider switch preserves native and remote models")
                    state["status"] = 401
                    rewrite.refresh_button.emit("clicked")
                    settle(lambda: rewrite.catalog_client is None)
                    assert "Could not load" in rewrite.status.get_label()
                    choose_model(rewrite, "writer-alias")
                    type_model(rewrite, "offline-alias")
                    page.apply_button.emit("clicked")
                    assert load_config(app.config_path).litellm_model == "offline-alias"
                    checks.append("Failed discovery retains choices; manual alias persists offline")
                    before = app.config_path.read_bytes()
                    rewrite.endpoint_entry.set_text("https://user:fixture-secret@example.test/v1")
                    page.apply_button.emit("clicked")
                    assert app.config_path.read_bytes() == before
                    assert "fixture-secret" not in page.status.get_label()
                    rewrite.endpoint_entry.set_text(base + "/rewrite")
                    app.capture_processing = True
                    type_model(rewrite, "busy-alias")
                    page.apply_button.emit("clicked")
                    assert app.config_path.read_bytes() == before
                    app.capture_processing = False
                    page.apply_button.emit("clicked")
                    assert load_config(app.config_path).litellm_model == "busy-alias"
                    checks.append("Invalid URL and busy save blocked atomically with drafts retained")
                    state.update(status=200, delay=True)
                    rewrite.refresh_button.emit("clicked")
                    settle(arrived.is_set)
                    old_client = rewrite.catalog_client
                    choose_provider(rewrite, "codex")
                    release.set()
                    settle(lambda: old_client.response is None or old_client.response.closed)
                    assert not rewrite.models and rewrite.values["rewrite_model"] == "gpt-5.4"
                    page.apply_button.emit("clicked")
                    assert load_config(app.config_path).rewrite_provider == "codex"
                    checks.append("Late endpoint catalog discarded after provider switch")
                    dialog.close()
                    settle(lambda: not page.get_mapped())
                    app.settings_button.emit("clicked")
                    dialog.set_visible_page_name("providers")
                    settle(page.get_mapped)
                    assert rewrite.values["rewrite_model"] == "gpt-5.4"
                    assert speech.values["transcription_remote_model"] == "speech-alias"
                    checks.append("Reopened dialog reflects the persisted provider/model pair")
            paint(window, output / f"providers-{scenario}.png")
            scroller = next(widget for widget in descendants(page) if isinstance(widget, Gtk.ScrolledWindow))
            adjustment = scroller.get_vadjustment()
            adjustment.set_value(adjustment.get_upper() - adjustment.get_page_size())
            paint(window, output / f"providers-{scenario}-bottom.png")
            valid, bounds = page.apply_button.compute_bounds(page)
            assert valid and bounds.get_y() >= 0 and bounds.get_y() + bounds.get_height() <= page.get_height()
            checks.append("Apply and rewrite controls reachable through the native preferences scroller")
            assert window.get_width() == width and window.get_height() == height
            receipt = {
                "scenario": scenario,
                "display": os.environ["DISPLAY"],
                "width": width,
                "height": height,
                "checks": checks,
                "settings_page": dialog.get_visible_page_name(),
                "real_accounts_tested": False,
                "no_capture_or_clipboard": True,
            }
            (output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
            assert all(client.process is None for client in native_clients)
        except Exception:
            errors.append(traceback.format_exc())
        app.quit()
        return GLib.SOURCE_REMOVE

    scheduled = []

    def activated(_app):
        """Schedule only the initial activation; reopening Settings activates the app again."""
        if not scheduled:
            scheduled.append(True)
            GLib.timeout_add(300, exercise)

    try:
        with (
            patch("mluva_linux.app.FocusedTextTargetTracker", return_value=None),
            patch.object(PipeWireDeviceCatalog, "from_system", return_value=PipeWireDeviceCatalog()),
            patch("mluva_linux.provider_catalog.CodexAppServerClient", side_effect=native_client),
            patch(
                "mluva_linux.provider_catalog.VoxtypeCatalog",
                side_effect=lambda: VoxtypeCatalog(
                    (sys.executable, str(voxtype_fixture)),
                ),
            ),
            patch.dict(
                os.environ,
                {
                    "ELEVENLABS_API_KEY": "fixture-only-key",
                    "FIXTURE_REWRITE_KEY": "fixture-only-key",
                    "FIXTURE_SPEECH_KEY": "fixture-only-key",
                },
            ),
        ):
            app.connect("activate", activated)
            app.run([])
    finally:
        release.set()
        server.shutdown()
        server.server_close()
        thread.join(timeout=2)
    if errors:
        raise RuntimeError("\n".join(errors))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
