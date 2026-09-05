"""A full-text dictation and rewrite workspace with searchable local history."""

from collections.abc import Callable
from datetime import datetime

import gi

from voice_scribe_linux.conversation import QUICK_POLISH, STRUCTURED_NOTE, ConversationStore, Rewrite
from voice_scribe_linux.history import HistoryEntry
from voice_scribe_linux.ui import SPACE_2, SPACE_3, set_margins

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, Gdk, GLib, Gtk, Pango  # noqa: E402


class ConversationWorkspace(Gtk.Box):
    """Keep history, source, versions and explicit rewrite actions in one place."""

    def __init__(
        self,
        store: ConversationStore,
        copy_text: Callable[[str], None],
        rewrite: Callable[[str], None],
        paste_text: Callable[[str], None],
        open_archive: Callable[[], None],
        save_prompt: Callable[[str], None],
        cancel_rewrite: Callable[[], None],
    ) -> None:
        """Bind user intentions while leaving recording and provider work to the application."""
        super().__init__(orientation=Gtk.Orientation.VERTICAL)
        self.store = store
        self.copy_text = copy_text
        self.request_rewrite = rewrite
        self.paste_text = paste_text
        self.open_archive = open_archive
        self.save_prompt = save_prompt
        self.cancel_rewrite = cancel_rewrite
        self.entry: HistoryEntry | None = None
        self.busy = False
        self.private = False
        self.drafts: dict[str, str] = {}
        self.result_widgets: list[Gtk.TextView] = []
        self.rows: dict[Gtk.ListBoxRow, str] = {}
        self.search_limit = 80
        self.split = Adw.OverlaySplitView(vexpand=True)
        self.split.set_min_sidebar_width(220)
        self.split.set_max_sidebar_width(260)
        self.split.set_sidebar_width_fraction(0.25)
        self.split.set_sidebar(self._build_sidebar())
        self.split.set_content(self._build_content())
        self.append(self.split)
        self.show_conversation(None, [])
        self.refresh_history()

    def _build_sidebar(self) -> Gtk.Box:
        """Keep the identity small and give history the sidebar's useful space."""
        sidebar = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=SPACE_2)
        sidebar.add_css_class("ml-history-sidebar")
        set_margins(sidebar, SPACE_3)
        brand = Gtk.Box(spacing=SPACE_2)
        brand.set_margin_top(6)
        brand.set_margin_bottom(10)
        icon = Gtk.Image.new_from_icon_name("com.voicescribe.Linux")
        icon.set_pixel_size(28)
        brand.append(icon)
        wordmark = Gtk.Label(label="Mluva", xalign=0)
        wordmark.add_css_class("ml-wordmark")
        brand.append(wordmark)
        sidebar.append(brand)
        new = Gtk.Button(label="New conversation")
        new.set_tooltip_text("Start with dictation or pasted text")
        new.connect("clicked", lambda _button: self.show_conversation(None, []))
        sidebar.append(new)
        self.search = Gtk.SearchEntry(placeholder_text="Search history")
        self.search.connect("search-changed", self._search_changed)
        sidebar.append(self.search)
        self.history_list = Gtk.ListBox(selection_mode=Gtk.SelectionMode.SINGLE)
        self.history_list.add_css_class("navigation-sidebar")
        self.history_list.connect("row-activated", self._open_row)
        scroll = Gtk.ScrolledWindow(hscrollbar_policy=Gtk.PolicyType.NEVER, vexpand=True)
        scroll.set_child(self.history_list)
        sidebar.append(scroll)
        self.more = Gtk.Button(label="Show more")
        self.more.connect("clicked", self._show_more)
        sidebar.append(self.more)
        archive = Gtk.Button(label="Manage history", has_frame=False)
        archive.set_tooltip_text("Rename, export, recover or delete dictations")
        archive.connect("clicked", lambda _button: self.open_archive())
        sidebar.append(archive)
        pane = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        pane.add_css_class("ml-history-pane")
        pane.append(sidebar)
        return pane

    def _build_content(self) -> Gtk.Box:
        """Build a scrolling conversation and fixed, discoverable rewrite composer."""
        content = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        content.add_css_class("ml-conversation")
        self.messages = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=24)
        set_margins(self.messages, 24)
        reading_width = Adw.Clamp(maximum_size=780, tightening_threshold=580, child=self.messages)
        self.scroll = Gtk.ScrolledWindow(hscrollbar_policy=Gtk.PolicyType.NEVER, vexpand=True)
        self.scroll.set_child(reading_width)
        content.append(self.scroll)

        self.live_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=SPACE_2, vexpand=True)
        self.live_box.add_css_class("ml-live")
        set_margins(self.live_box, SPACE_3)
        self.live_title = Gtk.Label(xalign=0)
        self.live_title.add_css_class("heading")
        self.live_box.append(self.live_title)
        self.live_text = self._text_view("")
        self.live_scroll = Gtk.ScrolledWindow(hscrollbar_policy=Gtk.PolicyType.NEVER, vexpand=True)
        self.live_scroll.set_child(self.live_text)
        self.live_box.append(self.live_scroll)
        self.live_box.set_visible(False)
        content.append(self.live_box)

        composer = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=SPACE_2)
        self.composer = composer
        composer.add_css_class("ml-composer")
        set_margins(composer, SPACE_3)
        self.actions = Gtk.FlowBox(
            selection_mode=Gtk.SelectionMode.NONE,
            column_spacing=SPACE_2,
            row_spacing=SPACE_2,
            max_children_per_line=3,
            min_children_per_line=1,
        )
        self.quick_polish = Gtk.Button(label="Quick Polish")
        self.quick_polish.set_tooltip_text("Remove filler words and fix phrasing while keeping your meaning")
        self.quick_polish.connect("clicked", lambda _button: self.request_rewrite(QUICK_POLISH))
        self.actions.insert(self.quick_polish, -1)
        self.structured_note = Gtk.Button(label="Structured Note")
        self.structured_note.set_tooltip_text("A concise summary followed by organized bullet points")
        self.structured_note.connect("clicked", lambda _button: self.request_rewrite(STRUCTURED_NOTE))
        self.actions.insert(self.structured_note, -1)
        self.saved_prompts = Gtk.MenuButton(label="Saved prompts")
        self.actions.insert(self.saved_prompts, -1)
        composer.append(self.actions)

        self.prompt_label = Gtk.Label(xalign=0)
        self.prompt_label.add_css_class("caption")
        composer.append(self.prompt_label)
        self.prompt = Gtk.TextView(wrap_mode=Gtk.WrapMode.WORD_CHAR, accepts_tab=False)
        self.prompt.add_css_class("ml-prompt")
        self.prompt.set_tooltip_text("Write a custom instruction, then press Ctrl+Enter to send")
        prompt_scroll = Gtk.ScrolledWindow(
            min_content_height=64, max_content_height=140, hscrollbar_policy=Gtk.PolicyType.NEVER
        )
        prompt_scroll.set_propagate_natural_height(True)
        prompt_scroll.set_child(self.prompt)
        composer.append(prompt_scroll)
        keys = Gtk.EventControllerKey()
        keys.connect("key-pressed", self._prompt_key)
        self.prompt.add_controller(keys)
        footer = Gtk.Box(spacing=SPACE_2)
        self.notice = Gtk.Label(xalign=0, wrap=True, hexpand=True, accessible_role=Gtk.AccessibleRole.STATUS)
        self.notice.add_css_class("caption")
        self.notice.set_max_width_chars(28)
        footer.append(self.notice)
        self.save = Gtk.Button(icon_name="bookmark-new-symbolic")
        self.save.set_tooltip_text("Save this prompt for later")
        self.save.connect("clicked", lambda _button: self.save_prompt(self.prompt_text()))
        footer.append(self.save)
        self.cancel = Gtk.Button(label="Cancel")
        self.cancel.connect("clicked", lambda _button: self.cancel_rewrite())
        self.cancel.set_visible(False)
        footer.append(self.cancel)
        self.send = Gtk.Button(label="Rewrite")
        self.send.add_css_class("suggested-action")
        self.send.add_css_class("ml-primary")
        self.send.connect("clicked", self._submit)
        footer.append(self.send)
        composer.append(footer)
        content.append(composer)
        return content

    @staticmethod
    def _text_view(text: str) -> Gtk.TextView:
        """Display complete selectable text without ellipses or a fixed line limit."""
        view = Gtk.TextView(editable=False, cursor_visible=False, wrap_mode=Gtk.WrapMode.WORD_CHAR)
        view.add_css_class("ml-transcript")
        view.set_top_margin(4)
        view.set_bottom_margin(4)
        view.get_buffer().set_text(text)
        return view

    def _message(self, title: str, text: str, source: bool = False) -> None:
        """Add a source or rewrite with its own explicit Copy action."""
        message = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=SPACE_2)
        message.add_css_class("ml-source" if source else "ml-reply")
        header = Gtk.Box(spacing=SPACE_2)
        heading = Gtk.Label(label=title, xalign=0, hexpand=True)
        heading.add_css_class("heading")
        header.append(heading)
        copy = Gtk.Button(label="Copy", has_frame=False)
        copy.set_tooltip_text(f"Copy {title.lower()}")
        copy.connect("clicked", lambda _button: self.copy_text(text))
        header.append(copy)
        message.append(header)
        view = self._text_view(text)
        self.result_widgets.append(view)
        message.append(view)
        self.messages.append(message)

    def show_conversation(self, entry: HistoryEntry | None, replies: list[Rewrite]) -> None:
        """Open the exact selected history item and every saved rewrite, preserving original text."""
        self.drafts[self.entry.identifier if self.entry else "new"] = self.prompt_text()
        self.entry = entry
        self.result_widgets.clear()
        child = self.messages.get_first_child()
        while child is not None:
            self.messages.remove(child)
            child = self.messages.get_first_child()
        if entry is None:
            empty = Adw.StatusPage(
                title="Your words, ready to use",
                description="Press F9 to dictate. Or paste text below.\nPolish it, structure it, make it yours.",
                icon_name="com.voicescribe.Linux",
            )
            empty.add_css_class("ml-empty")
            self.messages.append(empty)
        else:
            title = Gtk.Label(label=entry.title or "Your dictation", xalign=0, wrap=True)
            title.add_css_class("ml-conversation-title")
            self.messages.append(title)
            self._message("Original", entry.raw_text, source=True)
            if entry.delivered_text and entry.delivered_text != entry.raw_text:
                self._message("Copied dictation", entry.delivered_text)
            for reply in replies:
                instruction = (
                    "Quick Polish"
                    if reply.instruction == QUICK_POLISH
                    else "Structured Note"
                    if reply.instruction == STRUCTURED_NOTE
                    else reply.instruction
                )
                request = Gtk.Label(label=instruction, xalign=1, wrap=True, selectable=True)
                request.add_css_class("ml-instruction")
                self.messages.append(request)
                self._message("Rewrite", reply.text)
        self.prompt.get_buffer().set_text(self.drafts.get(entry.identifier if entry else "new", ""))
        self.prompt_label.set_label(
            "Ask for a rewrite or a follow-up" if entry else "Paste or type text to get started"
        )
        self.send.set_label("Rewrite" if entry else "Start conversation")
        self.notice.set_label(
            "Rewriting · Experimental. Your original stays here." if entry else "Dictation copies automatically."
        )
        self._update_actions()
        for row, identifier in self.rows.items():
            if entry is not None and identifier == entry.identifier:
                self.history_list.select_row(row)
        if self.split.get_collapsed():
            self.split.set_show_sidebar(False)

    def scroll_to_latest(self) -> None:
        """Reveal a completed reply after GTK measures it without moving a newly selected conversation."""
        if not self.result_widgets:
            return
        latest = self.result_widgets[-1]

        def reveal() -> bool:
            """Follow only the reply that requested this deferred scroll."""
            if self.result_widgets and self.result_widgets[-1] is latest:
                buffer = latest.get_buffer()
                mark = buffer.create_mark(None, buffer.get_end_iter(), False)
                latest.scroll_mark_onscreen(mark)
                buffer.delete_mark(mark)
            return GLib.SOURCE_REMOVE

        GLib.timeout_add(50, reveal)

    def refresh_history(self) -> None:
        """Search the whole archive while rendering only the requested page of results."""
        self.rows.clear()
        child = self.history_list.get_first_child()
        while child is not None:
            self.history_list.remove(child)
            child = self.history_list.get_first_child()
        entries = self.store.search(self.search.get_text(), self.search_limit)
        for entry in entries:
            box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
            set_margins(box, SPACE_2)
            title = Gtk.Label(
                label=entry.title or " ".join((entry.raw_text or "Transcription needs attention").split()),
                xalign=0,
                ellipsize=Pango.EllipsizeMode.END,
            )
            title.set_max_width_chars(23)
            box.append(title)
            stamp = datetime.fromisoformat(entry.created_at).astimezone().strftime("%d %b, %H:%M")
            date = Gtk.Label(label=stamp, xalign=0)
            date.add_css_class("caption")
            date.add_css_class("dim-label")
            box.append(date)
            row = Gtk.ListBoxRow(child=box)
            self.rows[row] = entry.identifier
            self.history_list.append(row)
            if self.entry is not None and self.entry.identifier == entry.identifier:
                self.history_list.select_row(row)
        if not entries:
            label = Gtk.Label(
                label="No matching conversations" if self.search.get_text() else "Your history will appear here.",
                wrap=True,
            )
            label.add_css_class("dim-label")
            self.history_list.append(label)
        self.more.set_visible(len(entries) == self.search_limit)

    def show_transient(self, original: str, text: str) -> None:
        """Show an unsaved Incognito result with explicit copying and no durable source."""
        self.show_conversation(None, [])
        self.messages.remove(self.messages.get_first_child())
        self._message("Original", original, source=True)
        if text != original:
            self._message("Dictation", text)
        self.notice.set_label("This conversation is not saved.")

    def _open_row(self, _list: Gtk.ListBox, row: Gtk.ListBoxRow) -> None:
        """Navigate to the selected source instead of a generic archive screen."""
        identifier = self.rows.get(row)
        if identifier is not None:
            self.show_conversation(self.store.history.find(identifier), self.store.replies(identifier))

    def _search_changed(self, _entry: Gtk.SearchEntry) -> None:
        """Restart pagination when the search changes."""
        self.search_limit = 80
        self.refresh_history()

    def _show_more(self, _button: Gtk.Button) -> None:
        """Reveal the next bounded group without losing the active query."""
        self.search_limit += 80
        self.refresh_history()

    def prompt_text(self) -> str:
        """Read the entire multiline instruction or pasted source."""
        buffer = self.prompt.get_buffer()
        return buffer.get_text(buffer.get_start_iter(), buffer.get_end_iter(), False).strip()

    def _submit(self, _button: Gtk.Button) -> None:
        """Distinguish an explicit source import from a contextual rewrite."""
        text = self.prompt_text()
        if not text or self.busy:
            return
        if self.entry is None:
            self.paste_text(text)
        else:
            self.request_rewrite(text)

    def _prompt_key(self, _controller: Gtk.EventControllerKey, key: int, _code: int, state: Gdk.ModifierType) -> bool:
        """Send on Ctrl+Enter while preserving plain Enter for paragraphs."""
        if key == Gdk.KEY_Return and state & Gdk.ModifierType.CONTROL_MASK:
            self._submit(self.send)
            return True
        return False

    def set_busy(self, busy: bool, message: str) -> None:
        """Make processing visible and prevent duplicate requests without blocking history."""
        self.busy = busy
        self.notice.set_label(message)
        self._update_actions()

    def _update_actions(self) -> None:
        """Make privacy and in-flight work authoritative for all rewrite entry points."""
        self.cancel.set_visible(self.busy)
        self.actions.set_sensitive(self.entry is not None and not self.busy and not self.private)
        self.send.set_sensitive(not self.busy and (self.entry is None or not self.private))
        self.save.set_sensitive(self.entry is not None and not self.private and not self.busy)
        if self.private:
            self.notice.set_label("Incognito: no saved conversation. Rewriting is unavailable.")

    def set_private(self, private: bool) -> None:
        """Prevent rewrite and prompt persistence in Incognito."""
        self.private = private
        if private:
            self.drafts.clear()
        self._update_actions()

    def set_live(self, phase: str, text: str) -> None:
        """Show every live word separately from committed, copyable messages."""
        self.live_box.set_visible(True)
        self.scroll.set_visible(False)
        self.composer.set_visible(False)
        self.live_title.set_label(phase)
        buffer = self.live_text.get_buffer()
        adjustment = self.live_scroll.get_vadjustment()
        following = adjustment.get_value() + adjustment.get_page_size() >= adjustment.get_upper() - 24
        buffer.set_text(text)
        if following:
            mark = buffer.create_mark(None, buffer.get_end_iter(), False)
            self.live_text.scroll_mark_onscreen(mark)
            buffer.delete_mark(mark)

    def finish_live(self) -> None:
        """Erase volatile text when finalization completes or capture is cancelled."""
        self.live_text.get_buffer().set_text("")
        self.live_box.set_visible(False)
        self.scroll.set_visible(True)
        self.composer.set_visible(True)

    def set_saved_prompts(self, prompts: list[tuple[str, str]]) -> None:
        """Expose existing custom styles as one-click rewrite prompts."""
        popover = Gtk.Popover()
        choices = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        set_margins(choices, SPACE_2)
        for name, instruction in prompts:
            button = Gtk.Button(label=name, has_frame=False)
            button.connect("clicked", self._saved_prompt_clicked, instruction, popover)
            choices.append(button)
        if not prompts:
            choices.append(Gtk.Label(label="Write a prompt below, then save it.", wrap=True))
        popover.set_child(choices)
        self.saved_prompts.set_popover(popover)

    def _saved_prompt_clicked(self, _button: Gtk.Button, instruction: str, popover: Gtk.Popover) -> None:
        """Dismiss the menu and run one selected prompt."""
        popover.popdown()
        self.request_rewrite(instruction)
