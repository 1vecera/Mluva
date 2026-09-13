"""Full-window settings and provider onboarding built from the real preference pages."""

from collections.abc import Callable

import gi

from mluva_linux.config import AppConfig
from mluva_linux.provider_settings import ProviderSettings
from mluva_linux.ui import SPACE_2, SPACE_4, brand_mark, set_margins

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, Gtk  # noqa: E402


class SettingsView(Gtk.Box):
    """Use the application viewport without a nested window or modal boundary."""

    def __init__(self, go_back: Callable[[], None]) -> None:
        """Keep all preference pages reachable at narrow and wide window sizes."""
        super().__init__(orientation=Gtk.Orientation.VERTICAL, vexpand=True)
        self.go_back = go_back
        header = Gtk.Box(spacing=SPACE_2)
        set_margins(header, SPACE_4)
        back = Gtk.Button(icon_name="go-previous-symbolic", tooltip_text="Back to workspace · Esc", has_frame=False)
        back.connect("clicked", lambda _button: self.close())
        header.append(back)
        self.selector = Gtk.DropDown(hexpand=True)
        self.names = Gtk.StringList()
        self.selector.set_model(self.names)
        self.selector.update_property([Gtk.AccessibleProperty.LABEL], ["Settings page"])
        self.selector.connect("notify::selected", self._selected)
        header.append(self.selector)
        self.append(header)
        self.stack = Gtk.Stack(vexpand=True, hexpand=True)
        self.pages: list[Adw.PreferencesPage] = []
        self.append(self.stack)

    def add(self, page: Adw.PreferencesPage) -> None:
        """Index the actual page, retaining row deep links from command search."""
        self.pages.append(page)
        self.stack.add_named(page, page.get_name())
        self.names.append(page.get_title())

    def _selected(self, *_args: object) -> None:
        index = self.selector.get_selected()
        if index < len(self.pages):
            self.stack.set_visible_child(self.pages[index])

    def set_visible_page(self, page: Adw.PreferencesPage) -> None:
        """Select a page without introducing a second copy of its settings state."""
        self.selector.set_selected(self.pages.index(page))
        self.stack.set_visible_child(page)

    def set_visible_page_name(self, name: str) -> None:
        """Open a stable destination used by commands and external app actions."""
        self.set_visible_page(next(page for page in self.pages if page.get_name() == name))

    def get_visible_page(self) -> Adw.PreferencesPage:
        """Return the real page for deep-link and focus verification."""
        return self.stack.get_visible_child()

    def get_visible_page_name(self) -> str:
        """Return the stable route for the currently visible settings page."""
        return self.stack.get_visible_child_name()

    def close(self) -> None:
        """Return to the workspace without closing or resizing the app."""
        self.go_back()


class WelcomeView(Gtk.Box):
    """Explain speech and rewriting, then configure their real provider/model controls."""

    def __init__(self, config: AppConfig, save: Callable[[dict], bool], finish: Callable[[], None]) -> None:
        """Reuse provider validation and saving instead of maintaining a separate setup form."""
        super().__init__(orientation=Gtk.Orientation.VERTICAL, vexpand=True)
        heading = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=SPACE_2)
        mark = brand_mark(40)
        mark.set_halign(Gtk.Align.START)
        heading.append(mark)
        heading.append(Gtk.Label(label="Welcome to Mluva", xalign=0, css_classes=["title-1"]))
        heading.append(Gtk.Label(label="Best of local and cloud.", xalign=0, css_classes=["heading"]))
        heading.append(
            Gtk.Label(
                label="Choose a speech provider for dictation and a rewriting provider for polishing and Live drafts. "
                "You can change them in Settings at any time.",
                xalign=0,
                wrap=True,
            )
        )
        self.providers = ProviderSettings(config, save, introduction=heading)
        self.providers.set_vexpand(True)
        self.append(self.providers)
        footer = Gtk.Box(spacing=SPACE_2)
        set_margins(footer, SPACE_4)
        footer.append(Gtk.Label(label="F9 to talk · Ctrl+P for commands", xalign=0, hexpand=True, wrap=True))
        start = Gtk.Button(label="Open workspace")
        start.connect("clicked", lambda _button: finish())
        footer.append(start)
        self.append(Adw.Clamp(maximum_size=600, tightening_threshold=600, child=footer))
