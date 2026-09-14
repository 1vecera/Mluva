"""A quiet, fixed-size recording light shared by native capture controls."""

import math

import cairo
import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Gdk", "4.0")
from gi.repository import Gdk, Gtk  # noqa: E402


class RecordingLight(Gtk.DrawingArea):
    """Breathe as one soft fluid silhouette without changing the fixed layout slot."""

    def __init__(self) -> None:
        """Allocate one fixed slot and animate only while mapped and recording."""
        super().__init__(content_width=20, content_height=20, valign=Gtk.Align.CENTER)
        self.add_css_class("ml-recording-light")
        self.set_accessible_role(Gtk.AccessibleRole.STATUS)
        self._recording = False
        self._breath = 0.0
        self._phase = 0.0
        self._started_at = 0
        self._tick_id = 0
        self._settings: Gtk.Settings | None = None
        self._settings_handler = 0
        self.set_draw_func(self._draw)
        self.connect("map", self._mapped)
        self.connect("unmap", self._unmapped)
        self.set_recording(False)

    def set_recording(self, recording: bool) -> None:
        """Expose the recording state to accessibility and stop motion at rest."""
        self._recording = recording
        label = "Recording" if recording else "Dictation status"
        self.set_tooltip_text(label)
        self.update_property([Gtk.AccessibleProperty.LABEL], [label])
        self._sync_motion()

    def _mapped(self, _widget: Gtk.Widget) -> None:
        self._settings = self.get_settings()
        self._settings_handler = self._settings.connect("notify::gtk-enable-animations", self._sync_motion)
        self._sync_motion()

    def _unmapped(self, _widget: Gtk.Widget) -> None:
        if self._settings_handler:
            self._settings.disconnect(self._settings_handler)
            self._settings_handler = 0
        self._settings = None
        self._stop_motion()

    def _sync_motion(self, *_args: object) -> None:
        moving = self._recording and self.get_mapped() and self.get_settings().get_property("gtk-enable-animations")
        if moving and not self._tick_id:
            self._started_at = 0
            self._tick_id = self.add_tick_callback(self._tick)
        elif not moving:
            self._stop_motion()
        self.queue_draw()

    def _stop_motion(self) -> None:
        if self._tick_id:
            self.remove_tick_callback(self._tick_id)
            self._tick_id = 0
        self._breath = 0.0
        self._phase = 0.0
        self.queue_draw()

    def _tick(self, _widget: Gtk.Widget, clock: Gdk.FrameClock) -> bool:
        if not self._started_at:
            self._started_at = clock.get_frame_time()
        phase = (clock.get_frame_time() - self._started_at) / 3_400_000 * math.tau
        self._phase = phase
        self._breath = math.sin(phase)
        self.queue_draw()
        return True

    def _draw(self, _area: Gtk.DrawingArea, context: cairo.Context, width: int, height: int) -> None:
        color = self.get_color()
        radius = 6.4 + 1.6 * self._breath if self._recording else 3
        points = []
        for index in range(65):
            angle = index / 64 * math.tau
            wobble = (
                0.07 * math.sin(3 * angle + self._phase) + 0.035 * math.sin(5 * angle - 2 * self._phase)
                if self._recording
                else 0
            )
            points.append(((1 + wobble) * math.cos(angle), (1 + wobble) * math.sin(angle)))
        min_x, max_x = min(point[0] for point in points), max(point[0] for point in points)
        min_y, max_y = min(point[1] for point in points), max(point[1] for point in points)
        for index, point in enumerate(points):
            x = width / 2 + radius * (2 * (point[0] - min_x) / (max_x - min_x) - 1)
            y = height / 2 + radius * (2 * (point[1] - min_y) / (max_y - min_y) - 1)
            if index:
                context.line_to(x, y)
            else:
                context.move_to(x, y)
        context.close_path()
        gradient = cairo.RadialGradient(width * 0.42, height * 0.4, 0, width / 2, height / 2, radius * 1.1)
        gradient.add_color_stop_rgba(0, color.red, color.green, color.blue, 0.84)
        gradient.add_color_stop_rgba(0.64, color.red, color.green, color.blue, 0.66)
        gradient.add_color_stop_rgba(1, color.red, color.green, color.blue, 0.12)
        context.set_source(gradient)
        context.fill()
