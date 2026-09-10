"""Measure committed speech through the production scheduler, provider transport and GTK paint."""

import json
import os
import sys
import time
import traceback
from dataclasses import replace
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

from conversation_ui_smoke import IsolatedApplication
from gi.repository import GLib
from live_workspace_smoke import paint, settle

from mluva_linux.codex_client import CodexAppServerClient
from mluva_linux.realtime import RealtimePreview


class SpeechFixture:
    """Publish synthetic committed snapshots through the application's real readiness wiring."""

    is_healthy = True

    def __init__(self):
        """Keep the provider callback and latest snapshot private to one capture."""
        self.preview = RealtimePreview("", "")
        self.callback = None

    def start(self, _language, on_preview=None, on_committed_segment=None):
        """Replace microphone/network startup while retaining the production callback registration."""
        self.callback = on_preview
        return self

    def snapshot(self):
        """Supply the same committed text to the regular 250 ms recording timer."""
        return self.preview

    def commit(self, text):
        """Mark the instant speech recognition commits, before any scheduler callback or polling."""
        self.preview = RealtimePreview(text, "")
        if self.callback is not None:
            self.callback(self.preview)

    def cancel(self):
        """Release the synthetic source without touching a microphone or provider."""
        self.callback = None


def main():
    """Compare repeated real GTK runs using a controlled independent JSONL provider subprocess."""
    if "OFFSCREEN_SESSION_ROOT" not in os.environ or os.environ.get("DISPLAY") != ":192":
        raise RuntimeError("Run in the isolated harness with OFFSCREEN_DISPLAY_NUMBER=192")
    output = Path(os.environ["OFFSCREEN_ARTIFACT_DIR"])
    app = IsolatedApplication()
    errors = []
    samples = []
    fixture = Path(__file__).with_name("fake_app_server.py")
    chunks = (
        "Build a task specification for the release.",
        " Preserve the original recording and all manual changes to the draft.",
        " Keep missing information visible and save exactly one final reply when recording finishes.",
    )

    def measure_capture(repeat):
        """Measure one capture and drain its callbacks before the next independent trial."""
        workspace = app.conversation_workspace
        measured = {}
        speech = SpeechFixture()
        app.pending_session_identifier = f"timing-{repeat}"
        app.pending_mode = "dictation"
        app.pending_incognito = False
        app.realtime_client = speech
        app._start_live_rewrite()
        workspace.set_live("Recording · synthetic committed speech", "Waiting for speech…")

        class TimedClient(CodexAppServerClient):
            """Time the complete transport, including model discovery and process shutdown."""

            def transform(self, *args, **kwargs):
                """Separate model/setup time from the controlled provider's completed reply."""
                measured.setdefault("transform", time.monotonic())
                result = super().transform(*args, **kwargs)
                measured.setdefault("provider_complete", time.monotonic())
                return result

        def client(*_args, **_kwargs):
            """Record the request start before creating the actual production transport."""
            measured.setdefault("request", time.monotonic())
            return TimedClient(
                command=(sys.executable, str(fixture), "--live", "--compact-live"), turn_timeout_seconds=5
            )

        def after_paint(_clock):
            """Use a painted completed draft, not request start, template display or a text delta."""
            view = workspace.live_draft_text
            ending = view.get_iter_location(view.get_buffer().get_end_iter())
            visible = view.get_visible_rect()
            if "Dictated details" in workspace.live_draft() and visible.y <= ending.y < visible.y + visible.height:
                measured.setdefault("visible", time.monotonic())

        with (
            patch.object(app, "_new_rewrite_client", side_effect=client),
            patch.object(app, "_capture_prepared", return_value=GLib.SOURCE_REMOVE),
        ):
            app._prepare_capture(
                app.pending_session_identifier, output / "unused.wav", "eng", False, None, "dictation", False, None
            )
            settle(lambda: not GLib.MainContext.default().pending())
            app.realtime_session = speech
            app.recorder = SimpleNamespace(process=True, audio_level=0.2)
            app.capture_started_at = time.monotonic()
            timer = GLib.timeout_add(250, app._update_capture_status)
            clock = app.window.get_frame_clock()
            painted = clock.connect("after-paint", after_paint)
            # Commit just after a timer tick to include the polling delay in v0.3.0.
            deadline = time.monotonic() + 0.27
            settle(lambda: time.monotonic() >= deadline)
            measured["committed"] = time.monotonic()
            speech.commit(chunks[0])
            scheduled = []
            for index in (1, 2):

                def commit(index=index):
                    """Grow the committed utterance past the original 160-character gate."""
                    speech.commit("".join(chunks[: index + 1]))
                    return GLib.SOURCE_REMOVE

                scheduled.append(GLib.timeout_add(index * 2000, commit))
            try:
                settle(lambda: "visible" in measured, timeout=8)
                assert "volatile words" not in workspace.live_draft()
                sample = {
                    "scheduler_ms": (measured["request"] - measured["committed"]) * 1000,
                    "setup_ms": (measured["transform"] - measured["request"]) * 1000,
                    "provider_ms": (measured["provider_complete"] - measured["transform"]) * 1000,
                    "paint_ms": (measured["visible"] - measured["provider_complete"]) * 1000,
                    "committed_to_visible_ms": (measured["visible"] - measured["committed"]) * 1000,
                    "model_identifier": app.live_last_model,
                    "callback_registered": speech.callback is not None,
                }
                samples.append(sample)
                if repeat == 0:
                    paint(app.window, output / "first-draft.png")
            finally:
                clock.disconnect(painted)
                GLib.source_remove(timer)
                for source in scheduled:
                    if GLib.MainContext.default().find_source_by_id(source) is not None:
                        GLib.source_remove(source)
                app.recorder = None
                app.capture_started_at = None
                app.realtime_session = None
                app._cancel_live_rewrite()
                workspace.finish_live()

    def exercise():
        """Keep real scheduler intervals and frame clocks; substitute only external effects."""
        try:
            app.window.set_default_size(1060, 780)
            app.window.set_size_request(1060, 780)
            app.config = replace(app.config, live_rewrite_enabled=True)
            for repeat in range(3):
                measure_capture(repeat)
            result = {
                "nature": "Controlled speech and Codex JSONL subprocess; real app scheduler, transport and GTK paint",
                "provider": "fake_app_server.py --live --compact-live (500 ms reply; model identifier is a fixture)",
                "display": os.environ["DISPLAY"],
                "application_source": sys.modules["mluva_linux.app"].__file__,
                "viewport": [app.window.get_width(), app.window.get_height()],
                "minimum_characters": app.config.live_rewrite_min_characters,
                "interval_seconds": app.config.live_rewrite_interval_seconds,
                "committed_chunks_at_seconds": [0, 2, 4],
                "committed_characters": [len("".join(chunks[:index])) for index in (1, 2, 3)],
                "samples": samples,
            }
            (output / "timing.json").write_text(json.dumps(result, indent=2) + "\n")
            print(json.dumps(result), flush=True)
        except Exception:
            errors.append(traceback.format_exc())
        app.quit()
        return GLib.SOURCE_REMOVE

    app.connect("activate", lambda _app: GLib.timeout_add(500, exercise))
    app.run([])
    if errors:
        raise RuntimeError("\n".join(errors))


if __name__ == "__main__":
    main()
