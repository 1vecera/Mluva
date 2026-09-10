"""Record production GTK and QML with synthetic input in a private Linux desktop."""

import argparse
import json
import math
import os
import shutil
import sqlite3
import subprocess
import time
import traceback
from dataclasses import replace
from datetime import UTC, datetime, timedelta
from pathlib import Path
from unittest.mock import patch

import gi
from mluva_linux.app import MluvaApplication
from mluva_linux.overlay_state import RecordingOverlayState
from mluva_linux.pipewire import PipeWireDeviceCatalog
from mluva_linux.ui import set_button_content

gi.require_version("Gtk", "4.0")
gi.require_version("GdkX11", "4.0")
from gi.repository import GLib  # noqa: E402


class DemoApplication(MluvaApplication):
    """Keep production views, stores, actions and D-Bus; replace external capture setup."""

    def _initialize_local_services(self) -> None:
        """Use the isolated stores with automatic provider requests disabled."""
        super()._initialize_local_services()
        self.config = replace(self.config, automatic_titles=False)

    def _initialize_capture_services(self) -> None:
        """Never open a microphone, request a portal or resolve a live credential."""
        self.approved_recording_trigger = "F9"
        self._set_status("Press F9 to dictate.")


class DemoRewriteClient:
    """Stream a declared local fixture through the real rewrite lifecycle."""

    def __init__(self, reply: str) -> None:
        """Retain only synthetic copy and a cancellation flag."""
        self.reply = reply
        self.closed = False

    def resolve_model(self, _configured: str | None) -> str:
        """Identify this boundary as a scripted provider, never a real model."""
        return "scripted-demo"

    def transform(self, _prompt, _cwd, _model, *, max_output_characters, on_delta) -> str:
        """Send gradual deltas so production views and completion gates actually run."""
        assert len(self.reply) < max_output_characters
        for offset in range(0, len(self.reply), 4):
            if self.closed:
                raise RuntimeError("Demo rewrite cancelled")
            on_delta(self.reply[offset : offset + 4])
            time.sleep(0.085)
        return self.reply

    def close(self) -> None:
        """Stop only this fixture's pending work."""
        self.closed = True


def main() -> int:
    """Capture a bounded story while retaining source, viewport and interaction evidence."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--format", choices=("landscape", "portrait"), default="landscape")
    parser.add_argument("--theme", choices=("tokyo-night", "rose-pine"), default="tokyo-night")
    args = parser.parse_args()
    if "OFFSCREEN_SESSION_ROOT" not in os.environ or os.environ.get("GDK_BACKEND") != "x11":
        raise RuntimeError("Run through dev/run-isolated.sh inside the dev box.")
    if os.environ.get("WAYLAND_DISPLAY") or os.environ.get("ELEVENLABS_API_KEY"):
        raise RuntimeError("The demo must not inherit a live desktop or recognition credential.")
    root = Path(__file__).resolve().parents[1]
    output = Path(os.environ["OFFSCREEN_ARTIFACT_DIR"])
    story = json.loads((root / "dev/promotion-story.json").read_text())
    os.environ.update(MLUVA_PROMO_FORMAT=args.format, MLUVA_PROMO_THEME=args.theme)
    theme = Path("/usr/share/omarchy/themes") / args.theme / "colors.toml"
    private_theme = Path(os.environ["XDG_STATE_HOME"]) / "omarchy/current/theme"
    private_theme.mkdir(parents=True)
    shutil.copy2(theme, private_theme / "colors.toml")
    font_config = Path(os.environ["XDG_CONFIG_HOME"]) / "fontconfig"
    font_config.mkdir()
    font_config.joinpath("fonts.conf").write_text(
        '<?xml version="1.0"?><!DOCTYPE fontconfig SYSTEM "fonts.dtd"><fontconfig>'
        "<alias><family>monospace</family><prefer><family>Inter</family></prefer></alias></fontconfig>"
    )
    for module in ("Commons", "Ui"):
        shutil.copytree(Path("/usr/share/omarchy/shell") / module, output / module)
        for path in (output / module).glob("*.qml"):
            path.write_text(
                path.read_text().replace('Quickshell.env("HOME")', 'Quickshell.env("OFFSCREEN_SESSION_ROOT")')
            )
    shutil.copytree(root / "linux/quickshell/mluva.dictation", output / "mluva.dictation")
    shutil.copy2(root / "dev/promotion-stage.qml", output / "shell.qml")
    shutil.copy2(root / "linux/resources/com.mluva.Linux.svg", output / "mark.svg")
    binaries = output / "bin"
    binaries.mkdir()
    stub = binaries / "hyprctl"
    stub.write_text("#!/bin/sh\nprintf '{\"int\":0}\\n'\n")
    stub.chmod(0o700)
    environment = {
        **os.environ,
        "PATH": str(binaries) + os.pathsep + os.environ["PATH"],
        "MLUVA_SHELL_COMMAND": str(root / "linux/mluva-shell"),
    }
    portrait = args.format == "portrait"
    width, height = (1080, 1920) if portrait else (1920, 1080)
    app_width, app_height = (936, 820) if portrait else (1116, 780)
    app_x, app_y = (72, 620) if portrait else (704, 160)
    errors: list[str] = []
    copies: list[str] = []
    shots: set[str] = set()
    receipts: list[dict] = []
    children: list[subprocess.Popen] = []
    logs = []
    application = DemoApplication()
    recorder = None
    shell = None
    started = 0.0
    chapter = ""
    entry = None
    rewrite_requested = False
    copy_requested = False

    def launch(command: list[str], name: str, **kwargs) -> subprocess.Popen:
        """Keep every child PID and log local to this one capture."""
        log = (output / name).open("w")
        logs.append(log)
        child = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT, **kwargs)
        children.append(child)
        return child

    def ipc(*arguments: str) -> str:
        """Address the exact private shell instance, never a desktop-wide IPC target."""
        return subprocess.check_output(
            ["quickshell", "ipc", "--pid", str(shell.pid), "call", "promotion", *arguments],
            text=True,
            timeout=5,
        ).strip()

    def capture(name: str, *, app_only: bool = False) -> None:
        """Record settled native pixels and the matching real widget state."""
        if name in shots:
            return
        shots.add(name)
        window = str(application.window.get_surface().get_xid()) if app_only else "root"
        subprocess.run(["import", "-window", window, str(output / f"{name}.png")], check=True, timeout=10)
        receipts.append({"image": name, "seconds": round(time.monotonic() - started, 2), **json.loads(ipc("state"))})

    def tick() -> bool:
        """Advance only input boundaries; production actions drive review and persistence."""
        nonlocal chapter, entry, rewrite_requested, copy_requested
        try:
            elapsed = time.monotonic() - started
            if elapsed > 40:
                raise TimeoutError("The scripted workflow did not finish")
            if recorder.poll() is not None or shell.poll() is not None:
                raise RuntimeError("A capture process exited early; inspect its retained log.")
            desired = (
                "intro"
                if elapsed < 3
                else "recording"
                if elapsed < 15.5
                else "review"
                if elapsed < 18
                else "rewriting"
                if elapsed < 24
                else "saved"
                if elapsed < 28
                else "end"
            )
            if chapter != desired:
                chapter = desired
                ipc("chapter", chapter)
                if chapter == "end":
                    ipc("dismiss")
            workspace = application.conversation_workspace
            if chapter == "recording":
                words = story["source"].split()
                count = min(len(words), int((elapsed - 3) / 12.0 * len(words)) + 1)
                preview = " ".join(words[:count])
                seconds = int(elapsed - 3)
                workspace.set_live(f"Recording  00:{seconds:02d}", preview)
                application._set_status("Listening. Press F9 when you're done.")
                set_button_content(application.record_button, "media-playback-stop-symbolic", "Stop")
                application.recording_overlay_publisher.publish(
                    RecordingOverlayState(
                        phase="recording",
                        elapsed_seconds=seconds,
                        level=0.35 + 0.25 * math.sin(elapsed * 4),
                        preview=preview,
                    )
                )
                if elapsed > 14 and "recording" not in shots:
                    capture("recording")
                    ipc("overlayImage", str(output / "widget-recording.png"))
            elif elapsed >= 15.5 and entry is None:
                workspace.finish_live()
                entry = application.history_store.add(
                    story["source"], story["source"], "dictation", "eng", None, "copied"
                )
                application.history_store.update_title(entry.identifier, story["title"])
                with sqlite3.connect(application.history_store.path) as connection:
                    connection.execute(
                        "UPDATE transcription_history SET created_at = ? WHERE identifier = ?",
                        ("2026-09-08T09:30:00+00:00", entry.identifier),
                    )
                entry = application.history_store.find(entry.identifier)
                workspace.refresh_history()
                workspace.show_conversation(entry, [])
                application._set_status("Copied—ready to paste.")
                set_button_content(application.record_button, "audio-input-microphone-symbolic", "Dictate")
                application._publish_review(entry.identifier)
            elif chapter == "review" and elapsed > 17:
                capture("review")
            elif chapter == "rewriting":
                if not rewrite_requested:
                    rewrite_requested = True
                    ipc("structure")
                if elapsed > 22:
                    capture("rewriting")
            elif chapter in {"saved", "end"}:
                if application.rewrite_client is not None:
                    return GLib.SOURCE_CONTINUE
                replies = application.conversation_store.replies(entry.identifier)
                assert len(replies) == 1 and replies[0].text == story["rewrite"], "Production rewrite did not commit"
                assert application.history_store.find(entry.identifier).raw_text == story["source"]
                if not copy_requested:
                    copy_requested = True
                    ipc("copy")
                if elapsed > 26 and elapsed < 28 and "conversation" not in shots:
                    capture("conversation")
                    capture("workspace", app_only=True)
                    ipc("overlayImage", str(output / "widget-review.png"))
                if chapter == "end":
                    if elapsed > 30:
                        capture("hero")
                if elapsed > 32:
                    assert copies == [story["rewrite"]], "Copy did not traverse the real review action"
                    (output / "capture.json").write_text(
                        json.dumps(
                            {
                                "format": args.format,
                                "theme": args.theme,
                                "viewport": [width, height],
                                "app_viewport": [app_width, app_height],
                                "display": os.environ["DISPLAY"],
                                "source_commit": os.environ.get("MLUVA_SOURCE_COMMIT", "unspecified"),
                                "disclosure": story["disclosure"],
                                "real_rewrite_action": True,
                                "original_preserved": True,
                                "copy_action": True,
                                "screenshots": receipts,
                            },
                            indent=2,
                        )
                        + "\n"
                    )
                    application.quit()
                    return GLib.SOURCE_REMOVE
        except Exception:
            errors.append(traceback.format_exc())
            application.quit()
            return GLib.SOURCE_REMOVE
        return GLib.SOURCE_CONTINUE

    def prepare() -> bool:
        """Position real windows after mapping, then start the recorder at a stable frame."""
        try:
            window = application.window
            window.set_default_size(app_width, app_height)
            window.set_size_request(app_width, app_height)
            for index, title in enumerate(story["history"]):
                item = application.history_store.add(title, title, "dictation", "eng", None, "copied")
                application.history_store.update_title(item.identifier, title)
                stamp = datetime(2026, 9, 8, 9, 30, tzinfo=UTC) - timedelta(hours=index + 1)
                with sqlite3.connect(application.history_store.path) as connection:
                    connection.execute(
                        "UPDATE transcription_history SET created_at = ? WHERE identifier = ?",
                        (stamp.isoformat(), item.identifier),
                    )
            application.conversation_workspace.refresh_history()
            frames = 0

            def settled(_widget, _clock) -> bool:
                """Require painted GTK allocation and a live independent QML shell."""
                nonlocal frames, recorder, started
                frames += 1
                if frames < 8:
                    return GLib.SOURCE_CONTINUE
                try:
                    assert (window.get_surface().get_width(), window.get_surface().get_height()) == (
                        app_width,
                        app_height,
                    )
                    xid = str(window.get_surface().get_xid())
                    subprocess.run(["xdotool", "windowmove", xid, str(app_x), str(app_y)], check=True)
                    subprocess.run(["xdotool", "windowraise", xid], check=True)
                    subprocess.run(["xdotool", "mousemove", "0", "0"], check=True)
                    state = json.loads(ipc("state"))
                    assert (state["width"], state["height"]) == (width, height), state
                    ipc("theme", theme.read_text())
                    recorder = launch(
                        [
                            "ffmpeg",
                            "-hide_banner",
                            "-y",
                            "-f",
                            "x11grab",
                            "-draw_mouse",
                            "0",
                            "-framerate",
                            "30",
                            "-video_size",
                            f"{width}x{height}",
                            "-i",
                            os.environ["DISPLAY"],
                            "-c:v",
                            "libvpx-vp9",
                            "-deadline",
                            "realtime",
                            "-cpu-used",
                            "8",
                            "-b:v",
                            "4M",
                            "-pix_fmt",
                            "yuv420p",
                            "-an",
                            str(output / "capture.webm"),
                        ],
                        "ffmpeg.log",
                        stdin=subprocess.PIPE,
                    )
                    started = time.monotonic()
                    GLib.timeout_add(40, tick)
                except Exception:
                    errors.append(traceback.format_exc())
                    application.quit()
                return GLib.SOURCE_REMOVE

            window.add_tick_callback(settled)
        except Exception:
            errors.append(traceback.format_exc())
            application.quit()
        return GLib.SOURCE_REMOVE

    try:
        launch(["picom", "--backend", "xrender", "--config", "/dev/null"], "compositor.log")
        shell = launch(["quickshell", "--no-color", "-p", str(output / "shell.qml")], "quickshell.log", env=environment)
        with (
            patch("mluva_linux.app.FocusedTextTargetTracker", return_value=None),
            patch.object(PipeWireDeviceCatalog, "from_system", return_value=PipeWireDeviceCatalog()),
            patch(
                "mluva_linux.app.CodexAppServerClient", side_effect=lambda: DemoRewriteClient(story["rewrite"])
            ),
            patch("mluva_linux.app.deliver_text", side_effect=lambda text, **_kwargs: copies.append(text)),
        ):
            GLib.timeout_add(800, prepare)
            application.run(None)
    finally:
        if recorder is not None and recorder.poll() is None:
            recorder.communicate(b"q", timeout=15)
        for child in reversed(children):
            if child.poll() is None:
                child.terminate()
                child.wait(timeout=10)
        for log in logs:
            log.close()
    if errors:
        raise RuntimeError("\n".join(errors))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
