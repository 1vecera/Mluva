"""Record production GTK surfaces with synthetic content in the private desktop runner."""

import ctypes
import hashlib
import json
import os
import shutil
import subprocess
import sys
import time
import traceback
from dataclasses import replace
from pathlib import Path

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
gi.require_version("Graphene", "1.0")
from gi.repository import Adw, GLib, Graphene, Gtk
from mluva_linux.app import MluvaApplication
from mluva_linux.codex_client import CodexModel
from mluva_linux.ui import set_button_content

WIDTH, HEIGHT = 1040, 640
DENSITY = 2
SOURCE = "Let's keep the first release small. Dictation, editable notes, and searchable history."
POLISHED = (
    "Keep the first release focused: dictation, editable notes, and searchable history."
)
STRUCTURED = "## First release\n\n- Dictate your thoughts.\n- Edit and polish.\n- Find them again."
GRILLING = "## Questions\n- Who will try the first version?\n\n## Architecture\nBuild a focused note-taking app.\n\n- Dictation and editable notes\n- Searchable history\n- A small first release"


class FixtureRewriteClient:
    """Replace the external provider while exercising real buttons and streaming callbacks."""

    def __init__(self, text):
        self.text = text

    def list_models(self):
        return [CodexModel("capture", "capture", "Capture fixture", True)]

    def transform(self, _prompt, *_args, on_delta=None, **_kwargs):
        for word in self.text.split(" "):
            time.sleep(0.045)
            if on_delta:
                on_delta(word + " ")
        return self.text

    def close(self):
        pass

    def cancel(self):
        pass


class CaptureApplication(MluvaApplication):
    """Keep every production view; substitute only device/provider boundaries."""

    def _initialize_local_services(self):
        super()._initialize_local_services()
        self.config = replace(
            self.config,
            automatic_titles=False,
            auto_copy_rewrite=False,
            welcome_completed=True,
            scroll_lookahead_lines=0,
        )

    def _initialize_capture_services(self):
        self.approved_recording_trigger = "F9"
        self._set_status("Ready to dictate.")

    def _new_rewrite_client(self, **_kwargs):
        return FixtureRewriteClient(getattr(self, "capture_response", POLISHED))


def main():
    if (
        "OFFSCREEN_SESSION_ROOT" not in os.environ
        or os.environ.get("GDK_BACKEND") != "x11"
    ):
        raise RuntimeError(
            "Use dev/run-isolated.sh; captures must have private state and display."
        )
    # Xvfb starts its pointer at screen center. Move it off the app so native
    # hover tooltips cannot obscure the recording; this display is private.
    x11 = ctypes.CDLL("libX11.so.6")
    x11.XOpenDisplay.argtypes = [ctypes.c_char_p]
    x11.XOpenDisplay.restype = ctypes.c_void_p
    x11.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
    x11.XDefaultRootWindow.restype = ctypes.c_ulong
    x11.XWarpPointer.argtypes = [
        ctypes.c_void_p,
        ctypes.c_ulong,
        ctypes.c_ulong,
        ctypes.c_int,
        ctypes.c_int,
        ctypes.c_uint,
        ctypes.c_uint,
        ctypes.c_int,
        ctypes.c_int,
    ]
    x11.XFlush.argtypes = [ctypes.c_void_p]
    x11.XCloseDisplay.argtypes = [ctypes.c_void_p]
    display = x11.XOpenDisplay(None)
    if not display:
        raise RuntimeError("Private X display is unavailable")
    x11.XWarpPointer(
        display, 0, x11.XDefaultRootWindow(display), 0, 0, 0, 0, 2500, 1500
    )
    x11.XFlush(display)
    x11.XCloseDisplay(display)
    output = Path(os.environ["OFFSCREEN_ARTIFACT_DIR"])
    palette = Path(
        os.environ.get(
            "MLUVA_CAPTURE_PALETTE",
            str(Path(__file__).parents[1] / "reference/nord.toml"),
        )
    )
    target = Path(os.environ["XDG_STATE_HOME"]) / "omarchy/current/theme"
    target.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(palette, target / "colors.toml")
    app = CaptureApplication()
    errors, captures, recorder = [], [], []
    started = time.monotonic()

    def record_failure(*error):
        errors.append("".join(traceback.format_exception(*error)))
        app.quit()

    sys.excepthook = record_failure

    def snapshot(name, done=None, attempt=0):
        window = app.window
        paintable = Gtk.WidgetPaintable.new(window)
        width, height = (
            paintable.get_intrinsic_width(),
            paintable.get_intrinsic_height(),
        )
        snapshot = Gtk.Snapshot()
        snapshot.scale(DENSITY, DENSITY)
        paintable.snapshot(snapshot, width, height)
        node = snapshot.to_node()
        if node is None:
            if attempt >= 30:
                raise RuntimeError(f"The native surface was not painted: {name}")
            window.queue_draw()
            GLib.timeout_add(33, lambda: retry_snapshot(name, done, attempt + 1))
            return
        viewport = Graphene.Rect().init(0, 0, width * DENSITY, height * DENSITY)
        texture = window.get_native().get_renderer().render_texture(node, viewport)
        path = output / f"{name}.png"
        texture.save_to_png(str(path))
        regions = {}
        workspace = app.conversation_workspace
        for key, widget in {
            "header": app.header_bar,
            "heading": workspace.heading,
            "messages": workspace.messages,
            "composer": workspace.composer,
            "capture_controls": app.capture_action_bar,
            "live": workspace.live_box,
            "live_draft": workspace.live_draft_box,
            "clock": workspace.live_title,
            "history": workspace.history_list,
        }.items():
            if widget is not None and widget.get_mapped():
                ok, bounds = widget.compute_bounds(window)
                if ok:
                    regions[key] = [
                        round(bounds.get_x(), 2),
                        round(bounds.get_y(), 2),
                        round(bounds.get_width(), 2),
                        round(bounds.get_height(), 2),
                    ]
        captures.append(
            {
                "file": path.name,
                "seconds": round(time.monotonic() - started, 3),
                "width": texture.get_width(),
                "height": texture.get_height(),
                "density": DENSITY,
                "logical_width": width,
                "logical_height": height,
                "widget_width": window.get_width(),
                "widget_height": window.get_height(),
                "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                "palette_sha256": hashlib.sha256(
                    (target / "colors.toml").read_bytes()
                ).hexdigest(),
                "regions": regions,
            }
        )
        if done is not None:
            GLib.timeout_add(100, done)

    def retry_snapshot(name, done, attempt):
        snapshot(name, done, attempt)
        return GLib.SOURCE_REMOVE

    def schedule(seconds, action):
        def run():
            try:
                action()
            except Exception:  # noqa: BLE001 -- retain any GTK callback failure and terminate the capture.
                errors.append(traceback.format_exc())
                app.quit()
            return GLib.SOURCE_REMOVE

        GLib.timeout_add(round(seconds * 1000), run)

    def descendants(widget):
        child = widget.get_first_child()
        while child is not None:
            yield child
            yield from descendants(child)
            child = child.get_next_sibling()

    def prepare():
        window = app.window
        window.set_default_size(WIDTH, HEIGHT)
        window.set_size_request(WIDTH, HEIGHT)
        Gtk.Settings.get_default().set_property("gtk-enable-animations", True)
        Adw.StyleManager.get_default().set_color_scheme(Adw.ColorScheme.FORCE_DARK)
        workspace = app.conversation_workspace
        workspace.set_config(app.config)
        app._navigate_to_page("capture")
        for title, text in [
            ("Weekend ideas", "A walk, a book, and time away from the screen."),
            ("Design review", "Make the useful action easy to find."),
            ("Team follow-up", "Share the next draft on Tuesday."),
        ]:
            entry = app.history_store.add(text, text, "dictation", "eng", None, "ready")
            app.history_store.update_title(entry.identifier, title)
        workspace.refresh_history()
        workspace.show_conversation(None, [])
        display_scale = int(os.environ.get("GDK_SCALE", "1"))
        recorder.append(
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
                    f"{WIDTH * display_scale}x{HEIGHT * display_scale}",
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
                    str(output / "native-navigation.mp4"),
                ],
                stdin=subprocess.PIPE,
            )
        )
        schedule(0.7, lambda: snapshot("empty"))

        def paste():
            workspace.prompt.get_buffer().set_text(SOURCE)
            workspace.send.emit("clicked")
            app.history_store.update_title(
                workspace.entry.identifier, "A focused first release"
            )
            workspace.show_conversation(
                app.history_store.find(workspace.entry.identifier), []
            )

        schedule(1.6, paste)
        schedule(2.5, lambda: snapshot("original"))

        def select_original():
            editor = workspace.editors[(workspace.entry.identifier, None)]
            editor.grab_focus()
            buffer = editor.get_buffer()
            start = SOURCE.index("small")
            buffer.select_range(
                buffer.get_iter_at_offset(start), buffer.get_iter_at_offset(start + 5)
            )

        schedule(2.7, select_original)
        schedule(3.2, lambda: snapshot("original-editable"))

        def polish():
            app.capture_response = POLISHED
            workspace.quick_polish.grab_focus()
            workspace.quick_polish.emit("clicked")

        schedule(4, polish)
        schedule(4.8, lambda: snapshot("polished"))

        def structure():
            app.capture_response = STRUCTURED
            workspace.structured_note.grab_focus()
            workspace.structured_note.emit("clicked")

        schedule(6, structure)
        schedule(7.0, lambda: workspace.conversation_follower.follow(snap=True))
        schedule(7.4, lambda: snapshot("structured"))
        schedule(8, lambda: app._toggle_history_sidebar(None))
        schedule(8.8, lambda: snapshot("history"))
        schedule(9.5, lambda: workspace.search.set_text("release"))
        schedule(10.4, lambda: snapshot("history-search"))
        schedule(11.2, lambda: workspace.search.set_text(""))
        schedule(12, lambda: app._show_commands())
        schedule(12.9, lambda: snapshot("commands"))
        schedule(13.7, lambda: app.command_palette.close())
        schedule(14.4, lambda: app._show_welcome())
        schedule(15.3, lambda: snapshot("providers"))

        def provider_details():
            scroll = next(
                widget
                for widget in descendants(app.welcome_view.providers)
                if isinstance(widget, Gtk.ScrolledWindow)
            )
            scroll.get_vadjustment().set_value(270)

        schedule(15.5, provider_details)
        schedule(16.0, lambda: snapshot("providers-detail"))
        schedule(16.3, lambda: app._show_settings(app.settings_button))
        schedule(17.1, lambda: snapshot("settings"))
        schedule(17.2, lambda: app.settings_view.set_visible_page_name("providers"))
        schedule(17.7, lambda: snapshot("provider-settings"))

        def live():
            app._navigate_to_page("capture")
            if workspace.split.get_show_sidebar():
                app._toggle_history_sidebar(None)
            app.pending_session_identifier = "intro-capture"
            app.pending_mode = "dictation"
            app.pending_incognito = False
            app.config = replace(app.config, live_rewrite_enabled=True)
            app._start_live_rewrite()
            workspace.set_live("00:00", "")
            set_button_content(
                app.record_button, "media-playback-stop-symbolic", "Stop"
            )
            app._set_status("Microphone ready · Scribe v2 realtime")

        schedule(18.2, live)
        schedule(18.6, lambda: snapshot("live-empty"))
        for index, count in enumerate([4, 8, 13]):
            schedule(
                18.8 + index * 0.35,
                lambda count=count, index=index: workspace.set_live(
                    f"00:0{index + 1}", " ".join(SOURCE.split()[:count])
                ),
            )
            schedule(18.9 + index * 0.35, lambda: reset_source_scroll())
            schedule(
                19.0 + index * 0.35,
                lambda index=index: snapshot(f"live-progress-{index + 1}"),
            )
        schedule(19.9, lambda: snapshot("live-start"))

        def reset_source_scroll():
            workspace.live_follower._reader_navigation()
            workspace.live_scroll.get_vadjustment().set_value(0)

        schedule(20.0, reset_source_scroll)
        schedule(20.2, lambda: workspace.show_live_draft(GRILLING, "Grilling"))

        def reset_draft_scroll():
            # Read the beginning of the actual document, rather than retaining
            # the streaming editor's tail padding from the previous view.
            assert (
                "Build a focused note-taking app."
                in workspace.live_draft_text.get_text()
            )
            workspace.live_draft_follower._reader_navigation()
            workspace.live_draft_scroll.get_vadjustment().set_value(0)

        schedule(20.7, reset_draft_scroll)
        schedule(21.1, lambda: snapshot("grilling"))
        schedule(
            22.2,
            lambda: workspace.live_draft_text.get_buffer().insert_at_cursor(
                "\nKeep the scope small."
            ),
        )
        schedule(22.7, reset_draft_scroll)
        schedule(23.1, lambda: snapshot("grilling-edited"))

        def dictation():
            workspace.finish_live()
            app._cancel_live_rewrite()
            app.config = replace(app.config, live_rewrite_enabled=False)
            app._start_live_rewrite()
            workspace.set_live("00:00", "")

        schedule(24.2, dictation)
        schedule(24.7, lambda: snapshot("dictation-empty"))
        for index, count in enumerate([4, 8, 13]):
            schedule(
                25.2 + index * 0.7,
                lambda count=count, index=index: workspace.set_live(
                    f"00:0{index + 1}", " ".join(SOURCE.split()[:count])
                ),
            )
            schedule(25.3 + index * 0.7, reset_source_scroll)
            schedule(
                25.5 + index * 0.7,
                lambda index=index: snapshot(f"dictation-{index + 1}"),
            )
        schedule(27.2, reset_source_scroll)
        schedule(27.8, lambda: snapshot("dictation-full"))

        def theme(name):
            workspace.finish_live()
            app._cancel_live_rewrite()
            app._navigate_to_page("capture")
            workspace.show_conversation(
                workspace.entry,
                app.conversation_store.replies(workspace.entry.identifier)[:1],
            )
            set_button_content(
                app.record_button, "microphone-sensitivity-high-symbolic", "Dictate"
            )
            app._set_status("Ready to dictate.")
            shutil.copyfile(
                Path(__file__).parents[1] / f"reference/{name}.toml",
                target / "colors.toml",
            )
            app.theme_controller.apply()

        schedule(28.5, lambda: theme("nord"))
        schedule(29.0, lambda: snapshot("theme-nord"))
        schedule(29.5, lambda: theme("tokyo-night"))
        schedule(30.1, lambda: snapshot("theme-tokyo-night"))
        schedule(30.6, lambda: theme("rose-pine"))
        schedule(31.3, lambda: snapshot("theme-rose-pine"))

        def clocks(second=0):
            if second == 0:
                theme("nord")
                live()
            if second > 12:
                finish()
                return GLib.SOURCE_REMOVE
            workspace.set_live_status(f"00:{second:02}", recording=True)
            GLib.timeout_add(
                100,
                lambda: retry_snapshot(
                    f"clock-{second:02}", lambda: clocks(second + 1), 0
                ),
            )
            return GLib.SOURCE_REMOVE

        schedule(32.0, clocks)
        return GLib.SOURCE_REMOVE

    def finish():
        app._cancel_live_rewrite()
        app.quit()

    def activated(_app):
        app.disconnect(activation)
        schedule(0.3, prepare)

    activation = app.connect("activate", activated)
    app.run([])
    for process in recorder:
        process.communicate(input=b"q", timeout=10)
    app_root = Path(__file__).resolve().parents[2] / "linux/mluva_linux"
    sources = {
        name: hashlib.sha256((app_root / name).read_bytes()).hexdigest()
        for name in [
            "app.py",
            "conversation_view.py",
            "theme.py",
            "provider_settings.py",
            "settings_view.py",
        ]
    }
    (output / "manifest.json").write_text(
        json.dumps(
            {
                "application": "Mluva production GTK from this worktree",
                "app_source_sha256": sources,
                "capture_script_sha256": hashlib.sha256(
                    Path(__file__).read_bytes()
                ).hexdigest(),
                "content": "Synthetic demonstration text; recognition/rewrite outputs are fixtures, not measured provider latency.",
                "capture": "Private Xvfb desktop with production widgets, native navigation and real screen recording",
                "palette_sha256": hashlib.sha256(palette.read_bytes()).hexdigest(),
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
