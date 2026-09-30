"""Compact, editable screenshot context beside a narration conversation."""

from collections.abc import Callable

import gi

from mluva_linux.screenshots import Screenshot

gi.require_version("Gtk", "4.0")
from gi.repository import Gtk  # noqa: E402


class ScreenshotShelf(Gtk.ScrolledWindow):
    """Show the exact attached images and offer ordinary edit and removal actions."""

    def __init__(self, edit: Callable[[str], None], remove: Callable[[str], None]) -> None:
        """Keep the image strip hidden until this conversation actually has screenshots."""
        super().__init__(hscrollbar_policy=Gtk.PolicyType.AUTOMATIC, vscrollbar_policy=Gtk.PolicyType.NEVER)
        self.edit = edit
        self.remove = remove
        self.images = Gtk.Box(spacing=12, halign=Gtk.Align.START, valign=Gtk.Align.START)
        self.set_child(self.images)
        self.set_visible(False)
        self.set_margin_start(16)
        self.set_margin_end(16)
        self.set_margin_bottom(8)

    def show_images(self, screenshots: list[Screenshot]) -> None:
        """Rebuild previews after capture, selection, removal or a completed editor save."""
        while child := self.images.get_first_child():
            self.images.remove(child)
        for index, screenshot in enumerate(screenshots, start=1):
            card = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
            preview = Gtk.Picture.new_for_filename(str(screenshot.path))
            preview.set_content_fit(Gtk.ContentFit.CONTAIN)
            preview.set_size_request(180, 100)
            image_button = Gtk.Button(child=preview, tooltip_text="Edit screenshot")
            image_button.connect("clicked", lambda _button, identifier=screenshot.identifier: self.edit(identifier))
            card.append(image_button)
            caption = f"Screenshot {index}"
            if screenshot.captured_after_seconds is not None:
                seconds = round(screenshot.captured_after_seconds)
                caption += f" · {seconds // 60}:{seconds % 60:02}"
            row = Gtk.Box(spacing=4)
            row.append(Gtk.Label(label=caption, xalign=0, hexpand=True, css_classes=["caption"]))
            remove = Gtk.Button(icon_name="edit-delete-symbolic", tooltip_text="Remove screenshot", has_frame=False)
            remove.connect("clicked", lambda _button, identifier=screenshot.identifier: self.remove(identifier))
            row.append(remove)
            card.append(row)
            self.images.append(card)
        self.set_visible(bool(screenshots))
