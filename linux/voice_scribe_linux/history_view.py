"""GTK history management surface for Linux transcription recovery."""

from collections.abc import Callable
from datetime import datetime
from pathlib import Path

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, Gio, GLib, Gtk  # noqa: E402

from voice_scribe_linux.history import (
    RECOGNITION_FALLBACK_STARTUP_FAILED,
    RECOGNITION_FALLBACK_STREAM_FAILED,
    RECOGNITION_FALLBACK_UNAVAILABLE,
    RECOGNITION_ROUTE_BATCH,
    RECOGNITION_ROUTE_BATCH_RETRY,
    RECOGNITION_ROUTE_REALTIME,
    HistoryEntry,
    HistoryStore,
)
from voice_scribe_linux.ui import (
    SPACE_2,
    SPACE_3,
    FeatureMaturityNotice,
    clamp,
    empty_state,
    page_content,
    set_margins,
)

RECOGNITION_ROUTE_LABELS = {
    RECOGNITION_ROUTE_REALTIME: "Scribe v2 Realtime",
    RECOGNITION_ROUTE_BATCH: "Scribe v2 batch",
    RECOGNITION_ROUTE_BATCH_RETRY: "Scribe v2 batch retry",
    None: "Legacy/unknown route",
}
RECOGNITION_FALLBACK_LABELS = {
    RECOGNITION_FALLBACK_UNAVAILABLE: "Realtime was unavailable before capture",
    RECOGNITION_FALLBACK_STARTUP_FAILED: "Realtime session startup failed",
    RECOGNITION_FALLBACK_STREAM_FAILED: "Realtime stream did not produce committed text",
}
ENHANCEMENT_CONTEXT_LABELS = {
    "selected-text": "explicit selected text",
    "style-instructions": "saved style instructions",
}
ENHANCEMENT_OUTCOME_LABELS = {
    "completed": "completed",
    "raw-fallback": "raw fallback",
    "safe-fallback": "partial safe fallback",
    "failed": "failed",
}


class HistoryPage(Gtk.Box):
    """Render recoverable history with details disclosed only on demand."""

    def __init__(
        self,
        store: HistoryStore,
        export_directory: Path,
        copy_text: Callable[[str], None],
        can_retry_delivery: Callable[[HistoryEntry], bool],
        retry_delivery: Callable[[HistoryEntry], None],
        retry_recognition: Callable[[HistoryEntry], None],
        reprocess_entry: Callable[[HistoryEntry], None],
        delete_entry: Callable[[HistoryEntry], bool],
        history_changed: Callable[[], None],
        show_message: Callable[[str], None],
    ) -> None:
        """Build one refreshable archive around injected application actions."""
        super().__init__(orientation=Gtk.Orientation.VERTICAL)
        self.store = store
        self.export_directory = export_directory
        self.copy_text = copy_text
        self.can_retry_delivery = can_retry_delivery
        self.retry_delivery = retry_delivery
        self.retry_recognition = retry_recognition
        self.reprocess_entry = reprocess_entry
        self.delete_entry = delete_entry
        self.history_changed = history_changed
        self.show_message = show_message
        self.entry_rows: dict[str, Adw.ExpanderRow] = {}

        content = page_content()
        content.append(FeatureMaturityNotice("history"))
        archive_heading = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=SPACE_2)
        archive_title = Gtk.Label(label="Recent transcriptions", xalign=0, hexpand=True)
        archive_title.add_css_class("title-2")
        archive_heading.append(archive_title)
        self.count_label = Gtk.Label(xalign=1)
        self.count_label.add_css_class("dim-label")
        archive_heading.append(self.count_label)
        content.append(archive_heading)

        self.archive_stack = Gtk.Stack(transition_type=Gtk.StackTransitionType.CROSSFADE)
        self.empty_page = empty_state(
            "No transcriptions yet",
            "Completed non-Incognito captures will remain recoverable here.",
            "document-open-recent-symbolic",
        )
        self.archive_stack.add_named(self.empty_page, "empty")
        self.list_box = Gtk.ListBox(selection_mode=Gtk.SelectionMode.NONE)
        self.list_box.add_css_class("boxed-list")
        self.archive_stack.add_named(self.list_box, "entries")
        self.archive_stack.set_vexpand(True)
        content.append(self.archive_stack)

        self.scroll = Gtk.ScrolledWindow(
            vexpand=True,
            hscrollbar_policy=Gtk.PolicyType.NEVER,
            vscrollbar_policy=Gtk.PolicyType.AUTOMATIC,
        )
        self.scroll.set_child(clamp(content))
        self.append(self.scroll)
        self.refresh()

    def refresh(self) -> None:
        """Rebuild durable rows while preserving open records and scroll position."""
        expanded = {identifier for identifier, row in self.entry_rows.items() if row.get_expanded()}
        scroll_position = self.scroll.get_vadjustment().get_value()
        self._clear_list(self.list_box)
        self.entry_rows.clear()
        entries = self.store.recent()
        self.count_label.set_label(f"{len(entries)} saved")
        if not entries:
            self.archive_stack.set_visible_child_name("empty")
        else:
            self.archive_stack.set_visible_child_name("entries")
            for entry in entries:
                row = self._build_entry(entry)
                row.set_expanded(entry.identifier in expanded)
                self.entry_rows[entry.identifier] = row
                self.list_box.append(row)
        GLib.idle_add(self._restore_scroll_position, scroll_position)

    def _restore_scroll_position(self, position: float) -> bool:
        """Restore the bounded archive position after GTK measures rebuilt rows."""
        adjustment = self.scroll.get_vadjustment()
        maximum = max(adjustment.get_lower(), adjustment.get_upper() - adjustment.get_page_size())
        adjustment.set_value(min(position, maximum))
        return GLib.SOURCE_REMOVE

    def _build_entry(self, entry: HistoryEntry) -> Adw.ExpanderRow:
        """Build one concise result row with editing and technical details nested below it."""
        preview = " ".join((entry.delivered_text or entry.raw_text).split())
        row = Adw.ExpanderRow(
            title=entry.title or preview[:80] or "Empty transcript",
            subtitle=self._entry_subtitle(entry),
        )

        title_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=SPACE_2)
        set_margins(title_box, SPACE_3)
        title_entry = Gtk.Entry(hexpand=True, placeholder_text="Optional title")
        title_entry.set_text(entry.title or "")
        title_box.append(title_entry)
        save_title = Gtk.Button(label="Save title")
        save_title.connect("clicked", self._save_title, entry, title_entry)
        title_box.append(save_title)
        row.add_row(title_box)

        delivered_row = Adw.ActionRow(
            title="Final text",
            subtitle=entry.delivered_text or "No final text is available",
        )
        delivered_row.set_subtitle_lines(3)
        copy_delivered = Gtk.Button(label="Copy", valign=Gtk.Align.CENTER)
        copy_delivered.set_sensitive(bool(entry.delivered_text))
        copy_delivered.connect("clicked", self._copy_delivered, entry)
        delivered_row.add_suffix(copy_delivered)
        paste_again = Gtk.Button(label="Paste again", valign=Gtk.Align.CENTER)
        paste_again.set_sensitive(self.can_retry_delivery(entry))
        paste_again.set_tooltip_text("Available only while this Mluva process still owns the exact accessible target.")
        paste_again.connect("clicked", self._retry_delivery, entry)
        delivered_row.add_suffix(paste_again)
        row.add_row(delivered_row)

        if entry.delivered_text:
            row.add_row(self._correction_editor(entry))

        row.add_row(self._technical_details(entry))

        actions = Gtk.FlowBox(selection_mode=Gtk.SelectionMode.NONE, row_spacing=SPACE_2, column_spacing=SPACE_2)
        actions.set_max_children_per_line(3)
        set_margins(actions, SPACE_3)
        restore = Gtk.Button(label="Restore raw")
        restore.set_sensitive(bool(entry.raw_text) and entry.raw_text != entry.delivered_text)
        restore.connect("clicked", self._restore_raw, entry)
        actions.append(restore)
        retry_recognition = Gtk.Button(label="Retry transcription")
        retry_recognition.set_visible(entry.retained_audio_path is not None and not entry.raw_text)
        retry_recognition.connect("clicked", self._retry_recognition, entry)
        actions.append(retry_recognition)
        reprocess = Gtk.Button(label="Reprocess raw")
        reprocess.set_sensitive(bool(entry.raw_text))
        reprocess.set_tooltip_text(
            "Apply current spoken-structure, dictionary, and snippet rules locally without Codex."
        )
        reprocess.connect("clicked", self._reprocess, entry)
        actions.append(reprocess)
        export_markdown = Gtk.Button(label="Export Markdown")
        export_markdown.connect("clicked", self._export, entry, "markdown")
        actions.append(export_markdown)
        export_json = Gtk.Button(label="Export JSON")
        export_json.connect("clicked", self._export, entry, "json")
        actions.append(export_json)
        delete = Gtk.Button(label="Delete")
        delete.add_css_class("destructive-action")
        delete.connect("clicked", self._confirm_delete, entry)
        actions.append(delete)
        row.add_row(actions)
        return row

    def _correction_editor(self, entry: HistoryEntry) -> Gtk.Box:
        """Build a labelled multiline correction surface without nested row focus chrome."""
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=SPACE_2)
        set_margins(box, SPACE_3)
        label = Gtk.Label(label="_Correct final text", xalign=0, use_underline=True)
        label.add_css_class("heading")
        box.append(label)
        explanation = Gtk.Label(
            label=(
                "Small saved edits can become review-only vocabulary suggestions; nothing is learned automatically."
            ),
            xalign=0,
            wrap=True,
        )
        explanation.add_css_class("caption")
        explanation.add_css_class("dim-label")
        box.append(explanation)
        correction_view = Gtk.TextView(wrap_mode=Gtk.WrapMode.WORD_CHAR)
        correction_view.get_buffer().set_text(entry.delivered_text)
        label.set_mnemonic_widget(correction_view)
        correction_scroll = Gtk.ScrolledWindow(min_content_height=110)
        correction_scroll.set_child(correction_view)
        box.append(correction_scroll)
        save_correction = Gtk.Button(label="Save correction", halign=Gtk.Align.END)
        save_correction.add_css_class("suggested-action")
        save_correction.connect("clicked", self._save_correction, entry, correction_view)
        box.append(save_correction)
        return box

    def _technical_details(self, entry: HistoryEntry) -> Adw.ExpanderRow:
        """Hide raw provider and timing metadata behind a secondary disclosure."""
        details = Adw.ExpanderRow(
            title="Technical details",
            subtitle="Raw transcript, recognition route, processing context, and timings",
        )
        raw_row = Adw.ActionRow(title="Raw transcript", subtitle=entry.raw_text or "No raw transcript")
        raw_row.set_subtitle_lines(3)
        copy_raw = Gtk.Button(label="Copy raw", valign=Gtk.Align.CENTER)
        copy_raw.set_sensitive(bool(entry.raw_text))
        copy_raw.connect("clicked", self._copy_raw, entry)
        raw_row.add_suffix(copy_raw)
        details.add_row(raw_row)
        if entry.application_identifier is not None:
            details.add_row(Adw.ActionRow(title="Captured application scope", subtitle=entry.application_identifier))
        details.add_row(
            Adw.ActionRow(
                title="Recognition route",
                subtitle=RECOGNITION_ROUTE_LABELS.get(entry.recognition_route, "Unknown controlled route"),
            )
        )
        if entry.recognition_fallback_reason is not None:
            details.add_row(
                Adw.ActionRow(
                    title="Realtime fallback",
                    subtitle=RECOGNITION_FALLBACK_LABELS.get(
                        entry.recognition_fallback_reason,
                        "Unknown controlled fallback",
                    ),
                )
            )
        if entry.enhancement_provider_id is not None:
            details.add_row(
                Adw.ActionRow(
                    title="Enhancement provider",
                    subtitle=(
                        f"Codex app-server · {entry.enhancement_model_identifier} · "
                        f"{ENHANCEMENT_OUTCOME_LABELS.get(entry.enhancement_outcome, 'unknown outcome')}"
                    ),
                )
            )
            details.add_row(
                Adw.ActionRow(
                    title="Disclosed enhancement context",
                    subtitle=(
                        ", ".join(
                            ENHANCEMENT_CONTEXT_LABELS.get(source, "unknown controlled context")
                            for source in entry.enhancement_context_sources
                        )
                        or "None"
                    ),
                )
            )
        if any(value is not None for value in (entry.recognition_ms, entry.enhancement_ms, entry.delivery_ms)):
            details.add_row(Adw.ActionRow(title="Stage timings", subtitle=self._timing_subtitle(entry)))
        return details

    def _entry_subtitle(self, entry: HistoryEntry) -> str:
        """Summarize the user-relevant state and local creation time."""
        try:
            created = datetime.fromisoformat(entry.created_at).astimezone().strftime("%Y-%m-%d %H:%M")
        except ValueError:
            created = entry.created_at
        retained = " · recovery audio" if entry.retained_audio_path is not None else ""
        return f"{entry.mode.title()} · {entry.delivery_outcome}{retained} · {created}"

    @staticmethod
    def _timing_subtitle(entry: HistoryEntry) -> str:
        """Render only stage timings actually recorded for this History entry."""
        timings = (
            ("recognition", entry.recognition_ms),
            ("enhancement", entry.enhancement_ms),
            ("delivery", entry.delivery_ms),
        )
        return " · ".join(f"{label} {value} ms" for label, value in timings if value is not None)

    def _save_title(self, _button: Gtk.Button, entry: HistoryEntry, title_entry: Gtk.Entry) -> None:
        """Persist a trimmed optional title and refresh the collapsed row label."""
        title = title_entry.get_text().strip() or None
        try:
            self.store.update_title(entry.identifier, title)
        except Exception as error:
            self.show_message(f"History title could not be saved: {error}")
            return
        self.refresh()
        self.show_message("History title saved.")

    def _save_correction(
        self,
        _button: Gtk.Button,
        entry: HistoryEntry,
        correction_view: Gtk.TextView,
    ) -> None:
        """Persist one explicit correction without delivering or learning automatically."""
        buffer = correction_view.get_buffer()
        start, end = buffer.get_bounds()
        corrected_text = buffer.get_text(start, end, include_hidden_chars=True).strip()
        if corrected_text == entry.delivered_text:
            self.show_message("The final text is unchanged.")
            return
        try:
            self.store.correct_delivered_text(entry.identifier, corrected_text)
        except Exception as error:
            self.show_message(f"Correction could not be saved: {error}")
            return
        self.refresh()
        self.history_changed()
        self.show_message("Correction saved locally. Review derived suggestions in Personalization.")

    def _copy_delivered(self, _button: Gtk.Button, entry: HistoryEntry) -> None:
        """Copy the selected final form without attempting delivery."""
        self.copy_text(entry.delivered_text)

    def _retry_delivery(self, _button: Gtk.Button, entry: HistoryEntry) -> None:
        """Request a paste only when the application advertised an exact retained target."""
        self.retry_delivery(entry)

    def _retry_recognition(self, _button: Gtk.Button, entry: HistoryEntry) -> None:
        """Request safe background re-transcription for managed failure audio."""
        self.retry_recognition(entry)

    def _reprocess(self, _button: Gtk.Button, entry: HistoryEntry) -> None:
        """Reapply current deterministic local rules to immutable recognition."""
        self.reprocess_entry(entry)

    def _copy_raw(self, _button: Gtk.Button, entry: HistoryEntry) -> None:
        """Copy immutable recognition for manual recovery or comparison."""
        self.copy_text(entry.raw_text)

    def _restore_raw(self, _button: Gtk.Button, entry: HistoryEntry) -> None:
        """Make raw recognition the selected final form on explicit request."""
        try:
            self.store.restore_raw(entry.identifier)
        except Exception as error:
            self.show_message(f"Raw transcript could not be restored: {error}")
            return
        self.refresh()
        self.history_changed()
        self.show_message("Raw transcript restored in history.")

    def _export(self, _button: Gtk.Button, entry: HistoryEntry, export_format: str) -> None:
        """Export one entry and reveal its exact owner-local path."""
        try:
            output_path = self.store.export(entry, self.export_directory, export_format)
        except Exception as error:
            self.show_message(f"History export failed: {error}")
            return
        self.show_message(f"Exported to {output_path}")

    def _confirm_delete(self, _button: Gtk.Button, entry: HistoryEntry) -> None:
        """Require confirmation before erasing a transcript and possible recovery audio."""
        dialog = Adw.AlertDialog.new(
            "Delete this history entry?",
            "Its transcript and retained recovery audio will be permanently deleted.",
        )
        dialog.add_response("cancel", "Cancel")
        dialog.add_response("delete", "Delete permanently")
        dialog.set_default_response("cancel")
        dialog.set_close_response("cancel")
        dialog.set_response_appearance("delete", Adw.ResponseAppearance.DESTRUCTIVE)
        dialog.choose(self, None, self._delete_chosen, entry)

    def _delete_chosen(
        self,
        dialog: Adw.AlertDialog,
        result: Gio.AsyncResult,
        entry: HistoryEntry,
    ) -> None:
        """Apply a confirmed history deletion and refresh dependent personalization."""
        if dialog.choose_finish(result) != "delete" or not self.delete_entry(entry):
            return
        self.refresh()
        self.history_changed()
        self.show_message("History entry and retained recovery audio permanently deleted.")

    @staticmethod
    def _clear_list(list_box: Gtk.ListBox) -> None:
        """Remove every current child without relying on stale row indexes."""
        child = list_box.get_first_child()
        while child is not None:
            next_child = child.get_next_sibling()
            list_box.remove(child)
            child = next_child
