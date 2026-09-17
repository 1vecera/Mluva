"""Recording appearance controls with a private, embedded ten-line desktop preview."""

import math
import os
import time
from collections.abc import Callable
from pathlib import Path

import cairo
import gi

from mluva_linux.config import WIDGET_POSITIONS, AppConfig
from mluva_linux.direct_choices import DirectChoices
from mluva_linux.theme import DarkTokens, LightTokens, read_omarchy_palette

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, GLib, Gtk, Pango, PangoCairo  # noqa: E402

SAMPLE_LINES = (
    "A small idea becomes a clear thought.",
    "Speak naturally and keep your rhythm.",
    "Your words appear while you talk.",
    "The original transcript stays yours.",
    "Five lines leave room for the desktop.",
    "New words gently move into view.",
    "Choose more space for longer thoughts.",
    "Or keep the recorder small and quiet.",
    "Adjust the background to suit your eyes.",
    "This is the last of ten sample lines.",
)


class AppearanceSettings(Adw.PreferencesGroup):
    """Share the exact same appearance form between onboarding and settings."""

    def __init__(self, config: AppConfig, changed: Callable[[], None] = lambda: None) -> None:
        """Preview synthetic content without capturing or moving the user's desktop."""
        super().__init__(title="Your recorder", description="A little space for your voice.")
        self.changed = changed
        self.timer = 0
        content = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        self.preview_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8, css_classes=["card"])
        preview_header = Gtk.Box(margin_start=14, margin_end=14, margin_top=12)
        preview_header.append(
            Gtk.Label(label="DESKTOP PREVIEW", xalign=0, hexpand=True, css_classes=["caption", "dim-label"])
        )
        preview_header.append(Gtk.Label(label="● Live", css_classes=["caption"]))
        self.preview_box.append(preview_header)
        self.preview = Gtk.DrawingArea(content_height=180, hexpand=True)
        self.preview.set_draw_func(self._draw)
        self.preview.update_property(
            [Gtk.AccessibleProperty.LABEL], ["Recorder appearance preview over a sample desktop"]
        )
        self.preview_box.append(self.preview)
        self.caption = Gtk.Label(
            wrap=True, xalign=0, margin_start=14, margin_end=14, margin_bottom=12, css_classes=["caption", "dim-label"]
        )
        self.preview_box.append(self.caption)
        content.append(self.preview_box)
        position_heading = Gtk.Label(label="Where should it sit?", xalign=0, css_classes=["heading"])
        content.append(position_heading)
        self.position = DirectChoices(["↙  Left", "↓  Center", "↘  Right"])
        self.position.set_selected([k for k, _ in WIDGET_POSITIONS].index(config.widget_position))
        content.append(self.position)
        self.lines = Gtk.Scale.new_with_range(Gtk.Orientation.HORIZONTAL, 1, 10, 1)
        self.lines.set_round_digits(0)
        self.lines.set_draw_value(False)
        self.lines.set_value(config.widget_lines)
        self.lines.update_property([Gtk.AccessibleProperty.LABEL], ["Visible transcript lines"])
        for value in (1, 3, 5, 10):
            self.lines.add_mark(value, Gtk.PositionType.BOTTOM, str(value))
        self.line_label = Gtk.Label(xalign=0, css_classes=["heading"])
        content.append(self.line_label)
        content.append(self.lines)
        self.opacity = Gtk.Scale.new_with_range(Gtk.Orientation.HORIZONTAL, 10, 100, 1)
        self.opacity.set_draw_value(False)
        self.opacity.set_value(config.widget_opacity)
        self.opacity.update_property([Gtk.AccessibleProperty.LABEL], ["Recorder background opacity"])
        self.opacity_label = Gtk.Label(xalign=0, css_classes=["heading"])
        content.append(self.opacity_label)
        content.append(self.opacity)
        ends = Gtk.Box()
        ends.append(Gtk.Label(label="More desktop", xalign=0, hexpand=True, css_classes=["caption", "dim-label"]))
        ends.append(Gtk.Label(label="More contrast", xalign=1, css_classes=["caption", "dim-label"]))
        content.append(ends)
        self.paste = Adw.SwitchRow(
            title="Paste when I stop", subtitle="Off recommended · copy when you are ready", active=config.auto_paste
        )
        switches = Gtk.ListBox(selection_mode=Gtk.SelectionMode.NONE, css_classes=["boxed-list"])
        switches.append(self.paste)
        content.append(switches)
        self.add(content)
        self.position.connect("notify::selected", self._changed)
        self.lines.connect("value-changed", self._changed)
        self.opacity.connect("value-changed", self._changed)
        self.paste.connect("notify::active", self._changed)
        self.preview.connect("map", self._animate)
        self.preview.connect("unmap", self._stop_animation)
        self._changed()

    def _animate(self, *_args):
        if Gtk.Settings.get_default().get_property("gtk-enable-animations") and not self.timer:
            self.timer = GLib.timeout_add(66, self._tick)

    def _tick(self):
        self.preview.queue_draw()
        return GLib.SOURCE_CONTINUE

    def _stop_animation(self, *_args):
        if self.timer:
            GLib.source_remove(self.timer)
            self.timer = 0

    def values(self) -> dict:
        """Return only settings represented by this form."""
        return dict(
            widget_position=WIDGET_POSITIONS[self.position.get_selected()][0],
            widget_lines=int(self.lines.get_value()),
            widget_opacity=int(self.opacity.get_value()),
            auto_paste=self.paste.get_active(),
        )

    def refresh_config(self, config: AppConfig) -> None:
        """Reflect externally saved changes when returning to settings."""
        self.position.set_selected([k for k, _ in WIDGET_POSITIONS].index(config.widget_position))
        self.lines.set_value(config.widget_lines)
        self.opacity.set_value(config.widget_opacity)
        self.paste.set_active(config.auto_paste)

    def _changed(self, *_args) -> None:
        self.preview.queue_draw()
        self.line_label.set_label(f"{int(self.lines.get_value())} visible lines · 5 recommended")
        self.opacity_label.set_label(f"Background · {int(self.opacity.get_value())}%")
        self.caption.set_label(
            f"Showing the last {int(self.lines.get_value())} of 10 lines · "
            f"{int(self.opacity.get_value())}% background opacity"
        )
        self.changed()

    def _draw(self, _area, cr, width: int, height: int) -> None:
        """Mirror the recorder's bare 20px header, 500px surface and 14/22px mono text."""
        state = Path(os.environ.get("XDG_STATE_HOME", Path.home() / ".local/state"))
        palette = read_omarchy_palette(state / "omarchy/current/theme/colors.toml")
        tokens = palette[0] if palette else DarkTokens if Adw.StyleManager.get_default().get_dark() else LightTokens

        def color(value, alpha=1):
            cr.set_source_rgba(*(int(value[i : i + 2], 16) / 255 for i in (1, 3, 5)), alpha)

        def text(value, x, y, size=14):
            layout = PangoCairo.create_layout(cr)
            font = Pango.FontDescription("JetBrains Mono")
            font.set_absolute_size(size * Pango.SCALE)
            layout.set_font_description(font)
            layout.set_text(value, -1)
            cr.move_to(x, y)
            PangoCairo.show_layout(cr, layout)

        # A clearly synthetic workspace makes transparency readable without a screenshot.
        color(tokens["canvas"])
        cr.paint()
        color(tokens["ink"], 0.06)
        cr.rectangle(20, 12, width - 40, height - 28)
        cr.fill()
        color(tokens["action"], 0.09)
        cr.rectangle(20, 12, 76, height - 28)
        cr.fill()
        color(tokens["ink"], 0.35)
        text("Launch notes", 112, 25, 12)
        color(tokens["ink"], 0.10)
        for i in range(7):
            cr.rectangle(112, 56 + i * 22, max(20, (width - 148) * (0.8 if i % 3 else 0.55)), 2)
        cr.fill()
        values = self.values()
        panel_width = 500
        panel_height = 40 + values["widget_lines"] * 22
        scale = min(1.0, (width - 56) / panel_width, (height - 32) / panel_height)
        rendered_width = panel_width * scale
        position = values["widget_position"]
        x = (
            12
            if position == "bottom-left"
            else width - rendered_width - 12
            if position == "bottom-right"
            else (width - rendered_width) / 2
        )
        cr.save()
        cr.translate(x, height - panel_height * scale - 12)
        cr.scale(scale, scale)
        # RecordingLight.qml at a fixed circular breathing endpoint: same bounds and gradient.
        ink = tuple(int(tokens["danger"][i : i + 2], 16) / 255 for i in (1, 3, 5))
        phase = (time.monotonic() % 3.4) / 3.4 * math.tau if self.timer else 0
        radius = 6.4 - 1.6 * math.cos(phase)
        gradient = cairo.RadialGradient(6.4, 8, 0, 8, 10, radius * 1.1)
        for stop, alpha in ((0, 0.84), (0.64, 0.66), (1, 0.12)):
            gradient.add_color_stop_rgba(stop, *ink, alpha)
        cr.set_source(gradient)
        points = []
        for i in range(65):
            angle = i / 64 * math.tau
            wobble = math.sin(phase) ** 2 * (
                0.07 * math.sin(3 * angle + phase) + 0.035 * math.sin(5 * angle - 2 * phase)
            )
            points.append(((1 + wobble) * math.cos(angle), (1 + wobble) * math.sin(angle)))
        left, right = min(p[0] for p in points), max(p[0] for p in points)
        top, bottom = min(p[1] for p in points), max(p[1] for p in points)
        for i, (px, py) in enumerate(points):
            point = (
                8 + radius * (2 * (px - left) / (right - left) - 1),
                10 + radius * (2 * (py - top) / (bottom - top) - 1),
            )
            (cr.move_to if i == 0 else cr.line_to)(*point)
        cr.close_path()
        cr.fill()
        color(tokens["ink"])
        text("00:12", panel_width - 42, 0)
        # The real header has no background and no redundant Recording label.
        cr.rectangle(0, 20, panel_width, panel_height - 20)
        color(tokens["surface"], values["widget_opacity"] / 100)
        cr.fill_preserve()
        color(tokens["outline"])
        cr.set_line_width(1)
        cr.stroke()
        cr.rectangle(10, 30, panel_width - 20, values["widget_lines"] * 22)
        cr.clip()
        color(tokens["ink"])
        for i, line in enumerate(SAMPLE_LINES[-values["widget_lines"] :]):
            text(line, 10, 30 + i * 22)
        cr.restore()
