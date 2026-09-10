"""A small keyboard-first panel for the application's existing commands."""

from collections.abc import Callable
from dataclasses import dataclass

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, Gdk, Gtk  # noqa: E402


@dataclass(frozen=True)
class Command:
    """Keep availability live so an open panel cannot dispatch an obsolete action."""

    title: str
    icon: str
    run: Callable[[], object]
    enabled: Callable[[], bool]
    keywords: str = ""


class CommandPalette(Adw.Dialog):
    """Search and invoke ordinary app controls without claiming a desktop shortcut."""

    def __init__(self, commands: tuple[Command, ...]) -> None:
        """Build a native searchable list with explicit keyboard and dismissal behavior."""
        super().__init__(title="Commands", content_width=460, content_height=500)
        self.commands = commands
        self.rows: dict[Gtk.ListBoxRow, Command] = {}
        self.pending: Command | None = None
        toolbar = Adw.ToolbarView()
        toolbar.add_top_bar(Adw.HeaderBar())
        content = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
        for setter in (
            content.set_margin_top,
            content.set_margin_bottom,
            content.set_margin_start,
            content.set_margin_end,
        ):
            setter(12)
        self.search = Gtk.SearchEntry(placeholder_text="Search actions…", hexpand=True)
        self.search.update_property([Gtk.AccessibleProperty.LABEL], ["Search actions"])
        self.search.set_search_delay(0)
        self.search.connect("search-changed", self._filter)
        content.append(self.search)
        self.results = Gtk.ListBox(selection_mode=Gtk.SelectionMode.SINGLE)
        self.results.add_css_class("boxed-list")
        self.results.connect("row-activated", self._activate)
        self.scroll = Gtk.ScrolledWindow(
            child=self.results,
            hscrollbar_policy=Gtk.PolicyType.NEVER,
            overlay_scrolling=False,
            vexpand=True,
        )
        content.append(self.scroll)
        self.empty = Gtk.Label(label="No matching actions", vexpand=True)
        self.empty.add_css_class("dim-label")
        content.append(self.empty)
        hint = Gtk.Label(label="↑ ↓ to choose · Enter to run · Esc to close")
        hint.add_css_class("caption")
        hint.add_css_class("dim-label")
        content.append(hint)
        toolbar.set_content(content)
        self.set_child(toolbar)
        self.set_focus(self.search)
        keys = Gtk.EventControllerKey(propagation_phase=Gtk.PropagationPhase.CAPTURE)
        keys.connect("key-pressed", self._key_pressed)
        self.add_controller(keys)
        self.connect("closed", self._closed)
        self._filter()

    def _filter(self, _entry: Gtk.SearchEntry | None = None) -> None:
        """Keep unavailable commands discoverable and select the first runnable match."""
        self.results.remove_all()
        self.rows.clear()
        terms = self.search.get_text().casefold().split()
        for command in self.commands:
            searchable = f"{command.title} {command.keywords}".casefold()
            if not all(term in searchable for term in terms):
                continue
            row = Adw.ActionRow(title=command.title, activatable=True)
            row.add_prefix(Gtk.Image.new_from_icon_name(command.icon))
            row.set_sensitive(command.enabled())
            self.rows[row] = command
            self.results.append(row)
        self.empty.set_visible(not self.rows)
        available = self._available_rows()
        if available:
            self.results.select_row(available[0])
        self.scroll.get_vadjustment().set_value(0)

    def _available_rows(self) -> list[Gtk.ListBoxRow]:
        """Recheck application state when navigating an already open panel."""
        available = []
        for row, command in self.rows.items():
            enabled = command.enabled()
            row.set_sensitive(enabled)
            if enabled:
                available.append(row)
        return available

    def _activate(self, _list: Gtk.ListBox, row: Gtk.ListBoxRow | None) -> None:
        """Dismiss before dispatch so dialogs and editor focus transfer correctly."""
        if row is None:
            return
        command = self.rows[row]
        if command.enabled():
            self.pending = command
            self.close()
        else:
            row.set_sensitive(False)

    def _closed(self, _dialog: Adw.Dialog) -> None:
        """Run only an explicit selection that is still available after dismissal."""
        command = self.pending
        self.pending = None
        if command is not None and command.enabled():
            command.run()

    def _key_pressed(self, _controller, key: int, _code: int, state: Gdk.ModifierType) -> bool:
        """Consume panel keys so Escape never also cancels an active recording."""
        if key == Gdk.KEY_Escape or (key in (Gdk.KEY_p, Gdk.KEY_P) and state & Gdk.ModifierType.CONTROL_MASK):
            self.close()
            return True
        focus = self.get_focus()
        if focus is None or not (focus is self.search or focus.is_ancestor(self.search)):
            return False
        if key in (Gdk.KEY_Return, Gdk.KEY_KP_Enter):
            self._activate(self.results, self.results.get_selected_row())
            return True
        if key not in (Gdk.KEY_Up, Gdk.KEY_Down):
            return False
        rows = self._available_rows()
        if rows:
            selected = self.results.get_selected_row()
            index = rows.index(selected) if selected in rows else -1
            step = 1 if key == Gdk.KEY_Down else -1
            row = rows[(index + step) % len(rows)]
            self.results.select_row(row)
            valid, bounds = row.compute_bounds(self.results)
            if valid:
                # Search retains focus, so ListBox's focus scrolling does not run.
                self.scroll.get_vadjustment().clamp_page(bounds.get_y(), bounds.get_y() + bounds.get_height())
        return True
