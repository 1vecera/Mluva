"""A small keyboard-first panel for the application's existing commands."""

from collections.abc import Callable
from dataclasses import dataclass
from typing import TYPE_CHECKING

import gi

from mluva_linux.config import TIME_FORMATS, WIDGET_POSITIONS
from mluva_linux.conversation import QUICK_POLISH
from mluva_linux.live_rewrite import TEMPLATE_CHOICES

if TYPE_CHECKING:
    from mluva_linux.app import MluvaApplication

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, Gdk, GLib, Gtk  # noqa: E402


@dataclass(frozen=True)
class Command:
    """Keep availability live so an open panel cannot dispatch an obsolete action."""

    title: str
    icon: str
    run: Callable[[], object]
    enabled: Callable[[], bool]
    keywords: str = ""


def application_commands(app: "MluvaApplication") -> tuple[Command, ...]:
    """Snapshot action identities at panel open and recheck availability before dispatch."""
    workspace = app.conversation_workspace
    recording = app.recorder is not None and app.recorder.process is not None
    capture_preparing = app.capture_preparing
    live_enabled = app.config.live_rewrite_enabled
    identifier = workspace.entry.identifier if workspace.entry is not None else None
    editor = workspace.result_widgets[-1] if workspace.result_widgets else None

    def current_document() -> bool:
        """Bind document commands to the visible conversation chosen when the panel opened."""
        selected = workspace.entry.identifier if workspace.entry is not None else None
        latest = workspace.result_widgets[-1] if workspace.result_widgets else None
        return (
            editor is not None
            and app.page_stack.get_visible_child_name() == "capture"
            and selected == identifier
            and latest is editor
        )

    record_title = (
        "Cancel preparation" if app.capture_preparing else "Stop dictation" if recording else "Start dictation"
    )
    commands = (
        Command(
            record_title,
            "media-playback-stop-symbolic" if recording else "audio-input-microphone-symbolic",
            lambda: app._toggle_recording(app.record_button),
            lambda: (
                app.record_button.is_sensitive()
                and app.capture_preparing == capture_preparing
                and (app.recorder is not None and app.recorder.process is not None) == recording
            ),
            "record microphone capture F9",
        ),
        Command(
            "Turn off Live rewrite" if app.config.live_rewrite_enabled else "Turn on Live rewrite",
            "document-edit-symbolic",
            lambda: app.live_mode_switch.set_active(not app.live_mode_switch.get_active()),
            lambda: app.live_mode_switch.is_sensitive() and app.config.live_rewrite_enabled == live_enabled,
            "automatic structured draft",
        ),
        Command(
            "Polish text",
            "applications-utilities-symbolic",
            lambda: workspace.request_prompt("rewrite-polish", QUICK_POLISH),
            lambda: (
                current_document() and workspace.quick_polish.is_sensitive() and not workspace.live_box.get_visible()
            ),
            "clean filler grammar rewrite",
        ),
        Command(
            "Rewrite with an instruction",
            "document-edit-symbolic",
            app._focus_rewrite_prompt,
            lambda: current_document() and workspace.send.is_sensitive() and not workspace.live_box.get_visible(),
            "custom prompt edit",
        ),
        Command(
            "Copy current text",
            "edit-copy-symbolic",
            workspace.copy_current_output,
            lambda: current_document() and workspace.can_copy_current_output(),
            "clipboard output result",
        ),
        Command(
            "Save edits",
            "document-save-symbolic",
            workspace.save_edits,
            lambda: (
                current_document()
                and not workspace.busy
                and not workspace.private
                and not workspace.live_box.get_visible()
                and workspace.entry is not None
                and any(key[0] == workspace.entry.identifier for key in workspace.edit_drafts)
            ),
            "keep document note",
        ),
        Command("History", "document-open-recent-symbolic", app._open_history, lambda: True, "archive search"),
        Command(
            "Live conversation",
            "audio-input-microphone-symbolic",
            lambda: (app._navigate_to_page("capture"), workspace.show_live()),
            lambda: workspace.live_active,
            "current recording return",
        ),
        Command(
            "Toggle history sidebar",
            "sidebar-show-symbolic",
            lambda: app._toggle_history_sidebar(None),
            lambda: True,
            "navigation show hide",
        ),
        Command(
            "Settings",
            "preferences-system-symbolic",
            lambda: app._show_settings(app.settings_button),
            lambda: True,
            "providers models appearance scrolling preferences",
        ),
    )
    for name, choices, title in (
        ("widget_position", WIDGET_POSITIONS, "Widget position"),
        ("time_format", TIME_FORMATS, "Time format"),
        ("live_rewrite_template", TEMPLATE_CHOICES, "Live rewrite template"),
    ):
        for value, label in choices:
            commands += (
                Command(
                    f"{title}: {label}",
                    "preferences-system-symbolic",
                    lambda name=name, value=value: app._apply_workspace_settings({name: value}),
                    lambda name=name: (
                        name != "live_rewrite_template"
                        or (
                            not app.capture_preparing
                            and not app.capture_processing
                            and app.live_final_entry is None
                            and app.rewrite_client is None
                            and not app.config.incognito_mode
                            and not (
                                app.recorder is not None
                                and app.recorder.process is not None
                                and (app.pending_incognito or app.pending_mode != "dictation")
                            )
                        )
                    ),
                    "settings preferences",
                ),
            )
    if hasattr(app, "prompt_store"):
        commands += tuple(
            Command(
                "Edit prompt · " + prompt.name,
                "document-edit-symbolic",
                lambda key=prompt.identifier: app._open_prompt_editor(key),
                lambda: True,
                "prompt template instructions configure",
            )
            for prompt in app.prompt_store.catalog.values()
        )
    return commands + settings_commands(app)


def settings_commands(app: "MluvaApplication") -> tuple[Command, ...]:
    """Index the real settings rows, so new preferences remain reachable without a second schema."""
    commands = []

    def visit(widget, page, group: str = ""):
        if isinstance(widget, Adw.PreferencesGroup):
            group = widget.get_title() or group
        if isinstance(widget, Adw.PreferencesRow) and widget.get_title():
            row = widget
            title = " · ".join(part for part in ("Settings", page.get_title(), group, row.get_title()) if part)
            commands.append(
                Command(
                    title,
                    "preferences-system-symbolic",
                    lambda row=row, page=page: open_setting(app, page, row),
                    lambda: True,
                    "preferences configure " + (row.get_subtitle() or "")
                    if isinstance(row, Adw.ActionRow)
                    else "preferences configure",
                )
            )
        child = widget.get_first_child()
        while child is not None:
            visit(child, page, group)
            child = child.get_next_sibling()

    for page in app.settings_pages:
        visit(page, page)
    return tuple(commands)


def open_setting(app: "MluvaApplication", page: Adw.PreferencesPage, row: Adw.PreferencesRow) -> None:
    """Open the containing page, expand advanced controls and scroll the requested setting into view."""
    app._show_settings(app.settings_button)
    app.settings_dialog.set_visible_page(page)
    parent = row.get_parent()
    while parent is not None and parent is not page:
        if isinstance(parent, Adw.ExpanderRow):
            parent.set_expanded(True)
        parent = parent.get_parent()
    if isinstance(row, Adw.ExpanderRow):
        row.set_expanded(True)

    def focus():
        row.set_focusable(True)
        row.grab_focus()
        return GLib.SOURCE_REMOVE

    GLib.idle_add(focus)


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
