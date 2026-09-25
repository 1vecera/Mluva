"""Record the production Omarchy widget through a scripted dictation on the isolated display.

Run from the repository root through the private X11 runner, for example:

    env -i PATH="$PATH" HOME="$HOME" USER="$USER" LANG=C.UTF-8 \
        OFFSCREEN_ENABLE_ATSPI=1 OFFSCREEN_SCREEN_SPEC=3840x2400x24 \
        bash dev/run-isolated.sh tmp/showreel/widget -- \
        env PYTHONPATH=linux GTK_A11Y=none GSK_RENDERER=cairo QT_SCALE_FACTOR=3 \
        uv run --project linux --locked python launch-video/showreel/capture_widget.py

The production QML, bridge and Omarchy controls paint every pixel. Only the provider events are
scripted: preview text, elapsed time and input level arrive over the same private D-Bus signal the
app publishes, so no microphone, provider or credential is involved.
"""

import json
import math
import os
import random
import shutil
import subprocess
import time
from pathlib import Path

import gi

from mluva_linux.overlay_state import RecordingOverlayPublisher, RecordingOverlayState
from mluva_linux.personalization import BUILT_IN_STYLES

gi.require_version("Gio", "2.0")
from gi.repository import Gio, GLib  # noqa: E402

REPO = Path(__file__).resolve().parents[2]
THEMES = Path("/usr/share/omarchy/themes")
THEME = THEMES / os.environ.get("MLUVA_CAPTURE_THEME", "vantablack")
MODE = os.environ.get("MLUVA_CAPTURE_MODE", "take")
SCALE = int(os.environ.get("QT_SCALE_FACTOR", "1"))
MARGIN = 24
TEXT = os.environ.get(
    "MLUVA_CAPTURE_TEXT", "Quick note for the team: the release is ready and we ship on Friday. Coffee is on me."
)
# Showreel pacing: the chapter compresses a spoken note into about one second of streaming,
# while the widget's timer keeps natural speech time (about 156 words per minute), like a time-lapse.
WORDS_PER_SECOND = float(os.environ.get("MLUVA_CAPTURE_WPS", "16"))
NATURAL_WORDS_PER_SECOND = 2.6
LEAD_IN = 0.4
PROCESSING_HOLD = 0.3
REVIEW_HOLD = 2.0
# Extra fixture IPC: load a real Omarchy colors.toml into the shell's Color singleton.
THEME_IPC = "        function themeRaw(raw: string): void { Color.loadColors(raw); }\n"


def pump(seconds: float = 0.0) -> None:
    """Dispatch the private bus while the independent Quickshell process paints."""
    deadline = time.monotonic() + seconds
    while True:
        while GLib.MainContext.default().pending():
            GLib.MainContext.default().iteration(False)
        if time.monotonic() >= deadline:
            return
        time.sleep(0.002)


def chunks(text: str, seed: int = 7) -> list[tuple[float, str]]:
    """Group words the way streamed recognition commits them: one to three at a time."""
    rng = random.Random(seed)
    words = text.split()
    events, index, clock = [], 0, 0.0
    while index < len(words):
        size = rng.choice((1, 1, 2, 2, 3))
        index = min(len(words), index + size)
        clock += size / WORDS_PER_SECOND * rng.uniform(0.8, 1.2)
        if words[index - 1].endswith((".", ":", ",")):
            clock += rng.uniform(0.08, 0.2)
        events.append((round(clock, 3), " ".join(words[:index])))
    return events


def level_at(t: float) -> float:
    """A speech-like input envelope: syllable pulses under slower phrase swells."""
    syllables = 0.5 + 0.5 * math.sin(t * 2 * math.pi * 4.3) * math.sin(t * 2 * math.pi * 1.7 + 0.6)
    phrase = 0.55 + 0.45 * math.sin(t * 2 * math.pi * 0.37 + 1.1)
    return max(0.05, min(1.0, 0.12 + 0.78 * syllables * phrase))


def grab(region: dict, path: Path) -> None:
    """Save one lossless frame of the private display region."""
    subprocess.run(
        [
            "ffmpeg", "-nostdin", "-y", "-v", "error",
            "-f", "x11grab", "-draw_mouse", "0",
            "-video_size", f"{region['width']}x{region['height']}",
            "-i", f"{os.environ['DISPLAY']}+{region['x']},{region['y']}",
            "-frames:v", "1", str(path),
        ],
        check=True,
    )  # fmt: skip


def capture_themes(publisher, state, settle, ipc, region: dict, output: Path) -> None:
    """Photograph the recording and review states under every installed Omarchy theme."""
    folder = output / "themes"
    folder.mkdir()
    preview = " ".join(TEXT.split()[:15])
    records = []
    for theme in sorted(path for path in THEMES.iterdir() if (path / "colors.toml").is_file()):
        ipc("themeRaw", (theme / "colors.toml").read_text())
        publisher.publish(state("recording", preview, 7, 0.55))
        recording = settle("recording")
        pump(0.6)
        grab(region, folder / f"{theme.name}-recording.png")
        publisher.publish(state("ready", TEXT, 9))
        settle("ready")
        pump(0.6)
        grab(region, folder / f"{theme.name}-review.png")
        records.append({"theme": theme.name, "background": recording["background"], "ink": recording["ink"]})
        publisher.clear()
        settle("idle")
    (folder / "themes.json").write_text(json.dumps({"region": region, "scale": SCALE, "themes": records}, indent=2))


def main() -> None:
    """Probe the widget geometry, then record one lossless take at 60 fps."""
    if "OFFSCREEN_SESSION_ROOT" not in os.environ:
        raise RuntimeError("Use the isolated X11 runner.")
    output = Path(os.environ["OFFSCREEN_ARTIFACT_DIR"])
    connection = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    publisher = RecordingOverlayPublisher(connection)
    actions = Gio.SimpleActionGroup()
    status = Gio.SimpleAction.new("status", None)
    status.connect("activate", lambda *_args: publisher.replay())
    actions.add_action(status)
    review = Gio.SimpleAction.new("review", GLib.VariantType.new("(sss)"))
    actions.add_action(review)
    connection.export_action_group("/com/mluva/Linux", actions)
    Gio.bus_own_name_on_connection(connection, "com.mluva.Linux", Gio.BusNameOwnerFlags.NONE, None, None)

    fixture = output / "shell.qml"
    fixture.write_text(
        (REPO / "linux/tests/shell_overlay_fixture.qml")
        .read_text()
        .replace('"../quickshell/mluva.dictation"', '"./mluva.dictation"')
        .replace("        function closeMenu(): void", THEME_IPC + "        function closeMenu(): void")
    )
    shutil.copytree(REPO / "linux/quickshell/mluva.dictation", output / "mluva.dictation")
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
    environment = {**os.environ, "MLUVA_SHELL_COMMAND": str(REPO / "linux/mluva-shell")}
    environment.pop("HYPRLAND_INSTANCE_SIGNATURE", None)
    environment["PATH"] = str(binaries) + os.pathsep + environment["PATH"]

    log_path = output / "quickshell.log"
    with log_path.open("w") as log:
        process = subprocess.Popen(
            ["quickshell", "--no-color", "-p", str(fixture)], env=environment, stdout=log, stderr=subprocess.STDOUT
        )
        try:

            def snapshots() -> list[dict]:
                lines = log_path.read_bytes().split(b"\n")[:-1]
                return [
                    json.loads(line.decode().split("MLUVA_SNAPSHOT ", 1)[1])
                    for line in lines
                    if b"MLUVA_SNAPSHOT " in line
                ]

            def settle(phase: str, timeout: float = 30) -> dict:
                deadline = time.monotonic() + timeout
                while time.monotonic() < deadline:
                    pump(0.1)
                    if process.poll() is not None:
                        raise RuntimeError(f"Quickshell exited; inspect {log_path}")
                    recent = [state for state in snapshots()[-3:] if state.get("phase") == phase]
                    if len(recent) == 3 and recent[0].get("y") == recent[-1].get("y"):
                        return recent[-1]
                raise AssertionError(f"Widget did not settle in {phase}")

            def ipc(*arguments: str) -> None:
                subprocess.run(
                    ["quickshell", "ipc", "--pid", str(process.pid), "call", "fixture", *arguments], check=True
                )

            settle("idle")
            ipc("themeRaw", (THEME / "colors.toml").read_text())
            options = tuple((style.identifier, style.name) for style in BUILT_IN_STYLES)

            def state(phase: str, preview: str, elapsed: int, level: float = 0.0) -> RecordingOverlayState:
                return RecordingOverlayState(
                    phase=phase,
                    preview=preview,
                    elapsed_seconds=elapsed,
                    level=level,
                    review_identifier="showreel-take" if phase == "ready" else "",
                    review_options=options if phase == "ready" else (),
                )

            # Probe the final review geometry so the region contains every phase.
            publisher.publish(state("recording", TEXT, 12, 0.5))
            recording = settle("recording")
            publisher.publish(state("ready", TEXT, 12))
            ready = settle("ready")
            publisher.clear()
            settle("idle")
            pump(1.5)

            left = min(recording["x"], ready["x"]) - MARGIN
            top = min(recording["y"], ready["y"]) - MARGIN
            right = max(recording["x"] + recording["width"], ready["x"] + ready["width"]) + MARGIN
            bottom = max(recording["y"] + recording["height"], ready["y"] + ready["height"]) + MARGIN
            region = {
                "x": int(left * SCALE),
                "y": int(top * SCALE),
                "width": int((right - left) * SCALE) // 2 * 2,
                "height": int((bottom - top) * SCALE) // 2 * 2,
            }
            if MODE == "themes":
                capture_themes(publisher, state, settle, ipc, region, output)
                return
            events = chunks(TEXT)
            speech_end = LEAD_IN + events[-1][0]
            duration = speech_end + 0.9 + REVIEW_HOLD
            with (output / "capture.log").open("w") as capture_log:
                capture = subprocess.Popen(
                    [
                        "ffmpeg", "-nostdin", "-y", "-v", "error",
                        "-f", "x11grab", "-draw_mouse", "0", "-framerate", "60",
                        "-video_size", f"{region['width']}x{region['height']}",
                        "-i", f"{os.environ['DISPLAY']}+{region['x']},{region['y']}",
                        "-t", f"{duration + 0.5:.2f}",
                        "-c:v", "libx264rgb", "-preset", "ultrafast", "-crf", "0",
                        str(output / "widget-take.mkv"),
                    ],
                    stderr=capture_log,
                )  # fmt: skip
                pump(0.5)
                started = time.monotonic()
                timeline = []
                preview, next_event = "", 0
                publisher.publish(state("recording", "", 0, 0.1))
                timeline.append({"t": 0.0, "phase": "recording", "preview": ""})
                while (now := time.monotonic() - started) < speech_end + 0.35:
                    while next_event < len(events) and now >= LEAD_IN + events[next_event][0]:
                        preview = events[next_event][1]
                        timeline.append({"t": round(now, 3), "phase": "recording", "preview": preview})
                        next_event += 1
                    speaking = LEAD_IN - 0.15 < now < speech_end
                    spoken = int(len(preview.split()) / NATURAL_WORDS_PER_SECOND)
                    publisher.publish(state("recording", preview, spoken, level_at(now) if speaking else 0.04))
                    pump(0.05)
                spoken = int(len(TEXT.split()) / NATURAL_WORDS_PER_SECOND)
                publisher.publish(state("processing", TEXT, spoken))
                timeline.append({"t": round(time.monotonic() - started, 3), "phase": "processing"})
                pump(PROCESSING_HOLD)
                publisher.publish(state("ready", TEXT, spoken))
                timeline.append({"t": round(time.monotonic() - started, 3), "phase": "ready"})
                while capture.poll() is None and time.monotonic() - started < duration + 5:
                    pump(0.05)
                assert capture.poll() == 0, "Private capture failed; inspect capture.log"
            (output / "widget-take.json").write_text(
                json.dumps(
                    {
                        "boundary": "Production QML and Omarchy controls on a private X11/D-Bus session; "
                        "scripted recognition events, no microphone or provider",
                        "theme": str(THEME),
                        "scale": SCALE,
                        "region": region,
                        "capture_started_before_first_event": 0.5,
                        "text": TEXT,
                        "timer_words_per_second": NATURAL_WORDS_PER_SECOND,
                        "timeline": timeline,
                        "recording": recording,
                        "ready": ready,
                    },
                    indent=2,
                )
            )
        finally:
            process.terminate()
            process.wait(timeout=10)


if __name__ == "__main__":
    main()
