"""A local animated example of optional polishing; never invokes a text provider."""

import gi

gi.require_version("Gtk", "4.0")
from gi.repository import GLib, Gtk  # noqa: E402


class PolishPreview(Gtk.Box):
    """Illustrate speaking, polishing and the finished sentence without sending content."""

    def __init__(self):
        """Run the example only while visible and respect the desktop animation setting."""
        super().__init__(
            orientation=Gtk.Orientation.VERTICAL, spacing=14, margin_top=24, margin_bottom=16, css_classes=["card"]
        )
        self.timer = 0
        self.tick = 0
        self.caption = Gtk.Label(
            label="Polishing preview", xalign=0, margin_start=18, margin_top=16, css_classes=["heading"]
        )
        self.append(self.caption)
        self.text = Gtk.Label(xalign=0, wrap=True, margin_start=18, margin_end=18, height_request=72)
        self.append(self.text)
        self.progress = Gtk.ProgressBar(margin_start=18, margin_end=18, margin_bottom=18)
        self.append(self.progress)
        self.connect("map", self._start)
        self.connect("unmap", self._stop)

    def _start(self, *_args):
        self.tick = 0
        if Gtk.Settings.get_default().get_property("gtk-enable-animations"):
            self.timer = GLib.timeout_add(90, self._animate)
        else:
            self.text.set_label("Let’s meet Friday at 10 to review the launch.")
            self.caption.set_label("Polishing preview · ready")
            self.progress.set_fraction(1)

    def _stop(self, *_args):
        if self.timer:
            GLib.source_remove(self.timer)
            self.timer = 0

    def _animate(self):
        raw = "so um let’s meet friday at ten to review the launch"
        polished = "Let’s meet Friday at 10 to review the launch."
        frame = self.tick % 120
        if frame < 52:
            self.caption.set_label("Polishing preview · speaking")
            self.text.set_label(raw[: frame + 1] + "▌")
            self.progress.set_fraction(frame / 120)
        elif frame < 70:
            self.caption.set_label("Polishing preview · refining" + "." * (frame // 4 % 3 + 1))
            self.text.set_label(raw)
            self.progress.set_fraction(frame / 120)
        else:
            self.caption.set_label("Polishing preview · ready")
            self.text.set_label(polished)
            self.progress.set_fraction(1)
        self.tick += 1
        return GLib.SOURCE_CONTINUE
