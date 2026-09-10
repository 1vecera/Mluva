"""A quiet, fixed-size recording light shared by native capture controls."""

import math

import cairo
import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Gdk", "4.0")
from gi.repository import Gdk, Gtk  # noqa: E402


class RecordingLight(Gtk.DrawingArea):
    """Breathe around a six-pixel center without requesting a new layout."""

    def __init__(self) -> None:
        """Allocate one fixed slot and animate only while mapped and recording."""
        super().__init__(content_width=20, content_height=20, valign=Gtk.Align.CENTER)
        self.add_css_class("ml-recording-light")
        self.set_accessible_role(Gtk.AccessibleRole.STATUS)
        self._recording = False
        self._breath = 0.0
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
        self.queue_draw()

    def _tick(self, _widget: Gtk.Widget, clock: Gdk.FrameClock) -> bool:
        if not self._started_at:
            self._started_at = clock.get_frame_time()
        self._breath = math.sin((clock.get_frame_time() - self._started_at) / 2_600_000 * math.tau)
        self.queue_draw()
        return True

    def _draw(self, _area: Gtk.DrawingArea, context: cairo.Context, width: int, height: int) -> None:
        color = self.get_color()
        if self._recording:
            context.set_source_rgba(color.red, color.green, color.blue, 0.12 + 0.04 * self._breath)
            context.arc(width / 2, height / 2, 7 * (1 + 0.12 * self._breath), 0, math.tau)
            context.fill()
        context.set_source_rgba(color.red, color.green, color.blue, 0.92 + 0.08 * self._breath)
        context.arc(width / 2, height / 2, 3 * (1 + 0.18 * self._breath), 0, math.tau)
        context.fill()


def set_recording_button_content(button: Gtk.Button) -> RecordingLight:
    """Keep the stop action explicit beside the calm live recording light."""
    content = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=6, halign=Gtk.Align.CENTER)
    light = RecordingLight()
    light.set_recording(True)
    content.append(light)
    content.append(Gtk.Label(label="Stop"))
    button.set_child(content)
    return light
