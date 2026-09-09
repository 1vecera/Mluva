"""Exercise native feature controls using explicit local text and rewrite fixtures."""

import ctypes
import json
import os
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
from voice_scribe_linux.pipewire import PipeWireDeviceCatalog

gi.require_version("GdkX11", "4.0")
from gi.repository import Gdk, GLib, Gtk  # noqa: E402

SOURCE = (
    "Okay, let's keep Friday's demo small. Show the upload preview, flag missing order IDs, "
    "and check the totals before saving."
)
POLISHED = (
    "Keep Friday's demo focused: show the upload preview, flag missing order IDs, and check the totals before saving."
)
EDITED = POLISHED + "\n\nLeave time for a final review."


def settle(predicate, timeout=10) -> None:
    """Wait for an observed state while servicing GTK callbacks."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        while GLib.MainContext.default().pending():
            GLib.MainContext.default().iteration(False)
        if predicate():
            return
        time.sleep(0.01)
    raise TimeoutError("Production feature state did not settle")


def hold(seconds: float) -> None:
    """Retain real-time reading room without freezing the GUI."""
    until = time.monotonic() + seconds
    settle(lambda: time.monotonic() >= until, seconds + 1)


def descendants(widget):
    """Locate real buttons through their visible labels."""
    child = widget.get_first_child()
    while child is not None:
        yield child
        yield from descendants(child)
        child = child.get_next_sibling()


class FixtureRewrite:
    """A visibly disclosed local reply, never a provider or latency demonstration."""

    def list_models(self):
        """Label persisted provenance with the fixture identity."""
        return [CodexModel("scripted-ui-demo", "scripted-ui-demo", "Scripted UI demo", True)]

    def transform(self, _prompt, _cwd, _model, *, max_output_characters, on_delta, effort, service_tier):
        """Exercise actual streaming and persistence with a short deterministic reply."""
        assert len(POLISHED) < max_output_characters
        for offset in range(0, len(POLISHED), 12):
            on_delta(POLISHED[offset : offset + 12])
            time.sleep(0.08)
        return POLISHED

    def close(self):
        """No external resources are opened by this fixture."""


class FeatureApplication(MluvaApplication):
    """Retain production views/actions/stores with external services disabled."""

    def _initialize_local_services(self):
        super()._initialize_local_services()
        self.config = replace(
            self.config,
            automatic_titles=False,
            auto_copy_dictation=False,
            auto_copy_rewrite=False,
            auto_paste=False,
            review_timeout_seconds=60,
        )

    def _initialize_capture_services(self):
        """Never initialize a microphone, provider, portal or input target."""
        self._set_status("Dictation ready. Automatic copying is off.")

    def _new_rewrite_client(self, *_args, **_kwargs):
        """Resolve all rewrite requests to the disclosed local fixture."""
        return FixtureRewrite()


def main() -> int:
    """Record one isolated action sequence and assertions beside its uncut video."""
    assert os.environ["DISPLAY"] == ":195" and os.environ["GDK_BACKEND"] == "x11"
    assert not os.environ.get("WAYLAND_DISPLAY")
    assert not any("TOKEN" in key or "API_KEY" in key or key.startswith("DAS_") for key in os.environ)
    root = Path(__file__).resolve().parents[1]
    output = Path(os.environ["OFFSCREEN_ARTIFACT_DIR"])
    session = Path(os.environ["OFFSCREEN_SESSION_ROOT"])
    for name in ("XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME", "XDG_CACHE_HOME", "XDG_RUNTIME_DIR"):
        Path(os.environ[name]).resolve().relative_to(session)
    os.environ.update(
        PIPEWIRE_REMOTE="disabled-film",
        PIPEWIRE_RUNTIME_DIR=os.environ["XDG_RUNTIME_DIR"],
        PULSE_SERVER="unix:" + os.environ["XDG_RUNTIME_DIR"] + "/disabled-pulse",
        DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + os.environ["XDG_RUNTIME_DIR"] + "/disabled-system",
    )
    for destination in (
        Path(os.environ["XDG_STATE_HOME"]) / "omarchy/current/theme",
        session / ".local/state/omarchy/current/theme",
    ):
        destination.mkdir(parents=True)
        for name in ("colors.toml", "shell.toml"):
            shutil.copy2(root / "docs/promotion/assets/sources/nord" / name, destination / name)
    for module in ("Commons", "Ui"):
        shutil.copytree(Path("/usr/share/omarchy/shell") / module, output / module)
        for path in (output / module).glob("*.qml"):
            path.write_text(
                path.read_text().replace('Quickshell.env("HOME")', 'Quickshell.env("OFFSCREEN_SESSION_ROOT")')
            )
    shutil.copytree(root / "linux/quickshell/mluva.dictation", output / "mluva.dictation")
    shutil.copy2(root / "dev/feature-stage.qml", output / "shell.qml")
    os.environ["MLUVA_SHELL_COMMAND"] = str(root / "linux/mluva-shell")
    scenario = os.environ["FILM_SCENARIO"]
    children, logs, errors, events, checks, copies = [], [], [], [], [], []
    recorder = None
    started = time.monotonic()

    def launch(command, name):
        log = (output / name).open("w")
        logs.append(log)
        child = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT, stdin=subprocess.PIPE)
        children.append(child)
        return child

    launch([os.environ["FILM_COMPOSITOR"], "-n"], "compositor.log")
    shell = launch(["quickshell", "-p", str(output / "shell.qml")], "quickshell.log")

    def ipc(*args):
        return subprocess.check_output(
            ["quickshell", "ipc", "--pid", str(shell.pid), "call", "features", *args], text=True, timeout=5
        ).strip()

    def event(name, **details):
        events.append({"event": name, "seconds": round(time.monotonic() - started, 3), **details})

    def photograph(name):
        frames = []
        app.window.add_tick_callback(lambda *_: frames.append(True) is None and len(frames) < 4)
        app.window.queue_draw()
        settle(lambda: len(frames) >= 4)
        subprocess.run(
            ["magick", "import", "-silent", "-window", "root", str(output / f"{name}.png")], check=True, timeout=10
        )
        event("screenshot", file=f"{name}.png")

    def copy_text(text, **_kwargs):
        Gdk.Display.get_default().get_clipboard().set(text)
        copies.append(text)

    app = FeatureApplication()
    xlib = ctypes.CDLL("libX11.so.6")
    xlib.XOpenDisplay.argtypes, xlib.XOpenDisplay.restype = [ctypes.c_char_p], ctypes.c_void_p
    xlib.XMoveWindow.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_int]
    xlib.XRaiseWindow.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
    xlib.XFlush.argtypes = [ctypes.c_void_p]
    display = xlib.XOpenDisplay(b":195")
    assert display

    def exercise():
        nonlocal recorder, started
        try:
            Gtk.Settings.get_default().set_property("gtk-font-name", "Inter 15")
            app.window.set_size_request(1840, 920)
            app.window.set_default_size(1840, 920)
            settle(lambda: app.window.get_width() > 1700)
            hold(0.2)
            app.window.set_default_size(3680 - app.window.get_width(), 1840 - app.window.get_height())
            settle(lambda: app.window.get_width() == 1840 and app.window.get_height() == 920)
            xid = app.window.get_surface().get_xid()
            xlib.XMoveWindow(display, xid, 40, 94)
            xlib.XRaiseWindow(display, xid)
            xlib.XFlush(display)
            settle(lambda: json.loads(ipc("state"))["width"] == 1920)
            ipc("layout")
            workspace = app.conversation_workspace
            workspace.set_config(app.config)
            workspace.copy_text = copy_text
            for title in ("Monday planning", "Notes from a morning walk", "Friday demo"):
                text = SOURCE if title == "Friday demo" else title
                item = app.history_store.add(text, text, "dictation", "eng", None, "copied")
                app.history_store.update_title(item.identifier, title)
            entry = app.history_store.find(item.identifier)
            workspace.refresh_history()
            workspace.show_conversation(entry, [])
            if scenario == "polish":
                workspace.show_conversation(None, [])
                workspace.prompt.get_buffer().set_text(SOURCE)
            elif scenario == "floating":
                app.window.set_visible(False)
                app._publish_review(entry.identifier)
                settle(lambda: json.loads(ipc("state"))["phase"] == "ready")
            elif scenario == "export":
                os.chdir(session)
                app.history_page.export_directory = Path("Exports")
                workspace.archive_button.emit("clicked")
                settle(lambda: app.history_page.entry_rows[entry.identifier].get_expanded())
                hold(0.4)
                adjustment = app.history_page.scroll.get_vadjustment()
                adjustment.set_value(adjustment.get_upper() - adjustment.get_page_size())
            elif scenario == "providers":
                app.settings_button.emit("clicked")
                app.settings_dialog.set_visible_page_name("providers")
                settle(lambda: app.workspace_settings_pages[1].get_mapped())
            if scenario != "floating":
                photograph("before")
            recorder = launch(
                [
                    "ffmpeg",
                    "-nostdin",
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
                    "1920x1080",
                    "-i",
                    ":195",
                    "-c:v",
                    "libx264",
                    "-preset",
                    "ultrafast",
                    "-crf",
                    "18",
                    "-threads",
                    "2",
                    "-pix_fmt",
                    "yuv420p",
                    str(output / "workflow.mkv"),
                ],
                "recording.log",
            )
            started = time.monotonic()
            hold(1.3)
            event("action", scenario=scenario)
            if scenario == "polish":
                workspace.send.emit("clicked")
                entry = workspace.entry
                assert entry.raw_text == SOURCE
                hold(1)
                workspace.quick_polish.emit("clicked")
                settle(lambda: app.rewrite_client is None)
                assert app.conversation_store.replies(entry.identifier)[-1].text == POLISHED
                checks.append("Pasted existing text and Polish button exercised; local scripted reply persisted")
            elif scenario == "floating":
                ipc("copy")
                settle(lambda: copies == [SOURCE])
                ipc("overlayImage", str(output / "overlay.png"))
                settle(lambda: (output / "overlay.png").exists())
                hold(2)
                ipc("open")
                settle(lambda: app.window.get_visible() and workspace.entry.identifier == entry.identifier)
                checks.append("Production floating Copy and Open actions; only private X11 clipboard used")
            elif scenario == "expanded":
                workspace.result_widgets[0].get_buffer().set_text(EDITED)
                hold(1)
                workspace.save_buttons[0].emit("clicked")
                assert app.conversation_store.source_text(entry) == EDITED
                checks.append("Native document editor and Save edits action persisted edited source separately")
            elif scenario == "history":
                workspace.search.set_text("Friday")
                settle(lambda: len(list(descendants(workspace.history_list))) > 0)
                hold(0.5)
                rows = [child for child in descendants(workspace.history_list) if isinstance(child, Gtk.ListBoxRow)]
                assert len(rows) == 1
                workspace.history_list.emit("row-activated", rows[0])
                checks.append("Search history filtered to one matching note and opened it")
            elif scenario == "export":
                for label, suffix in (("Export Markdown", ".md"), ("Export JSON", ".json")):
                    button = next(
                        child
                        for child in descendants(app.history_page.entry_rows[entry.identifier])
                        if isinstance(child, Gtk.Button) and child.get_label() == label
                    )
                    button.emit("clicked")
                    exported = next(app.history_page.export_directory.glob("*" + suffix))
                    assert SOURCE in exported.read_text()
                    shutil.copy2(exported, output / ("export" + suffix))
                checks.append("Both visible export buttons produced files containing the unchanged original")
            elif scenario == "providers":
                page = app.workspace_settings_pages[1]
                page.speech.provider_row.set_selected(
                    [provider.id for provider in page.speech.providers].index("voxtype")
                )
                assert page.rewrite.provider.id == "codex"
                hold(1.6)
                page.speech.provider_row.set_selected(0)
                page.rewrite.provider_row.set_selected(
                    [provider.id for provider in page.rewrite.providers].index("litellm")
                )
                assert page.speech.provider.id == "elevenlabs"
                hold(1.6)
                page.rewrite.provider_row.set_selected(0)
                checks.append("STT and LLM selections changed independently; no catalog or provider request")
            assert app.history_store.find(entry.identifier).raw_text == SOURCE
            checks.append("Raw input unchanged; no automatic paste or inference")
            hold(2.5)
            photograph("after")
            hold(1)
            (output / "recognition.json").write_text(
                json.dumps(asdict(app.history_store.find(entry.identifier)), indent=2) + "\n"
            )
            (output / "replies.json").write_text(
                json.dumps([asdict(reply) for reply in app.conversation_store.replies(entry.identifier)], indent=2)
                + "\n"
            )
        except Exception:
            errors.append(traceback.format_exc())
        finally:
            if recorder is not None:
                recorder.terminate()
                recorder.wait(timeout=10)
            app.quit()
        return GLib.SOURCE_REMOVE

    GLib.timeout_add(700, exercise)
    with (
        patch("voice_scribe_linux.app.FocusedTextTargetTracker", return_value=None),
        patch("voice_scribe_linux.app.PipeWireDeviceCatalog.from_system", return_value=PipeWireDeviceCatalog()),
        patch("voice_scribe_linux.app.deliver_text", side_effect=copy_text),
    ):
        app.run([])
    for child in reversed(children):
        if child.poll() is None:
            child.terminate()
            child.wait(timeout=10)
    for log in logs:
        log.close()
    (output / "capture.json").write_text(
        json.dumps(
            {
                "scenario": scenario,
                "runtime_commit": os.environ["FILM_REVISION"],
                "runtime_files": "runtime-files.json",
                "harness": "harness/",
                "display": ":195",
                "screen": [1920, 1080],
                "application": [1840, 920],
                "font": "Inter 15",
                "resolved_font": subprocess.check_output(["fc-match", "-f", "%{family}", "Inter"], text=True),
                "export_directory": "Isolated session/Exports for the export fixture",
                "disclosure": (
                    "Production UI; scripted input and local rewrite fixture; "
                    "no recognition, inference or provider timing."
                ),
                "isolation": (
                    "Private display, D-Bus, AT-SPI, XDG and clipboard; audio/provider/portal/input services disabled"
                ),
                "events": events,
                "checks": checks,
                "errors": errors,
                "provider_calls": 0,
            },
            indent=2,
        )
        + "\n"
    )
    if errors:
        print(errors[0])
    return 1 if errors else 0


if __name__ == "__main__":
    raise SystemExit(main())
