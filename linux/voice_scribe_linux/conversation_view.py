"""A full-text dictation and rewrite workspace with searchable local history."""

from collections.abc import Callable
from datetime import datetime

import gi

from voice_scribe_linux.config import AppConfig
from voice_scribe_linux.conversation import QUICK_POLISH, STRUCTURED_NOTE, ConversationStore, Rewrite
from voice_scribe_linux.conversation_titles import fallback_title
from voice_scribe_linux.history import HistoryEntry
from voice_scribe_linux.markdown_view import MarkdownTextView
from voice_scribe_linux.ui import SPACE_2, SPACE_4, brand_mark, document_scroll, set_margins

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, Gdk, GLib, Gtk, Pango  # noqa: E402


class _TailFollower:
    """Ease a growing viewport toward its end while respecting a reader who scrolls away."""

    def __init__(self, scroll: Gtk.ScrolledWindow) -> None:
        """Track allocation changes independently from animation writes and manual scrolling."""
        self.scroll = scroll
        self.following = True
        self.pending = False
        self.writing = False
        self.snap_next = True
        self.animation: Adw.Animation | None = None
        self.destination = 0.0
        self.duration_ms = 800
        self.smooth = True
        self.lookahead_lines = 2
        adjustment = scroll.get_vadjustment()
        adjustment.connect("changed", self.queue)
        adjustment.connect("value-changed", self._scrolled)
        navigation = Gtk.EventControllerScroll(flags=Gtk.EventControllerScrollFlags.VERTICAL)
        navigation.set_propagation_phase(Gtk.PropagationPhase.CAPTURE)
        navigation.connect("scroll", self._reader_scrolled)
        scroll.add_controller(navigation)
        drag = Gtk.GestureClick()
        drag.set_propagation_phase(Gtk.PropagationPhase.CAPTURE)
        drag.connect("pressed", lambda *_args: self.stop())
        scroll.get_vscrollbar().add_controller(drag)

    def follow(self, *, snap: bool = False) -> None:
        """Resume following, snapping only when opening a different document or capture."""
        self.following = True
        self.snap_next = self.snap_next or snap
        self.queue()

    def queue(self, *_args: object) -> None:
        """Wait for GTK's deferred wrapping before choosing the destination."""
        if self.following and not self.pending:
            self.pending = True
            GLib.idle_add(self._reveal)

    def _write(self, value: float) -> None:
        """Keep animation writes from being mistaken for user navigation."""
        self.writing = True
        self.scroll.get_vadjustment().set_value(value)
        self.writing = False

    def _pause(self) -> None:
        """Leave the viewport where it is when a stream is interrupted or the user scrolls."""
        if self.animation is not None and self.animation.get_state() == Adw.AnimationState.PLAYING:
            self.animation.pause()
        self.animation = None

    def stop(self) -> None:
        """Stop following a hidden or completed live document."""
        self.following = False
        self._pause()

    def _reveal(self) -> bool:
        """Retarget from the current position, preserving intermediate frames during bursts."""
        self.pending = False
        if not self.following:
            return GLib.SOURCE_REMOVE
        self._anticipate_wrap()
        adjustment = self.scroll.get_vadjustment()
        destination = max(0, adjustment.get_upper() - adjustment.get_page_size())
        if (
            not self.snap_next
            and self.smooth
            and self.animation is not None
            and self.animation.get_state() == Adw.AnimationState.PLAYING
            and abs(destination - self.destination) < 1
        ):
            return GLib.SOURCE_REMOVE
        self._pause()
        self.destination = destination
        if (
            self.snap_next
            or not self.smooth
            or not self.scroll.get_mapped()
            or abs(destination - adjustment.get_value()) < 1
        ):
            self._write(destination)
        else:
            self.animation = Adw.TimedAnimation.new(
                self.scroll,
                adjustment.get_value(),
                destination,
                self.duration_ms,
                Adw.CallbackAnimationTarget.new(self._write),
            )
            self.animation.set_easing(Adw.Easing.EASE_OUT_CUBIC)
            self.animation.play()
        self.snap_next = False
        return GLib.SOURCE_REMOVE

    def _anticipate_wrap(self) -> None:
        """Make proportionate room after the last line is 72% full, before its next wrap."""
        view = self.scroll.get_child()
        if not isinstance(view, Gtk.TextView) or not view.get_mapped():
            return
        ending = view.get_iter_location(view.get_buffer().get_end_iter())
        width = max(1, view.get_width() - view.get_left_margin() - view.get_right_margin())
        base = 4 + self.lookahead_lines * 20
        nearing_bottom = ending.y + ending.height + base >= self.scroll.get_vadjustment().get_page_size()
        fill = (ending.x - view.get_left_margin()) / width
        progress = max(0, min(1, (fill - 0.72) / 0.28)) if nearing_bottom else 0
        extra = round(ending.height * min(1.5, self.lookahead_lines * 0.325) * progress)
        if view.get_bottom_margin() != base + extra:
            view.set_bottom_margin(base + extra)

    def _scrolled(self, adjustment: Gtk.Adjustment) -> None:
        """Suspend following when the reader moves away from the newest text."""
        if not self.writing and not self.pending:
            self.following = adjustment.get_value() + adjustment.get_page_size() >= adjustment.get_upper() - 24
            if not self.following:
                self._pause()

    def _reader_scrolled(self, _controller, _dx: float, dy: float) -> bool:
        """Honor an upward scroll even while GTK still has a stream update queued."""
        if dy < 0:
            self.stop()
        return False


class DocumentEditor(MarkdownTextView):
    """Measure the entire editable document for one shared outer scrollbar."""

    def __init__(self, text: str, *, markdown: bool = True) -> None:
        """Keep wrapping, selection and editing in the same visible document."""
        super().__init__(text, markdown=markdown)

    def do_get_request_mode(self):
        """Ask GTK to measure wrapped height using the allocated column width."""
        return Gtk.SizeRequestMode.HEIGHT_FOR_WIDTH

    def do_measure(self, orientation, for_size):
        """Size the editor to its wrapped content, retaining space for the caret."""
        if orientation == Gtk.Orientation.HORIZONTAL:
            return (0, 300, -1, -1)
        height = self.document_height(for_size if for_size > 0 else 300)
        height += self.get_top_margin() + self.get_bottom_margin() + 8
        return (height, height, -1, -1)


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
        self.config = AppConfig()
        self.edit_drafts: dict[tuple[str, int | None], str] = {}
        self.editors: dict[tuple[str, int | None], DocumentEditor] = {}
        self.copy_buttons: list[Gtk.Button] = []
        self.save_buttons: list[Gtk.Button] = []
        self.copy_text = copy_text
        self.request_rewrite = rewrite
        self.paste_text = paste_text
        self.open_archive = open_archive
        self.save_prompt = save_prompt
        self.cancel_rewrite = cancel_rewrite
        self.entry: HistoryEntry | None = None
        self.title_label: Gtk.Label | None = None
        self.busy = False
        self.private = False
        self.drafts: dict[str, str] = {}
        self.result_widgets: list[DocumentEditor] = []
        self.rewrite_preview_identifier: str | None = None
        self.rewrite_preview_text = ""
        self.rewrite_preview_box: Gtk.Box | None = None
        self.rewrite_preview_label: DocumentEditor | None = None
        self.rows: dict[Gtk.ListBoxRow, str] = {}
        self.search_limit = 80
        self.split = Adw.OverlaySplitView(vexpand=True)
        self.split.set_min_sidebar_width(200)
        self.split.set_max_sidebar_width(232)
        self.split.set_sidebar_width_fraction(0.23)
        self.split.set_sidebar(self._build_sidebar())
        self.content = self._build_content()
        self.split.set_content(self.content)
        self.append(self.split)
        self.show_conversation(None, [])
        self.refresh_history()

    def _build_sidebar(self) -> Gtk.Box:
        """Keep the identity small and give history the sidebar's useful space."""
        sidebar = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=SPACE_2)
        sidebar.add_css_class("ml-history-sidebar")
        set_margins(sidebar, SPACE_4)
        brand = Gtk.Box(spacing=SPACE_2)
        self.sidebar_heading = brand
        brand.set_size_request(-1, 32)
        brand.append(brand_mark(20))
        wordmark = Gtk.Label(label="Mluva", xalign=0)
        wordmark.add_css_class("ml-wordmark")
        brand.append(wordmark)
        sidebar.append(brand)
        new = Gtk.Button(label="New conversation")
        new.add_css_class("ml-new-conversation")
        new.set_tooltip_text("Start with dictation or pasted text")
        new.connect("clicked", lambda _button: self.show_conversation(None, []))
        sidebar.append(new)
        self.search = Gtk.SearchEntry(placeholder_text="Search history")
        self.search.connect("search-changed", self._search_changed)
        sidebar.append(self.search)
        self.history_list = Gtk.ListBox(selection_mode=Gtk.SelectionMode.SINGLE)
        self.history_list.add_css_class("navigation-sidebar")
        self.history_list.connect("row-activated", self._open_row)
        self.history_scroll = document_scroll(self.history_list)
        sidebar.append(self.history_scroll)
        self.more = Gtk.Button(label="Show more")
        self.more.connect("clicked", self._show_more)
        sidebar.append(self.more)
        archive = Gtk.Button(label="Manage history", has_frame=False)
        self.archive_button = archive
        archive.set_size_request(-1, 32)
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
        self.heading = Gtk.Box()
        set_margins(self.heading, SPACE_4)
        self.heading.set_margin_bottom(SPACE_2)
        self.heading.set_size_request(-1, 32)
        self.conversation_title = Gtk.Label(xalign=0, hexpand=True, ellipsize=Pango.EllipsizeMode.END)
        self.conversation_title.add_css_class("ml-conversation-title")
        self.heading.append(self.conversation_title)
        self.live_title = Gtk.Label(xalign=0, hexpand=True)
        self.live_title.add_css_class("ml-conversation-title")
        self.live_title.set_visible(False)
        self.heading.append(self.live_title)
        self.live_cancel = Gtk.Button(label="Cancel", has_frame=False, valign=Gtk.Align.CENTER)
        self.live_cancel.set_tooltip_text("Cancel the final rewrite; keep the original dictation")
        self.live_cancel.connect("clicked", lambda _button: self.cancel_rewrite())
        # Reserve the finalization action's height throughout recording so it
        # cannot push either Live panel down when the microphone stops.
        self.live_cancel_slot = Gtk.Stack(hhomogeneous=False, vhomogeneous=True)
        self.live_cancel_slot.add_named(Gtk.Box(), "idle")
        self.live_cancel_slot.add_named(self.live_cancel, "cancel")
        self.live_cancel_slot.set_visible(False)
        self.heading.append(self.live_cancel_slot)
        content.append(self._reading_column(self.heading))
        self.messages = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=16)
        set_margins(self.messages, SPACE_4)
        self.messages.set_margin_top(0)
        reading_width = self._reading_column(self.messages)
        self.scroll = document_scroll(reading_width)
        self.conversation_follower = _TailFollower(self.scroll)
        content.append(self.scroll)

        self.live_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=24, homogeneous=True, vexpand=True)
        self.live_box.add_css_class("ml-live")
        set_margins(self.live_box, SPACE_4)
        self.live_box.set_margin_top(0)
        self.live_source_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=SPACE_2, hexpand=True)
        self.live_source_box.append(Gtk.Label(label="Dictation", xalign=0, css_classes=["heading"]))
        self.live_text = self._text_view("")
        self.live_text.update_property([Gtk.AccessibleProperty.LABEL], ["Dictation"])
        self.live_scroll = document_scroll(self.live_text)
        self.live_follower = _TailFollower(self.live_scroll)
        self.live_source_box.append(self.live_scroll)
        self.live_box.append(self.live_source_box)
        self.live_draft_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=SPACE_2, hexpand=True)
        self.live_draft_status = Gtk.Label(
            label="Live draft", xalign=0, ellipsize=Pango.EllipsizeMode.END, css_classes=["heading"]
        )
        self.live_draft_status.connect("notify::label", lambda label, _spec: label.set_tooltip_text(label.get_label()))
        self.live_draft_box.append(self.live_draft_status)
        self.live_draft_text = MarkdownTextView()
        self.live_draft_text.update_property([Gtk.AccessibleProperty.LABEL], ["Live draft"])
        self.live_draft_scroll = document_scroll(self.live_draft_text)
        self.live_draft_box.append(self.live_draft_scroll)
        self.live_draft_follower = _TailFollower(self.live_draft_scroll)
        draft_focus = Gtk.EventControllerFocus()
        draft_focus.connect("enter", lambda _controller: self.live_draft_follower.stop())
        self.live_draft_text.add_controller(draft_focus)
        self.live_box.append(self.live_draft_box)
        self.live_draft_box.set_visible(False)
        self.live_box.set_visible(False)
        self.live_column = self._reading_column(self.live_box)
        self.live_column.set_visible(False)
        content.append(self.live_column)

        composer = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=SPACE_2)
        self.composer = composer
        composer.add_css_class("ml-composer")
        set_margins(composer, SPACE_4)
        composer.set_margin_top(SPACE_2)
        composer.set_margin_bottom(SPACE_2)
        self.actions = Gtk.FlowBox(
            selection_mode=Gtk.SelectionMode.NONE,
            column_spacing=SPACE_2,
            row_spacing=SPACE_2,
            max_children_per_line=4,
            min_children_per_line=1,
        )
        self.actions.set_halign(Gtk.Align.START)
        self.quick_polish = Gtk.Button(label="Polish")
        self.quick_polish.set_tooltip_text("Remove filler words and fix phrasing while keeping your meaning")
        self.quick_polish.connect("clicked", lambda _button: self.request_rewrite(QUICK_POLISH))
        self.actions.insert(self.quick_polish, -1)
        self.structured_note = Gtk.Button(label="Structure")
        self.structured_note.set_tooltip_text("A concise summary followed by organized bullet points")
        self.structured_note.connect("clicked", lambda _button: self.request_rewrite(STRUCTURED_NOTE))
        self.actions.insert(self.structured_note, -1)
        self.saved_prompts = Gtk.MenuButton(label="More")
        self.actions.insert(self.saved_prompts, -1)
        composer.append(self.actions)

        self.prompt_label = Gtk.Label(xalign=0)
        self.prompt_label.add_css_class("caption")
        composer.append(self.prompt_label)
        self.prompt = Gtk.TextView(wrap_mode=Gtk.WrapMode.WORD_CHAR, accepts_tab=False)
        self.prompt.add_css_class("ml-prompt")
        self.prompt.set_tooltip_text("Write a custom instruction, then press Ctrl+Enter to send")
        prompt_scroll = document_scroll(self.prompt, vexpand=False)
        prompt_scroll.set_min_content_height(40)
        prompt_scroll.set_max_content_height(120)
        prompt_scroll.set_propagate_natural_height(True)
        prompt_overlay = Gtk.Overlay(child=prompt_scroll)
        self.prompt_placeholder = Gtk.Label(
            label="Ask for a rewrite…",
            halign=Gtk.Align.START,
            valign=Gtk.Align.CENTER,
            margin_start=10,
            can_target=False,
        )
        self.prompt_placeholder.add_css_class("dim-label")
        prompt_overlay.add_overlay(self.prompt_placeholder)
        self.prompt.get_buffer().connect(
            "changed", lambda buffer: self.prompt_placeholder.set_visible(buffer.get_char_count() == 0)
        )
        composer.append(prompt_overlay)
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
        self.composer_column = self._reading_column(composer)
        content.append(self.composer_column)
        return content

    @staticmethod
    def _reading_column(child: Gtk.Widget) -> Adw.Clamp:
        """Keep the heading, text, composer and recording controls on the same horizontal guides."""
        return Adw.Clamp(maximum_size=900, tightening_threshold=900, child=child)

    def set_capture_controls(self, controls: Gtk.Widget) -> None:
        """Keep the recording footer inside the conversation while history reaches the window bottom."""
        self.content.append(self._reading_column(controls))

    def set_rewrite_settings(self, settings: Gtk.Widget) -> None:
        """Keep model and speed choices beside the operations they affect."""
        self.actions.insert(settings, -1)

    @staticmethod
    def _text_view(text: str) -> Gtk.TextView:
        """Display complete selectable text without ellipses or a fixed line limit."""
        return MarkdownTextView(text, markdown=False, editable=False)

    def set_config(self, config: AppConfig) -> None:
        """Apply the same persisted behavior to all document and live surfaces."""
        self.config = config
        for follower in (self.conversation_follower, self.live_follower, self.live_draft_follower):
            follower.smooth = config.smooth_scrolling
            follower.duration_ms = config.scroll_duration_ms
            follower.lookahead_lines = config.scroll_lookahead_lines
        self.messages.set_margin_bottom(16 + config.scroll_lookahead_lines * 20)
        for view in (self.live_text, self.live_draft_text):
            view.set_bottom_margin(4 + config.scroll_lookahead_lines * 20)
        for button in self.copy_buttons:
            button.set_visible(config.show_copy_action)
        for button in self.save_buttons:
            button.set_visible(config.show_save_action)

    def _message(self, title: str, text: str, source: bool = False, reply_identifier: int | None = None) -> None:
        """Add a directly editable document with compact Copy and Save actions."""
        message = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=SPACE_2)
        message.add_css_class("ml-source" if source else "ml-reply")
        header = Gtk.Box(spacing=SPACE_2)
        heading = Gtk.Label(label=title, xalign=0, hexpand=True)
        heading.add_css_class("heading")
        header.append(heading)
        key = (self.entry.identifier, reply_identifier) if self.entry is not None else None
        view = DocumentEditor(self.edit_drafts.get(key, text), markdown=not source)
        self.result_widgets.append(view)
        save = Gtk.Button(icon_name="document-save-symbolic", has_frame=False, tooltip_text="Save edits")
        save.set_visible(self.config.show_save_action)
        save.set_sensitive(False)
        self.save_buttons.append(save)
        if key is not None:
            self.editors[key] = view

            def changed(_buffer):
                """Retain edits across navigation without changing stored recognition."""
                self.edit_drafts[key] = view.get_text()
                save.set_sensitive(view.get_text() != text)

            view.get_buffer().connect("changed", changed)
            save.connect("clicked", lambda _button: self.save_edits(key))
            save.set_sensitive(key in self.edit_drafts and self.edit_drafts[key] != text)
        header.append(save)
        copy = Gtk.Button(icon_name="edit-copy-symbolic", has_frame=False, tooltip_text=f"Copy {title.lower()}")
        copy.set_visible(self.config.show_copy_action)
        copy.connect("clicked", lambda _button: self.copy_text(view.get_text()))
        self.copy_buttons.append(copy)
        header.append(copy)
        message.append(header)
        message.append(view)
        keys = Gtk.EventControllerKey()
        keys.connect("key-pressed", self._document_key)
        view.add_controller(keys)
        self.messages.append(message)

    def _document_key(self, _controller, key, _code, state):
        """Keep explicit saving available from the keyboard when toolbar icons are hidden."""
        if key == Gdk.KEY_s and state & Gdk.ModifierType.CONTROL_MASK:
            self.save_edits()
            return True
        return False

    def save_edits(self, key: tuple[str, int | None] | None = None) -> bool:
        """Save explicit edits, also called before rewriting so the new text is authoritative."""
        keys = (
            [key]
            if key is not None
            else [key for key in self.edit_drafts if self.entry and key[0] == self.entry.identifier]
        )
        try:
            for selected in keys:
                if selected in self.edit_drafts:
                    self.store.save_text(selected[0], self.edit_drafts[selected], selected[1])
                    del self.edit_drafts[selected]
        except Exception:
            self.notice.set_label("Could not save edits. Keep the document open and try again.")
            return False
        self.notice.set_label("Edits saved")
        self.refresh_history()
        return True

    def can_copy_current_output(self) -> bool:
        """Offer explicit copying only when a complete, visible document exists."""
        return bool(self.result_widgets) and not self.live_box.get_visible()

    def copy_current_output(self) -> bool:
        """Copy the latest complete document with current edits and its original Markdown."""
        if not self.can_copy_current_output():
            return False
        self.copy_text(self.result_widgets[-1].get_text())
        return True

    def focus_prompt(self) -> None:
        """Move keyboard focus to the visible source or rewrite composer."""
        if self.composer.get_visible():
            self.prompt.grab_focus()

    def live_draft(self) -> str:
        """Read the structured live draft, including the speaker's manual edits."""
        return self.live_draft_text.get_text()

    def show_live_draft(self, text: str, status: str = "Live draft") -> None:
        """Replace a completed snapshot without rendering a partially restructured document."""
        follower = self.live_draft_follower
        following = follower.following
        position = self.live_draft_scroll.get_vadjustment().get_value()
        self.live_draft_box.set_visible(True)
        follower.writing = True
        self.live_draft_text.replace_text(text)
        follower.writing = False
        self.live_draft_status.set_label(status)
        if following:
            follower.follow()
        else:
            GLib.idle_add(lambda: follower._write(position))

    def show_conversation(self, entry: HistoryEntry | None, replies: list[Rewrite]) -> None:
        """Open the exact selected history item and every saved rewrite, preserving original text."""
        self.drafts[self.entry.identifier if self.entry else "new"] = self.prompt_text()
        same_entry = self.entry is not None and entry is not None and self.entry.identifier == entry.identifier
        position = self.scroll.get_vadjustment().get_value()
        following = self.conversation_follower.following
        self.editors.clear()
        self.copy_buttons.clear()
        self.save_buttons.clear()
        self.entry = entry
        self.title_label = None
        self.conversation_title.set_label("New conversation")
        self.conversation_title.set_tooltip_text(None)
        self.result_widgets.clear()
        self.rewrite_preview_box = None
        self.rewrite_preview_label = None
        child = self.messages.get_first_child()
        while child is not None:
            self.messages.remove(child)
            child = self.messages.get_first_child()
        if entry is None:
            empty = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12, valign=Gtk.Align.START)
            empty.add_css_class("ml-empty")
            mark = brand_mark(40)
            mark.set_halign(Gtk.Align.START)
            empty.append(mark)
            empty.append(Gtk.Label(label="Your words, ready to use", xalign=0, css_classes=["title-2"]))
            empty.append(
                Gtk.Label(
                    label="Press F9 to dictate. Or paste text below.\nPolish it, structure it, make it yours.",
                    xalign=0,
                    wrap=True,
                    css_classes=["dim-label"],
                )
            )
            self.messages.append(empty)
        else:
            self.title_label = self.conversation_title
            title = entry.title or fallback_title(entry.raw_text)
            self.title_label.set_label(title)
            self.title_label.set_tooltip_text(title)
            self._message("Original", self.store.source_text(entry), source=True)
            if entry.delivered_text and entry.delivered_text != entry.raw_text:
                # Prepared text remains recoverable in History; the workspace edits its source directly.
                pass
            for reply in replies:
                instruction = (
                    "Quick Polish"
                    if reply.instruction == QUICK_POLISH
                    else "Structured Note"
                    if reply.instruction == STRUCTURED_NOTE
                    else reply.instruction
                )
                request = Gtk.Label(label=instruction, xalign=1, wrap=True, selectable=True)
                request.set_halign(Gtk.Align.END)
                request.set_max_width_chars(64)
                request.add_css_class("ml-instruction")
                self.messages.append(request)
                self._message("Rewrite", reply.text, reply_identifier=reply.identifier)
            self._render_rewrite_preview()
        self.prompt.get_buffer().set_text(self.drafts.get(entry.identifier if entry else "new", ""))
        self.prompt_label.set_label(
            "Ask for a rewrite or a follow-up" if entry else "Paste or type text to get started"
        )
        self.prompt_label.set_visible(False)
        self.prompt_placeholder.set_label("Ask for a rewrite…" if entry else "Paste or type text to start…")
        self.send.set_label("Rewrite" if entry else "Start conversation")
        self.notice.set_label(
            "Edit either document. Save edits or rewrite to keep them." if entry else "Dictation copies automatically."
        )
        self._update_actions()
        for row, identifier in self.rows.items():
            if entry is not None and identifier == entry.identifier:
                self.history_list.select_row(row)
        if self.split.get_collapsed():
            self.split.set_show_sidebar(False)
        if not same_entry:
            self.scroll_to_latest()
        elif following:
            self.conversation_follower.follow()
        else:
            self.conversation_follower.stop()
            GLib.idle_add(lambda: self.conversation_follower._write(position))

    def set_rewrite_preview(self, identifier: str, text: str) -> None:
        """Retain one volatile stream across navigation, with no partial Copy or persistence."""
        self.rewrite_preview_identifier = identifier
        self.rewrite_preview_text = text
        self._render_rewrite_preview()

    def _render_rewrite_preview(self) -> None:
        """Update only the selected note's streaming reply without rebuilding its history."""
        if self.entry is None or self.entry.identifier != self.rewrite_preview_identifier or self.private:
            return
        if self.rewrite_preview_box is None:
            self.rewrite_preview_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=SPACE_2)
            self.rewrite_preview_box.add_css_class("ml-reply")
            self.rewrite_preview_box.append(Gtk.Label(label="Rewriting…", xalign=0, css_classes=["heading"]))
            self.rewrite_preview_label = DocumentEditor("")
            self.rewrite_preview_label.set_editable(False)
            self.rewrite_preview_label.set_cursor_visible(False)
            self.rewrite_preview_label.set_focusable(False)
            self.rewrite_preview_label.set_can_target(False)
            self.rewrite_preview_box.append(self.rewrite_preview_label)
            self.messages.append(self.rewrite_preview_box)
        self.rewrite_preview_label.replace_text(self.rewrite_preview_text)
        self.conversation_follower.queue()

    def clear_rewrite_preview(self) -> None:
        """Erase partial text on completion, failure, cancellation or privacy changes."""
        self.rewrite_preview_identifier = None
        self.rewrite_preview_text = ""
        if self.rewrite_preview_box is not None:
            self.messages.remove(self.rewrite_preview_box)
        self.rewrite_preview_box = None
        self.rewrite_preview_label = None

    def scroll_to_latest(self) -> None:
        """Follow the conversation's outer viewport through GTK's deferred text measurement."""
        self.conversation_follower.follow(snap=True)

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
            set_margins(box, 6)
            title = Gtk.Label(
                label=entry.title or fallback_title(entry.raw_text),
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

    def refresh_title(self, identifier: str) -> None:
        """Keep the reading position and active draft when an asynchronous label arrives."""
        entry = self.store.history.find(identifier)
        if self.entry is not None and self.entry.identifier == identifier:
            self.entry = entry
            if self.title_label is not None:
                title = self.entry.title or fallback_title(self.entry.raw_text)
                self.title_label.set_label(title)
                self.title_label.set_tooltip_text(title)
        if self.search.get_text():
            self.refresh_history()
        else:
            for row, row_identifier in self.rows.items():
                if row_identifier == identifier:
                    row.get_child().get_first_child().set_label(entry.title or fallback_title(entry.raw_text))

    def show_transient(self, original: str, text: str) -> None:
        """Show an unsaved Incognito result with explicit copying and no durable source."""
        self.show_conversation(None, [])
        self.messages.remove(self.messages.get_first_child())
        self._message("Original", original, source=True)
        if text != original:
            self._message("Dictation", text)
        self.notice.set_label("This conversation is not saved.")
        self.scroll_to_latest()

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
        for editor in self.editors.values():
            editor.set_editable(not self.busy)
        self.cancel.set_visible(self.busy)
        self.live_cancel_slot.set_visible_child_name("cancel" if self.busy else "idle")
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
            self.clear_rewrite_preview()
        self._update_actions()

    def set_live(self, phase: str, text: str) -> None:
        """Show every live word separately from committed, copyable messages."""
        starting = not self.live_box.get_visible()
        self.live_box.set_visible(True)
        self.live_column.set_visible(True)
        self.scroll.set_visible(False)
        self.composer.set_visible(False)
        self.composer_column.set_visible(False)
        self.conversation_title.set_visible(False)
        self.live_title.set_visible(True)
        self.live_cancel_slot.set_visible(True)
        self.live_title.set_label(phase)
        self.live_text.replace_text(text)
        if starting:
            self.live_follower.follow(snap=True)
        else:
            self.live_follower.queue()

    def finish_live(self) -> None:
        """Erase volatile text when finalization completes or capture is cancelled."""
        self.live_follower.stop()
        self.live_draft_follower.stop()
        self.live_draft_box.set_visible(False)
        self.live_text.get_buffer().set_text("")
        self.live_box.set_visible(False)
        self.live_column.set_visible(False)
        self.live_title.set_visible(False)
        self.live_cancel_slot.set_visible(False)
        self.conversation_title.set_visible(True)
        self.scroll.set_visible(True)
        self.composer.set_visible(True)
        self.composer_column.set_visible(True)

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
