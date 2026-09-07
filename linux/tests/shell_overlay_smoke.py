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
    environment = {**os.environ, "MLUVA_SHELL_COMMAND": str(Path(__file__).resolve().parents[1] / "mluva-shell")}
    fixture = output / "shell.qml"
    fixture.write_text(
        Path(__file__)
        .with_name("shell_overlay_fixture.qml")
        .read_text()
        .replace('"../quickshell/mluva.dictation"', '"./mluva.dictation"')
    )
    shutil.copytree(Path(__file__).resolve().parents[1] / "quickshell/mluva.dictation", output / "mluva.dictation")
    with (output / "quickshell.log").open("w") as log:
        process = subprocess.Popen(
            ["quickshell", "--no-color", "-p", str(fixture)],
            env=environment,
            stdout=log,
            stderr=subprocess.STDOUT,
        )

        def observe(phase: str) -> dict[str, object]:
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
                    if state["phase"] == phase and state == previous:
                        receipts.append(state)
                        return state
                    previous = state
                time.sleep(0.05)
            raise AssertionError(f"Widget did not settle in {phase}: {previous}")

        try:
            idle = observe("idle")
            assert not idle["visible"]
            focus_editor()
            idle = observe("idle")
            assert idle["focus"]
            for phase in ("preparing", "recording", "processing", "copied", "error"):
                preview = ("Earlier words " * 100 + "LATEST WORDS: Žluťoučký kůň") if phase == "recording" else ""
                publisher.publish(RecordingOverlayState(phase=phase, elapsed_seconds=73, level=0.4, preview=preview))
                state = observe(phase)
                assert state["visible"] and not state["focusable"] and state["mask"]
                assert state["focus"] == idle["focus"]
                assert state["width"] <= state["screenWidth"] - 32
                assert state["height"] + state["bottom"] <= state["screenHeight"]
                assert state["preview"] == preview[-180:]
                subprocess.run(["import", "-window", "root", str(output / f"{phase}.png")], check=True)
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
