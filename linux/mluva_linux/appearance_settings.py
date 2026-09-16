"""Recording appearance controls with a private, embedded ten-line desktop preview."""

from collections.abc import Callable

import gi

from mluva_linux.config import WIDGET_POSITIONS, AppConfig

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, Gtk  # noqa: E402

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
        self.position = Adw.ComboRow(
            title="Recorder position", model=Gtk.StringList.new([v for _, v in WIDGET_POSITIONS])
        )
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
        for row in (self.position, self.lines, self.opacity, self.paste):
            self.add(row)
        self.preview_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        self.preview = Gtk.DrawingArea(content_height=210, hexpand=True)
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
        """Composite the background only, leaving transcript text fully opaque."""
        cr.set_source_rgb(0.21, 0.32, 0.42)
        cr.paint()
        cr.set_source_rgb(0.34, 0.47, 0.45)
        cr.arc(width * 0.7, height * 0.45, width * 0.45, 0, 6.284)
        cr.fill()
        cr.set_source_rgba(1, 1, 1, 0.15)
        for i in range(7):
            cr.rectangle(20, 35 + i * 33, width * 0.6, 12)
        cr.fill()
        values = self.values()
        panel_width = min(390, width - 32)
        panel_height = 45 + values["widget_lines"] * 22
        position = values["widget_position"]
        x = (
            16
            if position == "bottom-left"
            else width - panel_width - 16
            if position == "bottom-right"
            else (width - panel_width) / 2
        )
        scale = min(1.0, (height - 32) / panel_height)
        cr.translate(x, height - panel_height * scale - 16)
        cr.scale(scale, scale)
        x, y = 0, 0
        cr.set_source_rgba(0.05, 0.06, 0.08, values["widget_opacity"] / 100)
        cr.rectangle(x, y, panel_width, panel_height)
        cr.fill()
        cr.save()
        cr.rectangle(x + 12, y + 10, panel_width - 24, panel_height - 20)
        cr.clip()
        cr.select_font_face("monospace")
        cr.set_font_size(13)
        cr.set_source_rgb(1, 0.45, 0.5)
        cr.move_to(x + 12, y + 24)
        cr.show_text("● Recording                         00:12")
        cr.set_source_rgb(0.97, 0.97, 0.98)
        for i, line in enumerate(SAMPLE_LINES[-values["widget_lines"] :]):
            cr.move_to(x + 12, y + 46 + i * 22)
            cr.show_text(line)
        cr.restore()
