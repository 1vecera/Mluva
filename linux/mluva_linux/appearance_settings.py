"""Recording appearance controls with a private, embedded ten-line desktop preview."""

import math
import os
from collections.abc import Callable
from pathlib import Path

import cairo
import gi

from mluva_linux.config import WIDGET_POSITIONS, AppConfig
from mluva_linux.direct_choices import DirectChoices
from mluva_linux.theme import DarkTokens, LightTokens, read_omarchy_palette

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, Gtk, Pango, PangoCairo  # noqa: E402

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
        super().__init__(title="Make it yours", description="Preview with ten sample lines. No desktop capture.")
        self.changed = changed
        position_row = Adw.ActionRow(title="Recorder position")
        self.position = DirectChoices(["Left", "Center", "Right"])
        position_row.add_suffix(self.position)
        self.position.set_selected([k for k, _ in WIDGET_POSITIONS].index(config.widget_position))
        self.lines = Adw.SpinRow(
            title="Visible transcript lines · 5 recommended",
            adjustment=Gtk.Adjustment(value=config.widget_lines, lower=1, upper=10, step_increment=1),
        )
        self.opacity = Adw.SpinRow(
            title="Background opacity (%)",
            subtitle="Lower: see more desktop · Higher: read more easily",
            adjustment=Gtk.Adjustment(
                value=config.widget_opacity, lower=10, upper=100, step_increment=1, page_increment=10
            ),
        )
        self.paste = Adw.SwitchRow(
            title="Paste automatically",
            subtitle="Off recommended. Copy and paste when you are ready.",
            active=config.auto_paste,
        )
        for row in (position_row, self.lines, self.opacity, self.paste):
            self.add(row)
        self.preview_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        self.preview = Gtk.DrawingArea(content_height=190, hexpand=True)
        self.preview.set_draw_func(self._draw)
        self.preview.update_property(
            [Gtk.AccessibleProperty.LABEL],
            ["Synthetic desktop with a recording widget and ten sample transcript lines"],
        )
        self.preview_box.append(self.preview)
        self.caption = Gtk.Label(wrap=True, xalign=0, margin_top=8, margin_bottom=8)
        self.preview_box.append(self.caption)
        self.add(self.preview_box)
        for row, prop in (
            (self.position, "selected"),
            (self.lines, "value"),
            (self.opacity, "value"),
            (self.paste, "active"),
        ):
            row.connect("notify::" + prop, self._changed)
        self._changed()

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
        color(tokens["ink"], 0.12)
        for i in range(12):
            cr.rectangle(16, 18 + i * 22, width * (0.72 if i % 3 else 0.5), 2)
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
        gradient = cairo.RadialGradient(6.4, 8, 0, 8, 10, 8.8)
        for stop, alpha in ((0, 0.84), (0.64, 0.66), (1, 0.12)):
            gradient.add_color_stop_rgba(stop, *ink, alpha)
        cr.set_source(gradient)
        cr.arc(8, 10, 8, 0, math.tau)
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
