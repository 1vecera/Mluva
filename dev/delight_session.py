"""Film one native installed Mluva workflow with disclosed local speech/rewrite fixtures."""

import ctypes
import importlib
import json
import math
import os
import re
import shutil
import subprocess
import time
import traceback
from dataclasses import asdict, replace
from pathlib import Path
from unittest.mock import patch

import gi
from voice_scribe_linux.app import MluvaApplication
from voice_scribe_linux.codex_client import CodexModel
from voice_scribe_linux.delivery import DeliveryReceipt
from voice_scribe_linux.elevenlabs import TranscriptionResult
from voice_scribe_linux.pipewire import PipeWireDeviceCatalog
from voice_scribe_linux.recording_control import set_recording_button_content
from voice_scribe_linux.workflow import WorkflowResult

gi.require_version("GdkX11", "4.0")
Gdk = importlib.import_module("gi.repository.Gdk")
GLib = importlib.import_module("gi.repository.GLib")
Gtk = importlib.import_module("gi.repository.Gtk")

SOURCE = (
    "Okay, a few thoughts for Friday. Let's make the first demo feel calm and useful. "
    "Start with someone speaking a rough idea, then let the note take shape while they keep talking. "
    "Keep every original word so we can always go back. The draft should have a short title, "
    "a clear summary, and only the actions that matter. Show the recording light breathing quietly. "
    "I want the words to stay readable as the thought gets longer, without chasing the bottom of the page. "
    "When the idea is ready, copy it into the next thing we're writing. We should also be able to polish "
    "an existing paragraph, edit the final version, and save it for later. Keep the controls close. "
    "A keyboard command should take us straight to the next action. And when we change the desktop theme, "
    "the whole app should feel at home immediately. Let's leave time for a final review before we share it."
)
DRAFT = (
    "# Friday's demo\n\n"
    "Make the first demo **calm and useful**.\n\n"
    "## The flow\n"
    "- Speak a rough idea.\n- Let a clear draft take shape.\n- Keep the original words.\n"
    "- Copy the result into the next task.\n\n"
    "## The details\n"
    "- A quiet recording light.\n- Readable text that follows the thought.\n"
    "- Polish, edit, and save.\n- Commands within reach.\n- A theme that feels at home.\n\n"
    "*Leave time for a final review.*"
)
POLISHED = (
    "# Ready for Friday\n\n"
    "Show a rough idea becoming a **clear, useful note**.\n\n"
    "- Speak and watch the draft take shape.\n"
    "- Preserve the original.\n- Copy, polish, edit, and save.\n"
    "- Keep the controls close and the theme familiar.\n\n"
    "*Review it once more before sharing.*"
)
REWRITTEN = (
    "# Friday, in three steps\n\n"
    "- **Speak** the rough idea.\n- **Shape** it into a clear note.\n"
    "- **Save** the version you want to keep.\n\n"
    "*Leave room for a final review.*"
)


def settle(predicate, timeout: float = 10) -> None:
    """Dispatch actual GTK callbacks until the requested observable state is ready."""
    until = time.monotonic() + timeout
    while time.monotonic() < until:
        while GLib.MainContext.default().pending():
            GLib.MainContext.default().iteration(False)
        if predicate():
            return
        time.sleep(0.004)
    raise TimeoutError("The native capture state did not settle")


def hold(seconds: float) -> None:
    """Keep animation and GTK frame clocks moving during recorded holds."""
    until = time.monotonic() + seconds
    settle(lambda: time.monotonic() >= until, seconds + 1)


class FixtureRewrite:
    """Bound one local deterministic response without a provider or latency claim."""

    def __init__(self, result: str) -> None:
        """Keep one reply immutable for its request."""
        self.result = result

    def list_models(self):
        """Persist explicit fixture identity in the production reply provenance."""
        return [CodexModel("launch-example", "launch-example", "Launch example", True)]

    def transform(self, _prompt, *_args, on_delta=None, **_kwargs):
        """Exercise native streaming for ordinary rewrites and whole-draft Live reconciliation."""
        if on_delta:
            for offset in range(0, len(self.result), 18):
                on_delta(self.result[offset : offset + 18])
                time.sleep(0.075)
        else:
            time.sleep(0.65)
        return self.result

    def close(self):
        """The fixture has no external resource."""


class LaunchApplication(MluvaApplication):
    """Retain native views, stores and actions while replacing only external service boundaries."""

    next_reply = DRAFT

    def _initialize_local_services(self):
        """Disable all automatic delivery and optional inference before constructing the views."""
        super()._initialize_local_services()
        self.config = replace(
            self.config,
            automatic_titles=False,
            auto_copy_dictation=False,
            auto_copy_rewrite=False,
            auto_paste=False,
            review_timeout_seconds=60,
            live_rewrite_enabled=True,
            live_rewrite_template="structured-note",
        )

    def _initialize_capture_services(self):
        """Never initialize hardware audio, global input, portals or authenticated providers."""
        self._set_status("Ready to dictate")

    def _new_rewrite_client(self, *_args, **_kwargs):
        """Every rewrite uses the local deterministic fixture."""
        return FixtureRewrite(self.next_reply)


def main() -> int:
    """Record the installed app, real private shell bridge and actual stock theme files."""
    assert os.environ["DISPLAY"] == ":203" and os.environ["GDK_BACKEND"] == "x11"
    assert not os.environ.get("WAYLAND_DISPLAY") and not os.environ.get("HYPRLAND_INSTANCE_SIGNATURE")
    assert not any("TOKEN" in key or "API_KEY" in key or key.startswith("DAS_") for key in os.environ)
    root = Path(__file__).resolve().parents[1]
    runtime = Path(os.environ["DELIGHT_RUNTIME"])
    pixel_ratio = int(os.environ.get("DELIGHT_PIXEL_RATIO", "1"))
    source_text, draft, polished, rewritten = SOURCE, DRAFT, POLISHED, REWRITTEN
    seed_take = os.environ.get("DELIGHT_RECORDED_TAKE")
    if seed_take:
        take = Path(seed_take)
        source_text = json.loads((take / "recognition.json").read_text())["raw_text"]
        draft = json.loads((take / "replies.json").read_text())[-1]["text"]
        if not source_text.strip() or not draft.strip():
            raise RuntimeError("The real take has no usable original and final draft")
        polished = (
            "# Friday's shop demo\n\n"
            "## Before saving\n"
            "- Show the **upload preview**.\n- Flag missing order IDs.\n"
            "- Check totals against the original file.\n- Keep source rows for an audit.\n\n"
            "## Owners and deadlines\n"
            "- **Maya:** sample data by Thursday.\n- **Me:** review the flow on Friday morning."
        )
        rewritten = (
            "# Ready for Friday\n\n"
            "- **Prepare:** Maya supplies sample data by Thursday.\n"
            "- **Validate:** preview the upload, flag missing IDs, and reconcile totals.\n"
            "- **Review:** I check the flow on Friday morning; source rows stay available for audit."
        )
    output = Path(os.environ["OFFSCREEN_ARTIFACT_DIR"])
    session = Path(os.environ["OFFSCREEN_SESSION_ROOT"])
    for name in ("XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME", "XDG_CACHE_HOME", "XDG_RUNTIME_DIR"):
        Path(os.environ[name]).resolve().relative_to(session)
    os.environ.update(
        PIPEWIRE_REMOTE="disabled-delight",
        PIPEWIRE_RUNTIME_DIR=os.environ["XDG_RUNTIME_DIR"],
        PULSE_SERVER="unix:" + os.environ["XDG_RUNTIME_DIR"] + "/disabled-pulse",
        DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + os.environ["XDG_RUNTIME_DIR"] + "/disabled-system",
    )
    theme_root = Path(os.environ["XDG_STATE_HOME"]) / "omarchy"
    (theme_root / "current").mkdir(parents=True)
    (session / ".local").mkdir()
    (session / ".local/state").symlink_to(Path(os.environ["XDG_STATE_HOME"]))
    for name, wallpaper in (
        ("nord", "0-black-moon.jpg"),
        ("tokyo-night", "0-winding-road.jpg"),
        ("rose-pine", "1-funky-shapes.jpg"),
    ):
        destination = theme_root / "themes" / name
        destination.mkdir(parents=True)
        source = Path("/usr/share/omarchy/themes") / name
        shutil.copy2(source / "colors.toml", destination / "colors.toml")
        if (source / "shell.toml").exists():
            shutil.copy2(source / "shell.toml", destination / "shell.toml")
        shutil.copy2(source / "backgrounds" / wallpaper, output / f"wallpaper-{name}.jpg")
    theme_link = theme_root / "current/theme"
    theme_link.symlink_to(theme_root / "themes/nord")
    for module in ("Commons", "Ui"):
        shutil.copytree(Path("/usr/share/omarchy/shell") / module, output / module)
        for path in (output / module).glob("*.qml"):
            path.write_text(
                path.read_text().replace('Quickshell.env("HOME")', 'Quickshell.env("OFFSCREEN_SESSION_ROOT")')
            )
    shutil.copytree(runtime / "quickshell/mluva.dictation", output / "mluva.dictation")
    shutil.copy2(root / "dev/delight-stage.qml", output / "shell.qml")
    os.environ["MLUVA_SHELL_COMMAND"] = str(runtime / "mluva-shell")
    app = LaunchApplication()
    app.next_reply = draft
    errors, events, checks, copies, children, logs, geometry = [], [], [], [], [], [], []
    recorder = None
    started = time.monotonic()

    def event(name, **details):
        events.append({"event": name, "seconds": round(time.monotonic() - started, 4), "epoch": time.time(), **details})

    def launch(command, name):
        log = (output / name).open("w")
        logs.append(log)
        child = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL)
        children.append(child)
        return child

    def stop(child):
        if child.poll() is not None:
            return
        child.terminate()
        try:
            child.wait(timeout=10)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait()

    launch([os.environ["DELIGHT_COMPOSITOR"], "-n"], "compositor.log")
    shell = launch(["quickshell", "-p", str(output / "shell.qml")], "quickshell.log")

    def ipc(*args):
        return subprocess.check_output(
            ["quickshell", "ipc", "--pid", str(shell.pid), "call", "delight", *args], text=True, timeout=5
        ).strip()

    def photo(name):
        hold(0.12)
        subprocess.run(
            ["magick", "import", "-silent", "-window", "root", str(output / f"{name}.png")], check=True, timeout=10
        )
        event("screenshot", file=name + ".png")

    def copy_text(text, **_kwargs):
        Gdk.Display.get_default().get_clipboard().set(text)
        copies.append(text)
        return DeliveryReceipt(True, False, "Copied. Ready to paste.")

    xlib = ctypes.CDLL("libX11.so.6")
    xlib.XOpenDisplay.argtypes, xlib.XOpenDisplay.restype = [ctypes.c_char_p], ctypes.c_void_p
    xlib.XMoveWindow.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_int]
    xlib.XRaiseWindow.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
    xlib.XFlush.argtypes = [ctypes.c_void_p]
    xlib.XSetInputFocus.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_ulong]
    xlib.XStringToKeysym.argtypes = [ctypes.c_char_p]
    xlib.XStringToKeysym.restype = ctypes.c_ulong
    xlib.XKeysymToKeycode.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
    xlib.XKeysymToKeycode.restype = ctypes.c_uint
    xlib.XSync.argtypes = [ctypes.c_void_p, ctypes.c_int]
    xlib.XCloseDisplay.argtypes = [ctypes.c_void_p]
    xtest = ctypes.CDLL("libXtst.so.6")
    xtest.XTestFakeKeyEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
    display = xlib.XOpenDisplay(b":203")
    assert display

    def position():
        xid = app.window.get_surface().get_xid()
        xlib.XMoveWindow(display, xid, 192 * pixel_ratio, 134 * pixel_ratio)
        xlib.XRaiseWindow(display, xid)
        xlib.XFlush(display)

    def key(*names):
        """Exercise native key routing only inside the explicit private X11 display."""
        xlib.XSetInputFocus(display, app.window.get_surface().get_xid(), 1, 0)
        codes = [xlib.XKeysymToKeycode(display, xlib.XStringToKeysym(name.encode())) for name in names]
        assert all(codes)
        for code in codes:
            assert xtest.XTestFakeKeyEvent(display, code, True, 0)
        for code in reversed(codes):
            assert xtest.XTestFakeKeyEvent(display, code, False, 0)
        xlib.XSync(display, False)

    def speech(words, begin, end, interval=0.14):
        for offset in range(begin, end, 2):
            text = " ".join(words[: min(offset + 2, len(words))])
            elapsed = time.monotonic() - started
            state = app._recording_bar_state(
                kind="recording", elapsed=f"00:{int(elapsed):02}", level=0.35 + 0.2 * math.sin(offset), preview=text
            )
            app._present_recording_bar(state)
            if app.window.get_visible() and app.conversation_workspace.live_draft_box.get_visible():
                boxes = []
                for widget in (app.conversation_workspace.live_scroll, app.conversation_workspace.live_draft_box):
                    valid, rect = widget.compute_bounds(app.window)
                    boxes.append([rect.get_x(), rect.get_y(), rect.get_width(), rect.get_height()] if valid else [])
                geometry.append({"seconds": round(elapsed, 4), "panels": boxes})
            hold(interval)

    def exercise():
        nonlocal recorder, started
        try:
            Gtk.Settings.get_default().set_property("gtk-font-name", "Adwaita Sans 14")
            Gtk.Settings.get_default().set_property("gtk-enable-animations", True)
            app.window.set_size_request(1536, 844)
            app.window.set_default_size(1536, 844)
            settle(lambda: app.window.get_width() > 1436 and app.window.get_height() > 744)
            hold(0.2)
            event(
                "initial-window-allocation",
                content=[app.window.get_width(), app.window.get_height()],
                scale=app.window.get_scale_factor(),
            )
            app.window.set_default_size(3072 - app.window.get_width(), 1688 - app.window.get_height())
            settle(lambda: app.window.get_width() == 1536 and app.window.get_height() == 844)
            position()
            settle(lambda: json.loads(ipc("state"))["width"] == 1920)
            ipc("layout")
            workspace = app.conversation_workspace
            workspace.set_config(app.config)
            workspace.copy_text = copy_text
            for title in ("A note from the morning walk", "A calmer first release", "Ideas worth keeping"):
                item = app.history_store.add(title, title, "dictation", "eng", None, "ready")
                app.history_store.update_title(item.identifier, title)
            workspace.refresh_history()
            workspace.show_conversation(None, [])
            app.window.set_visible(False)
            photo("desktop")
            recorder = launch(
                [
                    "ffmpeg",
                    "-nostdin",
                    "-hide_banner",
                    "-loglevel",
                    "info",
                    "-y",
                    "-f",
                    "x11grab",
                    "-draw_mouse",
                    "0",
                    "-framerate",
                    "60",
                    "-video_size",
                    f"{1920 * pixel_ratio}x{1080 * pixel_ratio}",
                    "-i",
                    ":203",
                    "-c:v",
                    "libx264",
                    "-preset",
                    "ultrafast",
                    "-crf",
                    "16",
                    "-threads",
                    "3",
                    "-pix_fmt",
                    "yuv420p",
                    str(output / "workflow.mkv"),
                ],
                "recording.log",
            )
            started = time.monotonic()
            words = source_text.split()
            event("outside-recording")
            set_recording_button_content(app.record_button)
            app.record_button.set_sensitive(True)
            app.record_button.remove_css_class("suggested-action")
            app.record_button.add_css_class("destructive-action")
            speech(words, 0, 52, 0.16)
            photo("outside-recording")
            hold(0.5)
            event("inside-recording")
            app.window.present()
            position()
            speech(words, 52, 92, 0.15)
            photo("inside-recording")
            hold(0.6)
            event("live-rewrite")
            app.pending_session_identifier = "delight-launch-example"
            app.pending_mode, app.pending_incognito = "dictation", False
            app._start_live_rewrite()
            for index, (begin, end) in enumerate(((92, 116), (116, 140), (140, len(words)))):
                app.live_updating = True
                workspace.show_live_draft(draft[: (180, 330, len(draft))[index]], "Live draft · provisional until Stop")
                app.live_updating = False
                speech(words, begin, end, 0.18)
                hold(0.55)
            photo("live-rewrite")
            hold(1)
            event("finish-live")
            entry = app.history_store.add(source_text, source_text, "dictation", "eng", None, "ready")
            app.history_store.update_title(entry.identifier, "Friday's shop demo" if seed_take else "Friday's demo")
            entry = app.history_store.find(entry.identifier)
            result = WorkflowResult(
                TranscriptionResult(source_text, "eng", None, None),
                source_text,
                DeliveryReceipt(False, False, "Dictation ready."),
                entry,
                None,
                False,
                False,
                "dictation",
                0,
                0,
                0,
                app.pending_session_identifier,
                False,
                "local-launch-example",
                None,
            )
            app.capture_processing = True
            app._publish_completion_status("processing", "Finishing dictation…")
            app._workflow_finished(result)
            settle(lambda: app.live_schedule is None)
            assert app.conversation_store.replies(entry.identifier)[-1].text == draft
            checks.append("Production Live finalization persisted local fixture draft with original unchanged")
            photo("markdown")
            hold(0.8)
            event("copy")
            workspace.copy_buttons[-1].emit("clicked")
            assert copies[-1] == draft
            checks.append("Visible Copy action wrote expected text into private X11 clipboard")
            hold(2)
            event("polish")
            app.next_reply = polished
            workspace.quick_polish.emit("clicked")
            settle(lambda: app.rewrite_client is None)
            assert app.conversation_store.replies(entry.identifier)[-1].text == polished
            photo("polish")
            hold(0.45)
            event("rewrite")
            app.next_reply = rewritten
            workspace.prompt.get_buffer().set_text("Make this three concise steps.")
            workspace.send.emit("clicked")
            settle(lambda: app.rewrite_client is None)
            assert app.conversation_store.replies(entry.identifier)[-1].text == rewritten
            checks.append("Both visible Polish and custom Rewrite actions persisted separate local example replies")
            photo("rewrite")
            hold(1)
            event("edit-save")
            editor = workspace.result_widgets[-1]
            editor.grab_focus()
            hold(0.6)
            buffer = editor.get_buffer()
            buffer.insert(buffer.get_end_iter(), "\n\nReady when you are.")
            hold(0.8)
            workspace.save_buttons[-1].emit("clicked")
            assert "Ready when you are." in app.conversation_store.replies(entry.identifier)[-1].text
            workspace.prompt.grab_focus()
            checks.append("Native lossless editor and Save edits persisted final Markdown")
            photo("saved")
            hold(1)
            event("commands")
            key("Control_L", "p")
            settle(lambda: app.command_palette is not None and app.command_palette.get_mapped())
            hold(0.8)
            palette = app.command_palette
            for letter in "copy":
                key(letter)
                hold(0.2)
            photo("commands")
            hold(0.6)
            assert palette.search.get_text() == "copy"
            key("Return")
            settle(lambda: app.command_palette is None)
            assert copies[-1] == app.conversation_store.replies(entry.identifier)[-1].text
            checks.append("Private XTest Ctrl-P, typed search and Enter dispatched Copy current text")
            hold(0.6)
            event("themes")
            for name in ("nord", "tokyo-night", "rose-pine"):
                if name != "nord":
                    theme_link.unlink()
                    theme_link.symlink_to(theme_root / "themes" / name)
                colors = (theme_link / "colors.toml").read_text()
                ipc("theme", name, colors)
                expected = {"nord": "#2e3440", "tokyo-night": "#1a1b26", "rose-pine": "#faf4ed"}[name]
                rgba = Gdk.RGBA()
                rgba.parse(expected)
                settle(
                    lambda expected_color=rgba: (
                        app.window.get_style_context().lookup_color("vs_canvas")[1].equal(expected_color)
                    ),
                    4,
                )
                event("theme-applied", theme=name, canvas=expected)
                photo("theme-" + name)
                hold(1.5)
            checks.append(
                "Existing GTK window followed real Nord, Tokyo Night and Rosé Pine palettes via theme watcher"
            )
            event("finished")
            assert app.history_store.find(entry.identifier).raw_text == source_text
            checks.append("Raw original remained byte-for-byte unchanged through all actions")
            (output / "original.txt").write_text(source_text + "\n")
            (output / "replies.json").write_text(
                json.dumps([asdict(reply) for reply in app.conversation_store.replies(entry.identifier)], indent=2)
                + "\n"
            )
        except Exception:  # noqa: BLE001 - retain a failed-take receipt and shut down all private children.
            errors.append(traceback.format_exc())
        finally:
            if recorder is not None:
                stop(recorder)
            app.quit()
        return GLib.SOURCE_REMOVE

    def activated(_app):
        app.disconnect(activation)
        GLib.timeout_add(650, exercise)

    activation = app.connect("activate", activated)
    try:
        with (
            patch("voice_scribe_linux.app.FocusedTextTargetTracker", return_value=None),
            patch("voice_scribe_linux.app.PipeWireDeviceCatalog.from_system", return_value=PipeWireDeviceCatalog()),
            patch("voice_scribe_linux.app.deliver_text", side_effect=copy_text),
        ):
            app.run([])
    finally:
        for child in reversed(children):
            stop(child)
        for log in logs:
            log.close()
        xlib.XCloseDisplay(display)
    recording_log = output / "recording.log"
    video_clock = (
        re.search(r"Duration: N/A, start: ([0-9.]+)", recording_log.read_text()) if recording_log.exists() else None
    )
    (output / "capture.json").write_text(
        json.dumps(
            {
                "display": ":203",
                "fps_requested": 60,
                "screen": [1920 * pixel_ratio, 1080 * pixel_ratio],
                "pixel_ratio": pixel_ratio,
                "application": [1536, 844],
                "application_position": [192, 134],
                "runtime": str(runtime),
                "seed_take": seed_take,
                "video_start_epoch": float(video_clock[1]) if video_clock else None,
                "event_clock": (
                    "Monotonic time since FFmpeg launch plus software wall time; "
                    "video_start_epoch is its first capture timestamp"
                ),
                "disclosure": (
                    "Installed native GTK and Quickshell views; scripted transcript and local rewrite fixtures. "
                    "No microphone, recognition, inference quality, provider timing or physical shortcut claim."
                ),
                "isolation": (
                    "Private X11, D-Bus, AT-SPI, XDG and clipboard; audio, global input and providers disabled."
                ),
                "desktop": (
                    "Stock Omarchy wallpapers and palettes; isolated editorial bar; actual installed Mluva widget."
                ),
                "events": events,
                "checks": checks,
                "errors": errors,
                "live_geometry": geometry,
            },
            indent=2,
        )
        + "\n"
    )
    if errors:
        print(errors[0])
    return int(bool(errors))


if __name__ == "__main__":
    raise SystemExit(main())
