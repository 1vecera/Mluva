"""Native prompt discovery and editing, shared by Settings, hover controls and Ctrl+P."""

from collections.abc import Callable

import gi

from mluva_linux.prompts import MAX_PROMPT_CHARACTERS, PromptStore

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, Gtk, Pango  # noqa: E402


def prompt_control(widget: Gtk.Widget, name: str, open_editor: Callable[[], None]) -> Gtk.Box:
    """Keep editing separate from execution, reserving a stable hover/focus target."""
    row = Gtk.Box(spacing=2, css_classes=["ml-prompt-control"])
    widget.set_hexpand(True)
    row.append(widget)
    edit = Gtk.Button(icon_name="emblem-system-symbolic", css_classes=["flat", "ml-prompt-settings"])
    edit.set_tooltip_text("Edit prompt · " + name)
    edit.update_property([Gtk.AccessibleProperty.LABEL], ["Edit prompt · " + name])
    edit.connect("clicked", lambda _button: open_editor())
    row.append(edit)
    reveal_prompt_button(row, edit)
    return row


def reveal_prompt_button(row: Gtk.Widget, button: Gtk.Button) -> None:
    """Track the whole row so the icon stays visible while pointer or keyboard focus enters it."""
    motion = Gtk.EventControllerMotion()
    focus = Gtk.EventControllerFocus()

    def update(*_args):
        button.set_opacity(1 if motion.contains_pointer() or focus.contains_focus() else 0)

    motion.connect("notify::contains-pointer", update)
    focus.connect("notify::contains-focus", update)
    row.add_controller(motion)
    row.add_controller(focus)
    button.set_opacity(0)


class PromptEditor(Adw.Dialog):
    """Preserve invalid and unsaved text, with an explicit Save/Cancel boundary."""

    def __init__(self, store: PromptStore, identifier: str, changed: Callable[[], None], writable: Callable[[], bool]):
        """Open one exact prompt with a local-file conflict token."""
        prompt = store.catalog[identifier]
        super().__init__(title=prompt.name, content_width=720, content_height=640)
        self.store, self.identifier, self.changed, self.writable = store, identifier, changed, writable
        self.state = store.read(identifier)
        self.reset_pending = False
        self.discard_dialog = None
        toolbar = Adw.ToolbarView()
        header = Adw.HeaderBar(show_start_title_buttons=False, show_end_title_buttons=False)
        self.cancel_button = Gtk.Button(label="Cancel")
        self.cancel_button.connect("clicked", lambda _button: self.close())
        header.pack_start(self.cancel_button)
        self.save_button = Gtk.Button(label="Save", css_classes=["suggested-action"])
        self.save_button.connect("clicked", self._save)
        header.pack_end(self.save_button)
        toolbar.add_top_bar(header)
        body = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
        for setter in (body.set_margin_top, body.set_margin_bottom, body.set_margin_start, body.set_margin_end):
            setter(20)
        purpose = Gtk.Label(label=prompt.purpose, xalign=0, wrap=True)
        body.append(purpose)
        self.identity = Gtk.Label(xalign=0, wrap=True, css_classes=["caption", "dim-label"])
        baseline = "Built-in default" if prompt.built_in else "Original saved text"
        self.identity.set_label("Local override" if self.state.overridden else baseline)
        body.append(self.identity)
        location = Gtk.Label(
            label=str(store.path(identifier)),
            xalign=0,
            ellipsize=Pango.EllipsizeMode.MIDDLE,
            selectable=True,
            css_classes=["caption", "dim-label"],
        )
        location.set_tooltip_text(str(store.path(identifier)))
        body.append(location)
        self.editor = Gtk.TextView(
            wrap_mode=Gtk.WrapMode.WORD_CHAR,
            accepts_tab=True,
            monospace=True,
            top_margin=16,
            bottom_margin=16,
            left_margin=16,
            right_margin=16,
        )
        self.editor.add_css_class("ml-prompt-editor")
        # Invalid UTF-8 is visible with replacements; saving requires an explicit edit.
        initial = (
            self.state.token.decode("utf-8", errors="replace")
            if self.state.error and self.state.token is not None
            else self.state.text
        )
        self.original = initial
        self.editor.get_buffer().set_text(initial)
        scroll = Gtk.ScrolledWindow(
            child=self.editor, vexpand=True, hscrollbar_policy=Gtk.PolicyType.NEVER, min_content_height=100
        )
        body.append(scroll)
        self.status = Gtk.Label(xalign=0, wrap=True, css_classes=["caption"])
        body.append(self.status)
        footer = Gtk.Box(spacing=12)
        self.reset_button = Gtk.Button(label="Restore default" if prompt.built_in else "Restore original")
        self.reset_button.connect("clicked", self._reset)
        footer.append(self.reset_button)
        self.count = Gtk.Label(hexpand=True, xalign=1, css_classes=["caption", "dim-label"])
        footer.append(self.count)
        body.append(footer)
        note = Gtk.Label(
            label=(
                "Save applies to the next request. A running Live session keeps its instructions until the next "
                "recording. Response format and source-integrity rules remain fixed."
            ),
            xalign=0,
            wrap=True,
            css_classes=["caption", "dim-label"],
        )
        body.append(note)
        toolbar.set_content(body)
        self.set_child(toolbar)
        self.set_focus(self.editor)
        self.editor.get_buffer().connect("changed", self._edited)
        self.connect("close-attempt", self._close_attempt)
        self._edited()

    def text(self) -> str:
        """Read exact editor text, including trailing whitespace."""
        buffer = self.editor.get_buffer()
        return buffer.get_text(buffer.get_start_iter(), buffer.get_end_iter(), False)

    def _edited(self, *_args) -> None:
        text = self.text()
        if self.reset_pending and text != self.store.catalog[self.identifier].default:
            self.reset_pending = False
        dirty = text != self.original or self.reset_pending
        self.set_can_close(not dirty)
        self.count.set_label(f"{len(text):,} / {MAX_PROMPT_CHARACTERS:,}")
        error = ""
        try:
            self.store.validate(self.identifier, text)
        except ValueError as problem:
            error = str(problem)
        if not self.writable():
            error = "Incognito · viewing only. Prompt changes are not saved."
        self.save_button.set_sensitive((dirty or not self.state.overridden) and not error)
        self.reset_button.set_sensitive(self.writable())
        self.status.set_label(
            error
            or (
                "Default staged · Save to apply"
                if self.reset_pending
                else self.state.error or "Unsaved changes"
                if dirty
                else self.state.error
            )
        )

    def _reset(self, _button) -> None:
        self.editor.get_buffer().set_text(self.store.catalog[self.identifier].default)
        self.reset_pending = True
        self._edited()

    def _save(self, _button) -> None:
        if not self.writable():
            self._edited()
            return
        try:
            if self.reset_pending and self.text() == self.store.catalog[self.identifier].default:
                self.store.reset(self.identifier, self.state.token)
            else:
                self.store.save(self.identifier, self.text(), self.state.token)
        except (OSError, ValueError) as error:
            self.status.set_label(str(error))
            return
        self.changed()
        self.force_close()

    def _close_attempt(self, _dialog) -> None:
        if self.discard_dialog is not None:
            return
        confirm = Adw.AlertDialog(heading="Discard prompt changes?", body="Your saved prompt will stay unchanged.")
        confirm.add_response("keep", "Keep editing")
        confirm.add_response("discard", "Discard")
        confirm.set_default_response("keep")
        confirm.set_close_response("keep")
        confirm.set_response_appearance("discard", Adw.ResponseAppearance.DESTRUCTIVE)
        self.discard_dialog = confirm

        def response(_alert, choice):
            self.discard_dialog = None
            if choice == "discard":
                self.force_close()

        confirm.connect("response", response)
        confirm.present(self)


class PromptsPage(Adw.PreferencesPage):
    """Expose the complete current catalog through native Settings search."""

    def __init__(self, store: PromptStore, open_editor: Callable[[str], None]):
        """Build a searchable catalog without executing any prompt."""
        super().__init__(name="prompts", title="Prompts", icon_name="document-edit-symbolic")
        self.store, self.open_editor = store, open_editor
        self.group = Adw.PreferencesGroup(
            title="Prompts",
            description=(
                "Edit task instructions and Markdown structures. Local .md files reload when opened or used; "
                "active Live sessions keep a snapshot."
            ),
        )
        self.add(self.group)
        self.rows = []
        self.refresh()

    def refresh(self) -> None:
        """Refresh identities and recoverable local-file errors."""
        for row in self.rows:
            self.group.remove(row)
        self.rows = []
        for prompt in self.store.catalog.values():
            state = self.store.read(prompt.identifier)
            row = Adw.ActionRow(title=prompt.name, subtitle=state.error or prompt.purpose, activatable=True)
            row.add_suffix(Gtk.Image.new_from_icon_name("document-edit-symbolic"))
            row.connect("activated", lambda _row, key=prompt.identifier: self.open_editor(key))
            self.group.add(row)
            self.rows.append(row)
