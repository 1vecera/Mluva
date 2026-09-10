"""Render native recording controls and verify their motion stays inside fixed allocations."""

import json
import os
import subprocess
import time
from pathlib import Path

import gi

from mluva_linux.recording_control import set_recording_button_content
from mluva_linux.theme import DarkTokens, LightTokens, build_stylesheet
from mluva_linux.ui import RecordingBarState, RecordingStatusBar

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
gi.require_version("GdkX11", "4.0")
from gi.repository import Adw, Gdk, GLib, Gtk  # noqa: E402


def main() -> None:
    """Check real GTK animation, reduced motion and teardown on a private display."""
    if "OFFSCREEN_SESSION_ROOT" not in os.environ:
        raise RuntimeError("Use the isolated X11 runner.")
    Adw.init()
    output = Path(os.environ["OFFSCREEN_ARTIFACT_DIR"])
    provider = Gtk.CssProvider()
    Gtk.StyleContext.add_provider_for_display(
        Gdk.Display.get_default(), provider, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION
    )
    settings = Gtk.Settings.get_default()
    settings.set_property("gtk-enable-animations", True)
    window = Adw.Window(default_width=560, default_height=240)
    content = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=16)
    for side in ("top", "bottom", "start", "end"):
        getattr(content, f"set_margin_{side}")(24)
    content.append(Gtk.Label(label="Mluva", xalign=0, css_classes=["ml-wordmark"]))
    strip = RecordingStatusBar()
    content.append(strip)
    button = Gtk.Button(halign=Gtk.Align.END, css_classes=["ml-record-toggle", "destructive-action"])
    light = set_recording_button_content(button)
    content.append(button)
    window.set_content(content)
    state = RecordingBarState(
        kind="recording",
        detail="Recording · Live rewrite",
        elapsed="01:13",
        mode="Dictation",
        delivery="Clipboard",
        level=0.4,
        preview="A quiet light keeps the words in focus.",
        quiet=False,
    )
    strip.present(state)
    window.present()

    def pump(duration: float) -> None:
        deadline = time.monotonic() + duration
        while time.monotonic() < deadline:
            while GLib.MainContext.default().pending():
                GLib.MainContext.default().iteration(False)
            time.sleep(0.005)

    def capture(name: str) -> None:
        subprocess.run(
            ["import", "-window", str(window.get_surface().get_xid()), str(output / f"{name}.png")], check=True
        )

    provider.load_from_string(build_stylesheet(DarkTokens))
    pump(0.25)
    assert strip.phase_chip.get_accessible_role() == Gtk.AccessibleRole.STATUS
    assert strip.phase_chip.get_tooltip_text() == "Recording"
    assert strip.phase_label.get_label() == "Live rewrite"
    assert strip.time_label.get_label() == "01:13"
    assert strip.preview_label.get_label() == state.preview
    allocations = []
    breaths = []
    captured = set()
    deadline = time.monotonic() + 3
    while time.monotonic() < deadline:
        pump(0.02)
        allocations.append((light.get_width(), light.get_height(), button.get_width(), strip.get_height()))
        breaths.append(light._breath)
        for name, reached in (("expanded", light._breath > 0.98), ("contracted", light._breath < -0.98)):
            if reached and name not in captured:
                capture(f"dark-{name}")
                captured.add(name)
    assert captured == {"expanded", "contracted"}
    assert len(set(allocations)) == 1, allocations
    assert min(breaths) < -0.98 and max(breaths) > 0.98
    settings.set_property("gtk-enable-animations", False)
    pump(0.1)
    assert light._tick_id == strip.phase_chip._tick_id == 0
    assert light._breath == strip.phase_chip._breath == 0
    capture("reduced-motion")
    provider.load_from_string(build_stylesheet(LightTokens))
    pump(0.15)
    capture("light-recording")
    settings.set_property("gtk-enable-animations", True)
    pump(0.1)
    assert light._tick_id and strip.phase_chip._tick_id
    window.set_visible(False)
    pump(0.05)
    assert light._tick_id == strip.phase_chip._tick_id == 0
    assert light._settings_handler == strip.phase_chip._settings_handler == 0
    window.present()
    pump(0.1)
    assert light._tick_id and strip.phase_chip._tick_id
    strip.clear()
    pump(0.05)
    assert strip.phase_chip._tick_id == 0 and strip.time_label.get_label() == "00:00"
    window.close()
    pump(0.05)
    assert light._tick_id == light._settings_handler == 0
    (output / "receipt.json").write_text(
        json.dumps(
            {"allocation": allocations[0], "breaths": breaths, "reduced_motion": True, "unmap_stops": True}, indent=2
        )
    )


if __name__ == "__main__":
    main()
