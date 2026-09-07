"""Verify an open GTK app follows a replaced Omarchy theme symlink on its next timer tick."""

import os
import subprocess
import time
from pathlib import Path

import gi

from voice_scribe_linux.theme import ThemeController

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
gi.require_version("GdkX11", "4.0")
from gi.repository import Adw, Gdk, GLib, Gtk  # noqa: E402


def main() -> None:
    """Render both schemes and recover from an invalid theme without desktop access."""
    if "OFFSCREEN_SESSION_ROOT" not in os.environ:
        raise RuntimeError("Use the isolated X11 runner.")
    Adw.init()
    output = Path(os.environ["OFFSCREEN_ARTIFACT_DIR"])
    root = Path(os.environ["XDG_STATE_HOME"]) / "omarchy/current"
    root.mkdir(parents=True)
    link = root / "theme"
    controller = ThemeController(link)
    controller.apply()
    window = Adw.Window(default_width=520, default_height=160)
    box = Gtk.Box(
        orientation=Gtk.Orientation.VERTICAL,
        spacing=12,
        margin_top=24,
        margin_start=24,
        margin_end=24,
        margin_bottom=24,
    )
    box.append(Gtk.Label(label="Mluva follows your Omarchy theme", css_classes=["title-2"]))
    box.append(Gtk.Entry(text="Žluťoučký kůň · a readable note"))
    box.append(Gtk.Button(label="Polish", css_classes=["suggested-action"]))
    window.set_content(box)
    window.present()
    for name, dark, background in (("tokyo-night", True, "#1a1b26"), ("rose-pine", False, "#faf4ed")):
        target = root / name
        target.mkdir()
        target.joinpath("colors.toml").write_text(Path(f"/usr/share/omarchy/themes/{name}/colors.toml").read_text())
        if link.is_symlink():
            link.unlink()
        link.symlink_to(target)
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline:
            while GLib.MainContext.default().pending():
                GLib.MainContext.default().iteration(False)
            time.sleep(0.02)
        assert Adw.StyleManager.get_default().get_dark() == dark
        found, actual = window.get_style_context().lookup_color("vs_canvas")
        expected = Gdk.RGBA()
        expected.parse(background)
        assert found and actual.equal(expected)
        subprocess.run(
            ["import", "-window", str(window.get_surface().get_xid()), str(output / f"{name}.png")], check=True
        )
    link.joinpath("colors.toml").write_text("invalid = [")
    controller._check_theme()
    assert controller._previous_scheme is None
    controller.close()
    assert controller._timer == 0 and controller._scheme_handler == 0
    window.close()


if __name__ == "__main__":
    main()
