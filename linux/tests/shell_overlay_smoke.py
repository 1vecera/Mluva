"""Exercise the real Quickshell widget and bridge against a synthetic private-bus publisher."""

import ctypes
import json
import os
import shutil
import subprocess
import time
from pathlib import Path

import gi
from preview_replay import replay_preview

from mluva_linux.overlay_state import RecordingOverlayPublisher, RecordingOverlayState

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
    exported = connection.export_action_group("/com/mluva/Linux", actions)
    owner = Gio.bus_own_name_on_connection(connection, "com.mluva.Linux", Gio.BusNameOwnerFlags.NONE, None, None)
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
    source = Path(
        os.environ.get("MLUVA_OVERLAY_SOURCE", Path(__file__).resolve().parents[1] / "quickshell/mluva.dictation")
    )
    shutil.copytree(source, output / "mluva.dictation")
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

        def log_lines() -> list[str]:
            """Read only complete records; a long snapshot can span several pipe writes."""
            return [line.decode("utf-8") for line in (output / "quickshell.log").read_bytes().split(b"\n")[:-1]]

        def observe(phase: str, preview: str | None = None) -> dict[str, object]:
            """Wait for a frame-stable snapshot from the separately running production QML."""
            deadline = time.monotonic() + 30
            previous = None
            observed_lines = len(log_lines())
            while time.monotonic() < deadline:
                while GLib.MainContext.default().pending():
                    GLib.MainContext.default().iteration(False)
                if process.poll() is not None:
                    raise RuntimeError(f"Quickshell exited; inspect {output / 'quickshell.log'}")
                lines = log_lines()
                payloads = [
                    line.split("MLUVA_SNAPSHOT ", 1)[1] for line in lines[observed_lines:] if "MLUVA_SNAPSHOT " in line
                ]
                observed_lines = len(lines)
                if payloads:
                    state = json.loads(payloads[-1])
                    stable = {key: value for key, value in state.items() if key not in ("dotOpacity", "dotScale")}
                    if (
                        state["phase"] == phase
                        and stable == previous
                        and (preview is None or state["preview"] == preview)
                    ):
                        receipts.append(state)
                        return state
                    previous = stable
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

        def motion_frames(preview: str, **preferences) -> list[dict[str, object]]:
            """Observe intermediate production animation frames as real D-Bus text updates arrive."""
            start = len(log_lines())
            publisher.publish(RecordingOverlayState(phase="recording", preview=preview, **preferences))
            deadline = time.monotonic() + 1.4
            while time.monotonic() < deadline:
                while GLib.MainContext.default().pending():
                    GLib.MainContext.default().iteration(False)
                time.sleep(0.01)
            frames = [
                json.loads(line.split("MLUVA_SNAPSHOT ", 1)[1])
                for line in log_lines()[start:]
                if "MLUVA_SNAPSHOT " in line
            ]
            return [frame for frame in frames if frame["preview"] == preview[frame["previewStart"] :]]

        try:
            idle = observe("idle")
            assert not idle["visible"]
            focus_editor()
            idle = observe("idle")
            assert idle["focus"]
            ipc("theme", "false")
            if os.environ.get("MLUVA_PANEL_REPLAY") == "1":
                replay_preview(publisher, observe, output)
                return
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
                    assert state["headerVisible"] and state["timerVisible"] and state["timerText"] == "01:13"
                    assert state["dotLabel"] == "Recording" and state["statusText"] == ""
                    assert state["headerBottom"] < state["viewportTop"]
                    assert state["viewportHeight"] == state["lineHeight"] * 5
                    assert state["textY"] < 0
                    assert abs(state["textY"] - state["targetY"]) < 0.1
                    assert 0.75 <= state["surfaceOpacity"] <= 0.85
                subprocess.run(["import", "-window", "root", str(output / f"{phase}.png")], check=True)
            samples = json.loads(
                subprocess.check_output(
                    ["quickshell", "ipc", "--pid", str(process.pid), "call", "fixture", "motionSamples"],
                    text=True,
                )
            )
            assert samples["nearEdge"] and samples["wrapped"].startswith(samples["nearEdge"])
            publisher.publish(RecordingOverlayState(phase="recording", preview=samples["shortText"]))
            short = observe("recording", samples["shortText"])
            assert short["lineCount"] == 4 and short["textY"] == 0
            near = motion_frames(samples["nearEdge"])
            assert near[-1]["lineCount"] == 5 and near[-1]["lookAhead"] > 0
            wrapped = motion_frames(samples["wrapped"])
            assert wrapped[-1]["lineCount"] == 6
            for initial, frames in ((short, near), (near[-1], wrapped)):
                target = frames[-1]["targetY"]
                assert target < initial["textY"]
                assert any(target + 0.1 < frame["textY"] < initial["textY"] - 0.1 for frame in frames), frames
                assert abs(frames[-1]["textY"] - target) < 0.1
                assert all(frame["height"] == short["height"] for frame in frames)
                assert all(
                    abs(after["textY"] - before["textY"]) < short["lineHeight"] * 0.65
                    for before, after in zip([initial, *frames], frames, strict=False)
                ), frames
            position = countdown()
            assert abs(position["x"] - (short["screenWidth"] - short["width"]) / 2) <= 1
            (output / "scroll-motion.json").write_text(
                json.dumps({"before": short, "near_edge": near, "wrap": wrapped}, indent=2)
            )
            subprocess.run(["import", "-window", "root", str(output / "five-lines.png")], check=True)
            # A committed segment can replace a much longer provisional preview.
            # The viewport must never keep the old offset after that text shrinks.
            long_preview = samples["wrapped"] * 3
            publisher.publish(RecordingOverlayState(phase="recording", preview=long_preview))
            before_contraction = observe("recording", long_preview)
            contraction = motion_frames(samples["wrapped"])
            (output / "preview-contraction.json").write_text(
                json.dumps({"before": before_contraction, "frames": contraction}, indent=2)
            )
            geometry_keys = ("x", "y", "width", "height", "viewportTop", "viewportHeight")
            assert contraction and all(frame["visible"] for frame in contraction)
            assert all(all(frame[key] == before_contraction[key] for key in geometry_keys) for frame in contraction), (
                contraction
            )
            assert all(frame["headerVisible"] and frame["timerVisible"] for frame in contraction)
            assert all(
                frame["textY"] + frame["textHeight"] >= frame["viewportHeight"] - frame["lineHeight"] * 1.5
                for frame in contraction
            ), "A shortened recognition preview scrolled out of its viewport; inspect preview-contraction.json"
            publisher.publish(RecordingOverlayState(phase="processing", preview=samples["wrapped"]))
            processing = observe("processing", samples["wrapped"])
            assert processing["visible"] and all(processing[key] == short[key] for key in geometry_keys)
            publisher.publish(RecordingOverlayState(phase="recording", preview=samples["wrapped"]))
            observe("recording", samples["wrapped"])
            pulse = motion_frames(samples["wrapped"])
            opacity = [frame["dotOpacity"] for frame in pulse]
            scale = [frame["dotScale"] for frame in pulse]
            assert len({round(value, 3) for value in scale}) >= 10, scale
            assert min(scale) >= 0.82 and max(scale) <= 1.18 and max(scale) - min(scale) > 0.16
            assert min(opacity) >= 0.84 and max(opacity) <= 1
            assert all(abs(a - b) < 0.06 for a, b in zip(scale, scale[1:], strict=False)), scale
            fixed_keys = ("dotCenterX", "dotCenterY", "dotWidth", "dotHeight", *geometry_keys)
            assert all(all(frame[key] == pulse[0][key] for key in fixed_keys) for frame in pulse), pulse
            for preferences in ({"smooth_scrolling": False}, {"scroll_duration_ms": 0}):
                still = motion_frames(samples["wrapped"] + " Motion is disabled.", **preferences)
                assert all(frame["dotScale"] == 1 and frame["dotOpacity"] == 0.92 for frame in still), still
                assert all(abs(frame["textY"] - frame["targetY"]) < 0.1 for frame in still), still
            (output / "recording-pulse.json").write_text(
                json.dumps(
                    {
                        "scale_frames": scale,
                        "opacity_frames": opacity,
                        "disabled_is_static": True,
                        "fixed_geometry": True,
                    },
                    indent=2,
                )
            )
            full_text = " ".join(f"{'🙂' if index < 12 else 'w'}{index:04d}" for index in range(670))
            assert len(full_text) < 4096
            publisher.publish(RecordingOverlayState(phase="recording", preview=full_text))
            observe("recording", full_text)
            retained_tail = []
            for index in range(8):
                full_text += f" x{index:04d} y{index:04d} café🙂 z{index:04d}"
                motion_frames(full_text)
                state = observe("recording")
                expected = json.loads(
                    subprocess.check_output(
                        ["quickshell", "ipc", "--pid", str(process.pid), "call", "fixture", "expectedTail", full_text],
                        text=True,
                    )
                )
                assert abs(state["lastLineFill"] - expected["lastFill"]) < 0.002, (state, expected)
                unanchored = json.loads(
                    subprocess.check_output(
                        [
                            "quickshell",
                            "ipc",
                            "--pid",
                            str(process.pid),
                            "call",
                            "fixture",
                            "expectedTail",
                            state["preview"],
                        ],
                        text=True,
                    )
                )
                assert state["preview"] == full_text[state["previewStart"] :]
                assert len(state["preview"]) <= 4096
                retained_tail.append(
                    {
                        "offset": state["previewStart"],
                        "actual": state["lastLineFill"],
                        "expected": expected["lastFill"],
                        "without_anchor": unanchored["lastFill"],
                    }
                )
            assert retained_tail[-1]["offset"] > 0 and state["discardedHeight"] > 0
            assert any(abs(frame["without_anchor"] - frame["expected"]) > 0.05 for frame in retained_tail)
            (output / "bounded-preview.json").write_text(json.dumps(retained_tail, indent=2))
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
            assert 0 < countdown()["remaining"] < 4000
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
            time.sleep(4.4)
            assert countdown()["remaining"] == 4000 and countdown()["visible"]
            publisher.publish(
                RecordingOverlayState(phase="ready", preview="Finished result", review_identifier="timed-note")
            )
            observe("ready")
            assert countdown()["remaining"] > 3000
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
                        "duration_ms": 4000,
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
            publisher.publish(
                RecordingOverlayState(
                    phase="ready",
                    preview="Custom settings",
                    review_identifier="custom-settings",
                    review_timeout_seconds=3,
                    show_copy_action=False,
                    smooth_scrolling=False,
                    scroll_duration_ms=1200,
                    scroll_lookahead_lines=0,
                )
            )
            configured = observe("ready")
            assert configured["reviewDuration"] == 3000 and not configured["copyVisible"]
            assert configured["scrollDuration"] == 1200 and not configured["smoothScrolling"]
            assert configured["scrollLookahead"] == 0
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
