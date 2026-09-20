"""Always-visible choices for short lists, with keyboard and accessibility support."""

import gi

gi.require_version("Gtk", "4.0")
from gi.repository import GObject, Gtk  # noqa: E402


class DirectChoices(Gtk.Box):
    """Expose a selected index like a combo, without hiding options in a menu."""

    selected = GObject.Property(type=int, default=0)

    def __init__(self, labels):
        """Keep labels visible and let grouped toggle buttons handle arrow keys."""
        super().__init__(spacing=0, homogeneous=True, css_classes=["linked"], margin_top=8, margin_bottom=8)
        self.buttons = []
        for index, label in enumerate(labels):
            button = Gtk.ToggleButton(label=label, hexpand=True)
            if self.buttons:
                button.set_group(self.buttons[0])
            button.connect("toggled", self._toggled, index)
            self.buttons.append(button)
            self.append(button)
        self.set_selected(0)

    def _toggled(self, button, index):
        if button.get_active():
            self.selected = index

    def set_selected(self, index):
        """Select a stable option and notify listeners only on changes."""
        self.buttons[index].set_active(True)

    def get_selected(self):
        """Return the currently active option."""
        return self.selected
