"""Capture the original storyboard with production GTK and prepared provider responses."""

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import time
import traceback
import wave
from dataclasses import replace
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
gi.require_version("Graphene", "1.0")
from gi.repository import Adw, GLib, Graphene, Gtk
from mluva_linux.app import MluvaApplication
from mluva_linux.codex_client import CodexModel
from mluva_linux.elevenlabs import TranscriptionResult
from mluva_linux.realtime import RealtimePreview, RealtimeSessionResult
from mluva_linux.ui import set_button_content
from mluva_linux.workflow import DictationWorkflow

ROOT = Path(__file__).resolve().parents[2]
WIDTH, HEIGHT, DENSITY = 1040, 640, 2
SOURCE = "Could we move the review to Tuesday? I’ll send the draft tomorrow."
POLISHED = "Could we reschedule the review for Tuesday? I’ll send the draft tomorrow."
LIVE_SOURCE = (
    "We need to export notes to Markdown, keep the original wording, and save everything locally. "
    "The first version is for personal use. Let’s include the date in each file name."
)
STRUCTURED = (
    "# Notes export\n\n## Requirements\n- Preserve the original wording.\n- Save files locally.\n"
    "- Include the date in each file name.\n\n## Audience\nPersonal use."
)
GRILLING_FIRST = (
    "## Questions\n- Who is it for?\n\n## Architecture\n### Notes export\n"
    "- Export notes as Markdown.\n- Preserve the original wording.\n- Save files locally."
)
ANSWER = "It’s for me, as a file I can keep."
GRILLING_FINAL = (
    "## Questions\n- How should files be named?\n\n## Architecture\n### Notes export\n"
    "- Export notes as Markdown.\n- Preserve the original wording.\n- Save files locally.\n"
    "- Audience: personal use.\n\n### Your answer\n" + ANSWER
)


class PreparedRecorder:
    """Substitute the microphone boundary with a private silent WAV, never a live device."""

    process = None
    audio_level = 0.07
    target = None

    def start(self, path, on_audio_chunk=None):
        self.output_path = path
        path.parent.mkdir(parents=True, exist_ok=True)
        with wave.open(str(path), "wb") as stream:
            stream.setnchannels(1)
            stream.setsampwidth(2)
            stream.setframerate(16000)
            stream.writeframes(b"\0" * 32000)
        self.process = SimpleNamespace(poll=lambda: None)

    def stop(self):
        self.process = None
        return self.output_path

    def cancel(self):
        self.process = None


class PreparedSpeech:
    """Supply storyboard words through the real realtime/finalization interface."""

    is_healthy = True
    bytes_sent = 32000

    def set_preview_enabled(self, _enabled):
        pass

    def submit_audio(self, _frames):
        pass

    def start(self, _language, on_preview=None, **_kwargs):
        self.started = time.monotonic()
        self.callback = on_preview
        self.active = True
        GLib.idle_add(lambda: GLib.timeout_add(130, self.update) and False)
        return self

    def snapshot(self):
        count = min(len(SOURCE.split()), max(0, int((time.monotonic() - self.started - 0.3) * 2.9)))
        return RealtimePreview("", " ".join(SOURCE.split()[:count]))

    def update(self):
        if self.active and self.callback:
            self.callback(self.snapshot())
        return self.active

    def finish(self):
        self.active = False
        return RealtimeSessionResult(TranscriptionResult(SOURCE, "eng", None, "demo"), 0.1)

    def cancel(self):
        self.active = False

    def transcribe(self, *_args, **_kwargs):
        return TranscriptionResult(SOURCE, "eng", None, "demo")


class PreparedRewrite:
    """Keep all native rewrite controls; replace only the external response."""

    def list_models(self):
        return [CodexModel("demo", "demo", "Use Codex default", True)]

    def resolve_model(self, *_args, **_kwargs):
        return "demo"

    def transform(self, *_args, on_delta=None, **_kwargs):
        for word in POLISHED.split():
            time.sleep(0.035)
            if on_delta:
                on_delta(word + " ")
        return POLISHED

    def close(self):
        pass

    cancel = close


class StoryboardApplication(MluvaApplication):
    """Run the installed app's views and workflows with private demo content."""

    def _initialize_local_services(self):
        super()._initialize_local_services()
        host = os.environ.get("MLUVA_AUTHORIZED_HOST_CAPTURE") == "1"
        self.focus_tracker = None
        self.config = replace(
            self.config,
            automatic_titles=False,
            auto_copy_dictation=host,
            auto_copy_rewrite=False,
            auto_paste=False,
            welcome_completed=True,
            live_rewrite_enabled=False,
            review_timeout_seconds=60,
            scroll_lookahead_lines=0,
        )

    def _initialize_capture_services(self):
        self.approved_recording_trigger = "F9"
        self.recorder = PreparedRecorder()
        self.realtime_client = PreparedSpeech()
        self.workflow = DictationWorkflow(
            self.config, self.realtime_client, PreparedRewrite(), self.history_store, self.codex_workspace
        )
        self._set_status("Ready to dictate.")

    def _new_rewrite_client(self, **_kwargs):
        return PreparedRewrite()

    def _maybe_live_rewrite(self, *_args):
        """The live sequence supplies prepared drafts at the storyboard's editorial times."""


def main():
    """Capture only in explicitly isolated state or an authorized host filming session."""
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", action="store_true")
    parser.add_argument("--wait-for-start", action="store_true")
    parser.add_argument("--start-at", type=float, default=0)
    parser.add_argument("--end-at", type=float, default=59.5)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if not 0 <= args.start_at < args.end_at <= 59.5:
        parser.error("Capture interval must be inside the 59.5-second storyboard")
    if args.host:
        if os.environ.get("MLUVA_AUTHORIZED_HOST_CAPTURE") != "1":
            raise RuntimeError("Host recording requires explicit task authorization")
    elif "OFFSCREEN_SESSION_ROOT" not in os.environ:
        raise RuntimeError("Run native asset capture through dev/run-isolated.sh")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    for name in ("XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME"):
        if not Path(os.environ[name]).resolve().is_relative_to(output):
            raise RuntimeError("All app data must remain inside the private capture directory")
    theme = Path(os.environ["XDG_STATE_HOME"]) / "omarchy/current/theme"
    theme.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(ROOT / "launch-video/reference/nord.toml", theme / "colors.toml")
    app, errors, captures, processes = StoryboardApplication(), [], [], []

    def failed(*error):
        errors.append("".join(traceback.format_exception(*error)))
        app.quit()

    sys.excepthook = failed

    def later(seconds, action):
        if seconds < args.start_at or seconds > args.end_at:
            return

        def execute():
            try:
                action()
            except Exception:  # noqa: BLE001 - Preserve callback failures in the capture manifest and fail the run.
                failed(*sys.exc_info())
            return GLib.SOURCE_REMOVE

        GLib.timeout_add(round((seconds - args.start_at) * 1000), execute)

    def snapshot(name, record=True, retries=8):
        paintable = Gtk.WidgetPaintable.new(app.window)
        scene = Gtk.Snapshot()
        scene.scale(DENSITY, DENSITY)
        paintable.snapshot(scene, app.window.get_width(), app.window.get_height())
        node = scene.to_node()
        if node is None:
            if retries == 0:
                raise RuntimeError(f"Native frame never became ready: {name}")
            GLib.timeout_add(50, lambda: (snapshot(name, record, retries - 1), GLib.SOURCE_REMOVE)[1])
            return
        bounds = Graphene.Rect().init(0, 0, app.window.get_width() * DENSITY, app.window.get_height() * DENSITY)
        texture = app.window.get_renderer().render_texture(node, bounds)
        path = output / f"{name}.png"
        texture.save_to_png(str(path))
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        if record:
            workspace = app.conversation_workspace
            layout = {}
            for label, follower in (("source", workspace.live_follower), ("draft", workspace.live_draft_follower)):
                adjustment = follower.scroll.get_vadjustment()
                layout[label] = {
                    "top": follower.view.get_top_margin(),
                    "inset": follower.revision_inset,
                    "position": adjustment.get_value(),
                    "upper": adjustment.get_upper(),
                    "page": adjustment.get_page_size(),
                    "destination": follower.destination,
                }
            captures.append(
                {
                    "file": path.name,
                    "width": texture.get_width(),
                    "height": texture.get_height(),
                    "sha256": digest,
                    "layout": layout,
                }
            )
        return digest

    def prepare():
        app.window.set_default_size(WIDTH, HEIGHT)
        if not args.host:
            app.window.set_size_request(WIDTH, HEIGHT)
        Adw.StyleManager.get_default().set_color_scheme(Adw.ColorScheme.FORCE_DARK)
        workspace = app.conversation_workspace
        app.cleanup_switch.set_active(False)
        for title, text in (
            ("Review checklist", "Review the draft and confirm the next step."),
            ("Weekend notes", "Walk by the river, then read."),
        ):
            item = app.history_store.add(text, text, "dictation", "eng", None, "ready")
            app.history_store.update_title(item.identifier, title)
        workspace.refresh_history()
        workspace.show_conversation(None, [])
        if not args.host:
            processes.append(
                subprocess.Popen(
                    [
                        "ffmpeg",
                        "-hide_banner",
                        "-loglevel",
                        "error",
                        "-y",
                        "-f",
                        "x11grab",
                        "-draw_mouse",
                        "0",
                        "-framerate",
                        "30",
                        "-video_size",
                        f"{WIDTH * DENSITY}x{HEIGHT * DENSITY}",
                        "-i",
                        os.environ["DISPLAY"] + "+0,0",
                        "-c:v",
                        "libx264",
                        "-preset",
                        "ultrafast",
                        "-crf",
                        "12",
                        "-pix_fmt",
                        "yuv420p",
                        str(output / "native-storyboard.mp4"),
                    ],
                    stdin=subprocess.PIPE,
                )
            )
        (output / "started.json").write_text(json.dumps({"monotonic": time.monotonic(), "wall_time": time.time()}))
        later(0.4, lambda: snapshot("empty"))
        later(6, app._show_welcome)
        later(6.7, lambda: snapshot("onboarding-speech"))
        later(7.5, lambda: (setattr(app.welcome_view, "step", 1), app.welcome_view._show_step()))
        later(8.3, lambda: snapshot("onboarding-rewrite"))
        later(9.2, lambda: app._show_settings(app.settings_button))
        later(9.25, lambda: app.settings_view.set_visible_page_name("providers"))
        later(11, lambda: snapshot("providers"))
        later(12, lambda: app._navigate_to_page("capture"))
        if not args.host:
            later(12.3, lambda: app._toggle_recording(app.record_button))
        for timecode, name in ((13, "record-empty"), (14.5, "record-1"), (16, "record-2"), (18, "record-full")):
            later(timecode, lambda name=name: snapshot(name))
        if not args.host:
            later(18.7, lambda: app._toggle_recording(app.record_button))

        def recorded():
            app.history_store.update_title(workspace.entry.identifier, "Review meeting")
            workspace.show_conversation(app.history_store.find(workspace.entry.identifier), [])
            snapshot("original")

        later(19.6, recorded)
        later(26.9, app._dismiss_review)
        later(27, app._open_latest_conversation)
        later(27.7, app._show_commands)
        later(28.3, lambda: snapshot("commands"))
        later(29, lambda: (app.command_palette.close(), workspace.quick_polish.emit("clicked")))
        later(29.9, app._dismiss_review)
        later(30.4, lambda: snapshot("polished"))
        later(33, lambda: app._toggle_history_sidebar(None))
        later(33.7, lambda: snapshot("history"))
        later(34.3, lambda: workspace.search.set_text("review"))
        later(35, lambda: snapshot("history-search"))
        later(36.5, lambda: (workspace.search.set_text(""), app._toggle_history_sidebar(None)))

        def palette(name):
            shutil.copyfile(ROOT / f"launch-video/reference/{name}.toml", theme / "colors.toml")
            app.theme_controller.apply()

        for second, name in ((37, "nord"), (38.6, "tokyo-night"), (40.2, "rose-pine")):
            later(second, lambda name=name: palette(name))
            later(second + 0.6, lambda name=name: snapshot(f"theme-{name}"))
        later(41.7, lambda: palette("nord"))

        def live():
            app._dismiss_review()
            app.pending_session_identifier = "storyboard-live"
            app.pending_mode = "dictation"
            app.pending_incognito = False
            app.config = replace(app.config, live_rewrite_enabled=True, live_rewrite_template="structured-note")
            app._start_live_rewrite()
            set_button_content(app.record_button, "media-playback-stop-symbolic", "Stop")
            workspace.set_live("00:00", "")
            app._set_status("Live rewrite · Experimental")

        later(42, live)
        for i, count in enumerate((7, 15, 23, len(LIVE_SOURCE.split()))):
            later(
                42.5 + i * 1.1,
                lambda i=i, count=count: workspace.set_live(f"00:0{i + 1}", " ".join(LIVE_SOURCE.split()[:count])),
            )
            later(43 + i * 1.1, lambda i=i: snapshot(f"live-{i}"))
        later(
            44,
            lambda: workspace.show_live_draft(
                STRUCTURED.replace("- Include the date in each file name.\n", ""), "Structured note"
            ),
        )
        later(46.3, lambda: workspace.show_live_draft(STRUCTURED, "Structured note"))
        later(47, lambda: snapshot("live-complete"))
        later(
            49,
            lambda: (
                workspace.set_live(
                    "00:07", "Build a notes export. Markdown only. Keep the original wording. Work locally."
                ),
                workspace.show_live_draft(GRILLING_FIRST, "Grilling"),
            ),
        )
        later(49.8, lambda: snapshot("grilling-question"))

        def answer():
            buffer = workspace.live_draft_text.get_buffer()
            buffer.place_cursor(buffer.get_end_iter())
            buffer.insert_at_cursor("\n\n### Your answer\n" + ANSWER)

        later(51, answer)
        later(51.8, lambda: snapshot("grilling-answer"))
        later(53, lambda: workspace.show_live_draft(GRILLING_FINAL, "Grilling"))
        later(53.7, lambda: snapshot("grilling-next"))
        later(args.end_at, app.quit)

    def activated(_app):
        app.disconnect(handler)

        def begin():
            if args.wait_for_start and not (output / "go").exists():
                return GLib.SOURCE_CONTINUE
            prepare()
            return GLib.SOURCE_REMOVE

        GLib.timeout_add(800, begin)

    handler = app.connect("activate", activated)
    with patch("mluva_linux.credentials.stored_speech_key", return_value="demo-not-a-key"):
        app.run([])
    for process in processes:
        process.communicate(input=b"q", timeout=20)
    (output / "manifest.json").write_text(
        json.dumps(
            {
                "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                "provenance": (
                    "Production GTK and workflows; prepared microphone/provider responses and private demo history."
                ),
                "display": "Actual host desktop" if args.host else "Private Xvfb",
                "captures": captures,
                "errors": errors,
            },
            indent=2,
        )
    )
    if errors:
        raise RuntimeError("\n".join(errors))


if __name__ == "__main__":
    main()
