"""GTK editors for dictionaries, snippets, suggestions, and saved output styles."""

from collections.abc import Callable

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, Gtk  # noqa: E402

from voice_scribe_linux.history import HistoryStore
from voice_scribe_linux.personalization import (
    DictionaryCaseBehavior,
    DictionaryReplacement,
    PersonalizationStore,
    SavedStyle,
    Snippet,
)
from voice_scribe_linux.ui import (
    SPACE_1,
    SPACE_2,
    SPACE_3,
    FeatureMaturityNotice,
    clamp,
    page_content,
    set_margins,
)
from voice_scribe_linux.vocabulary import (
    MAX_SUGGESTION_HISTORY_ENTRIES,
    VocabularySuggestion,
    VocabularySuggestionEngine,
)

PERSONALIZATION_FEATURE_IDS = {
    "dictionary": "dictionary_suggestions",
    "suggestions": "dictionary_suggestions",
    "snippets": "snippets",
    "styles": "saved_styles",
}


class PersonalizationPage(Gtk.Box):
    """Edit one personalization category at a time through nested native navigation."""

    def __init__(
        self,
        store: PersonalizationStore,
        history_store: HistoryStore,
        show_message: Callable[[str], None],
        styles_changed: Callable[[], None],
    ) -> None:
        """Build bounded editors around an injected owner-only store."""
        super().__init__(orientation=Gtk.Orientation.VERTICAL)
        self.store = store
        self.history_store = history_store
        self.show_message = show_message
        self.styles_changed = styles_changed
        self.editing_style_identifier: str | None = None

        header = page_content(spacing=SPACE_3)
        header.set_margin_bottom(SPACE_2)
        if store.persistence_error is not None:
            warning = Gtk.Label(
                label=(
                    "The personalization document is malformed and was preserved. Repair it before saving changes: "
                    f"{store.persistence_error}"
                ),
                xalign=0,
                wrap=True,
            )
            warning.add_css_class("error")
            header.append(warning)

        self.view_stack = Adw.ViewStack()
        switcher = Adw.ViewSwitcher(
            stack=self.view_stack,
            policy=Adw.ViewSwitcherPolicy.NARROW,
            hexpand=True,
        )
        header.append(switcher)
        self.feature_maturity = FeatureMaturityNotice(PERSONALIZATION_FEATURE_IDS["dictionary"])
        header.append(self.feature_maturity)
        self.append(clamp(header))

        self.dictionary_editor = self._build_dictionary_editor()
        self.dictionary_list = self._new_list_box()
        self.view_stack.add_titled_with_icon(
            self._subpage(
                self.dictionary_editor,
                "Saved dictionary entries",
                "Exact whole-phrase replacements run locally before optional Codex enhancement.",
                self.dictionary_list,
            ),
            "dictionary",
            "Dictionary",
            "accessories-dictionary-symbolic",
        )

        self.suggestion_list = self._new_list_box()
        self.view_stack.add_titled_with_icon(
            self._subpage(
                None,
                "Suggestions from your edits",
                (
                    "Only small corrections explicitly saved in History appear here. Add or dismiss each one; "
                    "Mluva never learns automatically."
                ),
                self.suggestion_list,
            ),
            "suggestions",
            "Suggestions",
            "dialog-information-symbolic",
        )

        self.snippet_editor = self._build_snippet_editor()
        self.snippet_list = self._new_list_box()
        self.view_stack.add_titled_with_icon(
            self._subpage(
                self.snippet_editor,
                "Saved snippets",
                "Exact spoken and optional typed triggers are expanded locally.",
                self.snippet_list,
            ),
            "snippets",
            "Snippets",
            "insert-text-symbolic",
        )

        self.style_editor = self._build_style_editor()
        self.style_list = self._new_list_box()
        self.view_stack.add_titled_with_icon(
            self._subpage(
                self.style_editor,
                "Output styles",
                "Built-ins are immutable; custom instructions are used only when explicitly selected.",
                self.style_list,
            ),
            "styles",
            "Styles",
            "document-edit-symbolic",
        )
        self.view_stack.set_vexpand(True)
        self.view_stack.connect("notify::visible-child-name", self._visible_category_changed)
        self.append(self.view_stack)

        if store.persistence_error is not None:
            for editor in (self.dictionary_editor, self.snippet_editor, self.style_editor):
                editor.set_sensitive(False)
        self.refresh()

    def _visible_category_changed(self, *_args: object) -> None:
        """Keep the selected personalization surface's maturity visible."""
        name = self.view_stack.get_visible_child_name() or "dictionary"
        self.feature_maturity.present(PERSONALIZATION_FEATURE_IDS[name])

    def _subpage(
        self,
        editor: Adw.ExpanderRow | None,
        list_title: str,
        list_description: str,
        list_box: Gtk.ListBox,
    ) -> Gtk.ScrolledWindow:
        """Build one independently scrolling, clamped personalization category."""
        content = page_content()
        if editor is not None:
            editor_group = Adw.PreferencesGroup()
            editor_group.add(editor)
            content.append(editor_group)
        saved = Adw.PreferencesGroup(title=list_title, description=list_description)
        saved.add(list_box)
        content.append(saved)
        scroll = Gtk.ScrolledWindow(
            vexpand=True,
            hscrollbar_policy=Gtk.PolicyType.NEVER,
            vscrollbar_policy=Gtk.PolicyType.AUTOMATIC,
        )
        scroll.set_child(clamp(content))
        return scroll

    def refresh(self) -> None:
        """Rebuild all durable rows after an atomic store mutation."""
        self._clear_list(self.suggestion_list)
        try:
            suggestions = VocabularySuggestionEngine().suggestions(
                self.history_store.recent(limit=MAX_SUGGESTION_HISTORY_ENTRIES),
                dictionary=self.store.dictionary,
                dismissed_identifiers=self.store.dismissed_vocabulary_suggestion_identifiers,
            )
        except Exception as error:
            self.suggestion_list.append(
                Adw.ActionRow(title="Vocabulary suggestions are unavailable", subtitle=str(error))
            )
        else:
            if not suggestions:
                self.suggestion_list.append(
                    Adw.ActionRow(
                        title="No suggestions awaiting review",
                        subtitle="Save a small explicit correction in History to propose one here.",
                    )
                )
            else:
                for suggestion in suggestions:
                    self.suggestion_list.append(self._suggestion_row(suggestion))

        self._clear_list(self.dictionary_list)
        if not self.store.dictionary:
            self.dictionary_list.append(
                Adw.ActionRow(
                    title="No dictionary entries yet",
                    subtitle="Add one only when Mluva repeatedly writes an exact phrase incorrectly.",
                )
            )
        else:
            for replacement in self.store.dictionary:
                self.dictionary_list.append(self._dictionary_row(replacement))

        self._clear_list(self.snippet_list)
        if not self.store.snippets:
            self.snippet_list.append(
                Adw.ActionRow(
                    title="No snippets yet",
                    subtitle="Add a reusable exact expansion behind a spoken trigger.",
                )
            )
        else:
            for snippet in self.store.snippets:
                self.snippet_list.append(self._snippet_row(snippet))

        self._clear_list(self.style_list)
        for style in self.store.styles:
            self.style_list.append(self._style_row(style))

    def _build_dictionary_editor(self) -> Adw.ExpanderRow:
        """Build a collapsed editor for one exact dictionary upsert."""
        editor = Adw.ExpanderRow(
            title="Add dictionary entry",
            subtitle="Map one exact spoken phrase to its written form",
        )
        self.dictionary_spoken = Adw.EntryRow(title="Spoken phrase")
        self.dictionary_written = Adw.EntryRow(title="Written replacement")
        self.dictionary_application = Adw.EntryRow(title="Application identifier (optional)")
        self.dictionary_application.set_tooltip_text(
            "Leave blank for every application, or use the local executable identity shown by your application."
        )
        self.dictionary_case = Adw.ComboRow(title="Capitalization")
        self.dictionary_case.set_model(Gtk.StringList.new(["Fixed written form", "Match spoken pattern"]))
        for row in (
            self.dictionary_spoken,
            self.dictionary_written,
            self.dictionary_application,
            self.dictionary_case,
        ):
            editor.add_row(row)
        action = Adw.ActionRow(title="Create or update exact phrase")
        save = Gtk.Button(label="Save entry", valign=Gtk.Align.CENTER)
        save.add_css_class("suggested-action")
        save.connect("clicked", self._save_dictionary)
        action.add_suffix(save)
        editor.add_row(action)
        return editor

    def _build_snippet_editor(self) -> Adw.ExpanderRow:
        """Build a collapsed editor for explicit spoken and exact typed snippets."""
        editor = Adw.ExpanderRow(
            title="Add snippet",
            subtitle="Expand a spoken trigger; optional typed triggers remain stored but inactive",
        )
        self.snippet_trigger = Adw.EntryRow(title="Spoken trigger")
        self.snippet_typed = Adw.EntryRow(title="Exact typed trigger (optional)")
        self.snippet_typed.set_tooltip_text(
            "Desktop-wide typed expansion stays disabled until a secure Wayland input boundary is available."
        )
        self.snippet_application = Adw.EntryRow(title="Application identifier (optional)")
        self.snippet_expansion = Gtk.TextView(wrap_mode=Gtk.WrapMode.WORD_CHAR)
        expansion_row = self._text_editor_row(
            "_Expansion",
            self.snippet_expansion,
            110,
            "Variables: {{date}}, {{time}}, {{datetime}}, and {{weekday}}.",
        )
        for row in (
            self.snippet_trigger,
            self.snippet_typed,
            self.snippet_application,
            expansion_row,
        ):
            editor.add_row(row)
        action = Adw.ActionRow(title="Create or update exact trigger")
        save = Gtk.Button(label="Save snippet", valign=Gtk.Align.CENTER)
        save.add_css_class("suggested-action")
        save.connect("clicked", self._save_snippet)
        action.add_suffix(save)
        editor.add_row(action)
        return editor

    def _build_style_editor(self) -> Adw.ExpanderRow:
        """Build a collapsed custom output-style creation and editing surface."""
        editor = Adw.ExpanderRow(
            title="Create custom style",
            subtitle="Custom instructions are sent with dictated text only when selected",
        )
        self.style_name = Adw.EntryRow(title="Custom style name")
        editor.add_row(self.style_name)
        self.style_instructions = Gtk.TextView(wrap_mode=Gtk.WrapMode.WORD_CHAR)
        editor.add_row(
            self._text_editor_row(
                "_Full custom instructions",
                self.style_instructions,
                140,
                "The target application identity is never sent to Codex.",
            )
        )
        actions = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=SPACE_2)
        self.save_style_button = Gtk.Button(label="Create custom style")
        self.save_style_button.add_css_class("suggested-action")
        self.save_style_button.connect("clicked", self._save_style)
        actions.append(self.save_style_button)
        self.cancel_style_button = Gtk.Button(label="Cancel edit")
        self.cancel_style_button.set_visible(False)
        self.cancel_style_button.connect("clicked", self._cancel_style_edit)
        actions.append(self.cancel_style_button)
        action_row = Adw.ActionRow(title="Custom output mode")
        action_row.add_suffix(actions)
        editor.add_row(action_row)
        return editor

    def _dictionary_row(self, replacement: DictionaryReplacement) -> Adw.ActionRow:
        """Render one exact rule with scope, capitalization, and delete action."""
        scope = replacement.application_identifier or "Every application"
        behavior = "fixed" if replacement.case_behavior is DictionaryCaseBehavior.FIXED else "match spoken case"
        row = Adw.ActionRow(title=f"{replacement.spoken} → {replacement.written}", subtitle=f"{scope} · {behavior}")
        delete = Gtk.Button(label="Delete", valign=Gtk.Align.CENTER)
        delete.add_css_class("destructive-action")
        delete.connect("clicked", self._delete_dictionary, replacement)
        row.add_suffix(delete)
        return row

    def _suggestion_row(self, suggestion: VocabularySuggestion) -> Adw.ActionRow:
        """Render one local correction with separate add and dismiss decisions."""
        scope = suggestion.application_identifier or "Every application"
        correction_count = "One correction" if suggestion.occurrences == 1 else f"{suggestion.occurrences} corrections"
        row = Adw.ActionRow(
            title=f"{suggestion.spoken} → {suggestion.written}",
            subtitle=f"{correction_count} · {scope}",
        )
        add = Gtk.Button(label="Add", valign=Gtk.Align.CENTER)
        add.add_css_class("suggested-action")
        add.connect("clicked", self._add_suggestion, suggestion)
        row.add_suffix(add)
        dismiss = Gtk.Button(label="Dismiss", valign=Gtk.Align.CENTER)
        dismiss.connect("clicked", self._dismiss_suggestion, suggestion)
        row.add_suffix(dismiss)
        return row

    def _snippet_row(self, snippet: Snippet) -> Adw.ExpanderRow:
        """Render one snippet with full expansion and exact scope metadata."""
        scope = snippet.application_identifier or "Every application"
        typed = f" · typed {snippet.typed_trigger}" if snippet.typed_trigger is not None else ""
        row = Adw.ExpanderRow(title=f"snippet {snippet.trigger}", subtitle=f"{scope}{typed}")
        expansion = Gtk.Label(label=snippet.expansion, xalign=0, selectable=True, wrap=True)
        set_margins(expansion, SPACE_3)
        row.add_row(expansion)
        action = Adw.ActionRow(title="Remove this exact snippet")
        delete = Gtk.Button(label="Delete", valign=Gtk.Align.CENTER)
        delete.add_css_class("destructive-action")
        delete.connect("clicked", self._delete_snippet, snippet)
        action.add_suffix(delete)
        row.add_row(action)
        return row

    def _style_row(self, style: SavedStyle) -> Adw.ExpanderRow:
        """Render full style instructions and custom-only edit/delete actions."""
        row = Adw.ExpanderRow(
            title=style.name,
            subtitle="Built-in output mode" if style.is_built_in else "Custom output mode",
        )
        instructions = Gtk.Label(label=style.instructions, xalign=0, selectable=True, wrap=True)
        set_margins(instructions, SPACE_3)
        row.add_row(instructions)
        if not style.is_built_in:
            actions = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=SPACE_2)
            edit = Gtk.Button(label="Edit")
            edit.connect("clicked", self._begin_style_edit, style)
            actions.append(edit)
            delete = Gtk.Button(label="Delete")
            delete.add_css_class("destructive-action")
            delete.connect("clicked", self._delete_style, style)
            actions.append(delete)
            action = Adw.ActionRow(title="Custom mode actions")
            action.add_suffix(actions)
            row.add_row(action)
        return row

    def _save_dictionary(self, _button: Gtk.Button) -> None:
        """Validate and atomically upsert the dictionary editor values."""
        behavior = (
            DictionaryCaseBehavior.FIXED
            if self.dictionary_case.get_selected() == 0
            else DictionaryCaseBehavior.MATCH_SPOKEN
        )
        try:
            self.store.save_dictionary_replacement(
                self.dictionary_spoken.get_text(),
                self.dictionary_written.get_text(),
                self.dictionary_application.get_text() or None,
                behavior,
            )
        except Exception as error:
            self.show_message(f"Dictionary entry could not be saved: {error}")
            return
        self.dictionary_spoken.set_text("")
        self.dictionary_written.set_text("")
        self.dictionary_editor.set_expanded(False)
        self.refresh()
        self.show_message("Dictionary entry saved locally.")

    def _add_suggestion(self, _button: Gtk.Button, suggestion: VocabularySuggestion) -> None:
        """Add one reviewed correction to the exact local application scope."""
        try:
            self.store.save_dictionary_replacement(
                suggestion.spoken,
                suggestion.written,
                application_identifier=suggestion.application_identifier,
            )
        except Exception as error:
            self.show_message(f"Vocabulary suggestion could not be added: {error}")
            return
        self.refresh()
        self.show_message("Reviewed vocabulary suggestion added locally.")

    def _dismiss_suggestion(self, _button: Gtk.Button, suggestion: VocabularySuggestion) -> None:
        """Hide one reviewed suggestion durably without creating a replacement."""
        try:
            self.store.dismiss_vocabulary_suggestion(suggestion.identifier)
        except Exception as error:
            self.show_message(f"Vocabulary suggestion could not be dismissed: {error}")
            return
        self.refresh()
        self.show_message("Vocabulary suggestion dismissed.")

    def _save_snippet(self, _button: Gtk.Button) -> None:
        """Validate and atomically upsert the snippet editor values."""
        try:
            self.store.save_snippet(
                self.snippet_trigger.get_text(),
                self._text_view_value(self.snippet_expansion),
                self.snippet_typed.get_text() or None,
                self.snippet_application.get_text() or None,
            )
        except Exception as error:
            self.show_message(f"Snippet could not be saved: {error}")
            return
        self.snippet_trigger.set_text("")
        self.snippet_typed.set_text("")
        self._set_text_view_value(self.snippet_expansion, "")
        self.snippet_editor.set_expanded(False)
        self.refresh()
        self.show_message("Snippet saved locally.")

    def _save_style(self, _button: Gtk.Button) -> None:
        """Create or edit one custom style from the full visible instructions."""
        try:
            if self.editing_style_identifier is None:
                self.store.save_style(self.style_name.get_text(), self._text_view_value(self.style_instructions))
            else:
                self.store.update_style(
                    self.editing_style_identifier,
                    self.style_name.get_text(),
                    self._text_view_value(self.style_instructions),
                )
        except Exception as error:
            self.show_message(f"Custom style could not be saved: {error}")
            return
        self._clear_style_editor()
        self.style_editor.set_expanded(False)
        self.refresh()
        self.styles_changed()
        self.show_message("Custom output style saved locally.")

    def _begin_style_edit(self, _button: Gtk.Button, style: SavedStyle) -> None:
        """Load one custom style into the visible editor without mutating it."""
        self.editing_style_identifier = style.identifier
        self.style_name.set_text(style.name)
        self._set_text_view_value(self.style_instructions, style.instructions)
        self.save_style_button.set_label("Save custom style")
        self.cancel_style_button.set_visible(True)
        self.style_editor.set_expanded(True)
        self.show_message(f"Editing custom style “{style.name}”.")

    def _cancel_style_edit(self, _button: Gtk.Button) -> None:
        """Clear the custom editor without changing durable style state."""
        self._clear_style_editor()
        self.style_editor.set_expanded(False)
        self.show_message("Custom style edit cancelled.")

    def _clear_style_editor(self) -> None:
        """Return the custom style editor to creation state."""
        self.editing_style_identifier = None
        self.style_name.set_text("")
        self._set_text_view_value(self.style_instructions, "")
        self.save_style_button.set_label("Create custom style")
        self.cancel_style_button.set_visible(False)

    def _delete_dictionary(self, _button: Gtk.Button, replacement: DictionaryReplacement) -> None:
        """Delete only the selected dictionary identifier."""
        try:
            self.store.delete_dictionary_replacement(replacement.identifier)
        except Exception as error:
            self.show_message(f"Dictionary entry could not be deleted: {error}")
            return
        self.refresh()
        self.show_message("Dictionary entry deleted.")

    def _delete_snippet(self, _button: Gtk.Button, snippet: Snippet) -> None:
        """Delete only the selected snippet identifier."""
        try:
            self.store.delete_snippet(snippet.identifier)
        except Exception as error:
            self.show_message(f"Snippet could not be deleted: {error}")
            return
        self.refresh()
        self.show_message("Snippet deleted.")

    def _delete_style(self, _button: Gtk.Button, style: SavedStyle) -> None:
        """Delete one custom style and refresh capture selections."""
        try:
            self.store.delete_style(style.identifier)
        except Exception as error:
            self.show_message(f"Custom style could not be deleted: {error}")
            return
        if self.editing_style_identifier == style.identifier:
            self._clear_style_editor()
        self.refresh()
        self.styles_changed()
        self.show_message("Custom output style deleted.")

    @staticmethod
    def _new_list_box() -> Gtk.ListBox:
        """Create one non-selecting boxed list for durable rows."""
        list_box = Gtk.ListBox(selection_mode=Gtk.SelectionMode.NONE)
        list_box.add_css_class("boxed-list")
        return list_box

    @staticmethod
    def _clear_list(list_box: Gtk.ListBox) -> None:
        """Remove every current child without relying on stale row indexes."""
        child = list_box.get_first_child()
        while child is not None:
            next_child = child.get_next_sibling()
            list_box.remove(child)
            child = next_child

    @staticmethod
    def _text_editor_row(
        title: str,
        text_view: Gtk.TextView,
        minimum_height: int,
        description: str,
    ) -> Gtk.Box:
        """Create one directly labelled multiline editor without nested focus chrome."""
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=SPACE_1)
        set_margins(box, SPACE_3)
        label = Gtk.Label(label=title, xalign=0, use_underline=True)
        label.add_css_class("heading")
        label.set_mnemonic_widget(text_view)
        box.append(label)
        help_label = Gtk.Label(label=description, xalign=0, wrap=True)
        help_label.add_css_class("caption")
        help_label.add_css_class("dim-label")
        box.append(help_label)
        scroll = Gtk.ScrolledWindow(min_content_height=minimum_height)
        scroll.set_child(text_view)
        box.append(scroll)
        return box

    @staticmethod
    def _text_view_value(text_view: Gtk.TextView) -> str:
        """Read the complete visible multiline editor value."""
        buffer = text_view.get_buffer()
        start, end = buffer.get_bounds()
        return buffer.get_text(start, end, include_hidden_chars=True)

    @staticmethod
    def _set_text_view_value(text_view: Gtk.TextView, value: str) -> None:
        """Replace one complete multiline editor value."""
        text_view.get_buffer().set_text(value)
