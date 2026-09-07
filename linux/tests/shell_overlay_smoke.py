"""Exercise the real Quickshell widget and bridge against a synthetic private-bus publisher."""

import ctypes
import json
import os
import shutil
import subprocess
import time
from pathlib import Path

import gi

from voice_scribe_linux.overlay_state import RecordingOverlayPublisher, RecordingOverlayState

gi.require_version("Gio", "2.0")
from gi.repository import Gio, GLib  # noqa: E402


def focus_editor() -> None:
    """Focus only the synthetic editor on the runner's private X11 display."""
    x11 = ctypes.CDLL("libX11.so.6")
    x11.XOpenDisplay.restype = ctypes.c_void_p
    x11.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
    x11.XDefaultRootWindow.restype = ctypes.c_ulong
    x11.XQueryTree.argtypes = [
        ctypes.c_void_p,
        ctypes.c_ulong,
        ctypes.POINTER(ctypes.c_ulong),
        ctypes.POINTER(ctypes.c_ulong),
        ctypes.POINTER(ctypes.POINTER(ctypes.c_ulong)),
        ctypes.POINTER(ctypes.c_uint),
    ]
    x11.XFetchName.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(ctypes.c_char_p)]
    x11.XSetInputFocus.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_ulong]
    x11.XSync.argtypes = [ctypes.c_void_p, ctypes.c_int]
    x11.XCloseDisplay.argtypes = [ctypes.c_void_p]
    x11.XFree.argtypes = [ctypes.c_void_p]
    display = x11.XOpenDisplay(None)
    root, parent = ctypes.c_ulong(), ctypes.c_ulong()
    children, count = ctypes.POINTER(ctypes.c_ulong)(), ctypes.c_uint()
    try:
        assert x11.XQueryTree(
            display,
            x11.XDefaultRootWindow(display),
            ctypes.byref(root),
            ctypes.byref(parent),
            ctypes.byref(children),
            ctypes.byref(count),
        )
        found = False
        for index in range(count.value):
            name = ctypes.c_char_p()
            x11.XFetchName(display, children[index], ctypes.byref(name))
            if name.value == b"Mluva overlay fixture":
                x11.XSetInputFocus(display, children[index], 1, 0)
                found = True
            if name:
                x11.XFree(name)
        x11.XSync(display, False)
        assert found, "Synthetic editor was not mapped on the private display"
    finally:
        x11.XFree(children)
        x11.XCloseDisplay(display)


def move_pointer(x: int, y: int) -> None:
    """Move only the synthetic pointer on the explicitly isolated X11 connection."""
    x11 = ctypes.CDLL("libX11.so.6")
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
    x11.XSync.argtypes = [ctypes.c_void_p, ctypes.c_int]
    x11.XCloseDisplay.argtypes = [ctypes.c_void_p]
    display = x11.XOpenDisplay(None)
    try:
        x11.XWarpPointer(display, 0, x11.XDefaultRootWindow(display), 0, 0, 0, 0, x, y)
        x11.XSync(display, False)
    finally:
        x11.XCloseDisplay(display)


def main() -> None:
    """Prove preview, lifecycle and focus behavior without microphone or live desktop access."""
    if "OFFSCREEN_SESSION_ROOT" not in os.environ:
        raise RuntimeError("Use the isolated X11 runner.")
    output = Path(os.environ["OFFSCREEN_ARTIFACT_DIR"])
    connection = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    publisher = RecordingOverlayPublisher(connection)
    actions = Gio.SimpleActionGroup()
    action = Gio.SimpleAction.new("status", None)
    action.connect("activate", lambda *_args: publisher.replay())
    actions.add_action(action)
    exported = connection.export_action_group("/com/voicescribe/Linux", actions)
    owner = Gio.bus_own_name_on_connection(connection, "com.voicescribe.Linux", Gio.BusNameOwnerFlags.NONE, None, None)
    receipts = []
    commands = []
    review_action = Gio.SimpleAction.new("review", GLib.VariantType.new("(sss)"))
    review_action.connect("activate", lambda _action, parameters: commands.append(parameters.unpack()))
    actions.add_action(review_action)
    environment = {**os.environ, "MLUVA_SHELL_COMMAND": str(Path(__file__).resolve().parents[1] / "mluva-shell")}
    fixture = output / "shell.qml"
    fixture.write_text(
        Path(__file__)
        .with_name("shell_overlay_fixture.qml")
        .read_text()
        .replace('"../quickshell/mluva.dictation"', '"./mluva.dictation"')
    )
    shutil.copytree(Path(__file__).resolve().parents[1] / "quickshell/mluva.dictation", output / "mluva.dictation")
    # Use the installed Omarchy controls, redirecting only their desktop reads to
    # the runner's private configuration. Never switch the user's active theme.
    for module in ("Commons", "Ui"):
        shutil.copytree(Path("/usr/share/omarchy/shell") / module, output / module)
        for qml in (output / module).glob("*.qml"):
            qml.write_text(
                qml.read_text().replace('Quickshell.env("HOME")', 'Quickshell.env("OFFSCREEN_SESSION_ROOT")')
            )
    binaries = output / "bin"
    binaries.mkdir()
    hyprctl = binaries / "hyprctl"
    hyprctl.write_text("#!/bin/sh\nprintf '{\"int\":0}\\n'\n")
    hyprctl.chmod(0o700)
    environment["PATH"] = str(binaries) + os.pathsep + environment["PATH"]
    with (output / "quickshell.log").open("w") as log:
        process = subprocess.Popen(
            ["quickshell", "--no-color", "-p", str(fixture)],
            env=environment,
            stdout=log,
            stderr=subprocess.STDOUT,
        )

        def observe(phase: str, preview: str | None = None) -> dict[str, object]:
            """Wait for a frame-stable snapshot from the separately running production QML."""
            deadline = time.monotonic() + 30
            previous = None
            observed_lines = len((output / "quickshell.log").read_text().splitlines())
            while time.monotonic() < deadline:
                while GLib.MainContext.default().pending():
                    GLib.MainContext.default().iteration(False)
                if process.poll() is not None:
                    raise RuntimeError(f"Quickshell exited; inspect {output / 'quickshell.log'}")
                lines = (output / "quickshell.log").read_text().splitlines()
                payloads = [
                    line.split("MLUVA_SNAPSHOT ", 1)[1] for line in lines[observed_lines:] if "MLUVA_SNAPSHOT " in line
                ]
                observed_lines = len(lines)
                if payloads:
                    state = json.loads(payloads[-1])
                    if (
                        state["phase"] == phase
                        and state == previous
                        and (preview is None or state["preview"] == preview)
                    ):
                        receipts.append(state)
                        return state
                    previous = state
                time.sleep(0.05)
            raise AssertionError(f"Widget did not settle in {phase}: {previous}")

        def ipc(*arguments: str) -> None:
            """Drive only the retained private Quickshell process."""
            subprocess.run(["quickshell", "ipc", "--pid", str(process.pid), "call", "fixture", *arguments], check=True)

        def expect_commands(expected: list[tuple[str, str, str]]) -> None:
            """Wait for the separate control process's D-Bus delivery, independently of frame timing."""
            deadline = time.monotonic() + 5
            while len(commands) < len(expected) and time.monotonic() < deadline:
                while GLib.MainContext.default().pending():
                    GLib.MainContext.default().iteration(False)
                time.sleep(0.01)
            assert commands == expected, commands

        def countdown() -> dict[str, object]:
            """Read timer state from the independently running production widget."""
            result = subprocess.run(
                ["quickshell", "ipc", "--pid", str(process.pid), "call", "fixture", "countdown"],
                check=True,
                capture_output=True,
                text=True,
            )
            return json.loads(result.stdout)

        try:
            idle = observe("idle")
            assert not idle["visible"]
            focus_editor()
            idle = observe("idle")
            assert idle["focus"]
            ipc("theme", "false")
            for phase in ("preparing", "recording", "processing", "error"):
                preview = ("Earlier words " * 100 + "LATEST WORDS: Žluťoučký kůň") if phase == "recording" else ""
                publisher.publish(RecordingOverlayState(phase=phase, elapsed_seconds=73, level=0.4, preview=preview))
                state = observe(phase)
                assert state["visible"] and not state["focusable"] and state["mask"]
                assert state["focus"] == idle["focus"]
                assert state["width"] <= state["screenWidth"] - 32
                assert state["height"] + state["bottom"] <= state["screenHeight"]
                assert state["preview"] == preview[-4096:]
                if phase == "recording":
                    assert state["viewportHeight"] == state["lineHeight"] * 3
                    assert state["textY"] < 0
                    assert state["textY"] + state["textHeight"] == state["viewportHeight"]
                subprocess.run(["import", "-window", "root", str(output / f"{phase}.png")], check=True)
            for light in (False, True):
                ipc("theme", str(light).lower())
                publisher.publish(
                    RecordingOverlayState(
                        phase="ready",
                        preview="A concise note with enough room to review the final words before choosing a rewrite.",
                        review_identifier="synthetic-note",
                        review_options=(("email", "Email"), ("tasks", "Tasks"), ("custom", "Saved prompt")),
                    )
                )
                state = observe("ready")
                assert state["focusable"] and state["identifier"] == "synthetic-note"
                assert state["background"] == ("#faf4ed" if light else "#1a1b26")
                assert commands == []
                subprocess.run(
                    ["import", "-window", "root", str(output / f"ready-{'light' if light else 'dark'}.png")], check=True
                )
                position = countdown()
                geometry = f"{state['width']}x{state['height']}+{position['x']}+{position['y']}"
                subprocess.run(
                    [
                        "import",
                        "-window",
                        "root",
                        "-crop",
                        geometry,
                        str(output / f"widget-{'light' if light else 'dark'}.png"),
                    ],
                    check=True,
                )
                ipc("click", "more")
                assert observe("ready")["menuOpen"]
                subprocess.run(
                    ["import", "-window", "root", str(output / f"menu-{'light' if light else 'dark'}.png")], check=True
                )
            ipc("click", "polish")
            expect_commands([("rewrite", "synthetic-note", "polish")])
            observe("ready")
            assert commands == [("rewrite", "synthetic-note", "polish")]
            ipc("click", "more")
            observe("ready")
            ipc("option", "2")
            expect_commands([("rewrite", "synthetic-note", "polish"), ("rewrite", "synthetic-note", "custom")])
            observe("ready")
            assert commands[-1] == ("rewrite", "synthetic-note", "custom")
            for preview in ("A streamed rewrite", "A streamed rewrite grows as the model responds. " * 8):
                preview = preview.strip()
                publisher.publish(
                    RecordingOverlayState(phase="rewriting", preview=preview, review_identifier="synthetic-note")
                )
                state = observe("rewriting", preview)
                assert state["visible"] and not state["copyEnabled"] and state["renderedText"] == preview
            subprocess.run(["import", "-window", "root", str(output / "streaming.png")], check=True)
            publisher.publish(
                RecordingOverlayState(
                    phase="review-error",
                    preview="Original stays safe",
                    review_identifier="synthetic-note",
                    message="Rewrite failed. Try again or open the note.",
                )
            )
            assert observe("review-error")["visible"]
            publisher.publish(
                RecordingOverlayState(phase="ready", preview="Timed review", review_identifier="timed-note")
            )
            state = observe("ready")
            assert 0 < countdown()["remaining"] < 8000
            position = countdown()
            move_pointer(position["x"] + 20, position["y"] + 20)
            observe("ready")
            hovered = countdown()
            assert hovered["paused"], hovered
            time.sleep(0.6)
            assert countdown()["remaining"] == hovered["remaining"]
            move_pointer(5, 150)
            ipc("click", "more")
            observe("ready")
            paused = countdown()
            assert paused["paused"]
            time.sleep(0.6)
            assert countdown()["remaining"] == paused["remaining"]
            ipc("closeMenu")
            ipc("focusReview")
            observe("ready")
            focused = countdown()
            assert focused["paused"]
            time.sleep(0.6)
            assert countdown()["remaining"] == focused["remaining"]
            focus_editor()
            publisher.publish(
                RecordingOverlayState(phase="rewriting", preview="Working", review_identifier="timed-note")
            )
            observe("rewriting")
            time.sleep(0.6)
            assert countdown()["remaining"] == 8000
            publisher.publish(
                RecordingOverlayState(phase="ready", preview="Finished result", review_identifier="timed-note")
            )
            observe("ready")
            assert countdown()["remaining"] > 7000
            before_dismiss = list(commands)
            deadline = time.monotonic() + 10
            while countdown()["visible"] and time.monotonic() < deadline:
                while GLib.MainContext.default().pending():
                    GLib.MainContext.default().iteration(False)
                time.sleep(0.1)
            assert not countdown()["visible"]
            expect_commands(before_dismiss + [("dismiss", "timed-note", "")])
            (output / "countdown.json").write_text(
                json.dumps(
                    {
                        "duration_ms": 8000,
                        "hover": hovered,
                        "menu": paused,
                        "keyboard": focused,
                        "expired": countdown(),
                        "dismissed_note": "timed-note",
                    },
                    indent=2,
                )
            )
            publisher.clear()
            assert not observe("idle")["visible"]
            publisher.publish(RecordingOverlayState(phase="recording", preview="Must disappear on owner loss"))
            observe("recording")
            Gio.bus_unown_name(owner)
            owner = 0
            stopped = observe("stopped")
            assert not stopped["visible"] and stopped["preview"] == ""
            (output / "receipt.json").write_text(json.dumps(receipts, indent=2))
        finally:
            process.terminate()
            process.wait(timeout=5)
            if owner:
                Gio.bus_unown_name(owner)
            connection.unexport_action_group(exported)


if __name__ == "__main__":
    main()
