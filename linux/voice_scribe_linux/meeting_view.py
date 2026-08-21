"""GTK review surface for explicit Meeting capture and its separate archive."""

from collections.abc import Callable
from datetime import datetime
from pathlib import Path

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, Gio, GLib, Gtk  # noqa: E402

from voice_scribe_linux.meeting import MeetingRecognitionStatus, MeetingRecord, MeetingStore
from voice_scribe_linux.ui import (
    PRIMARY_ACTION_HEIGHT,
    SPACE_1,
    SPACE_2,
    SPACE_3,
    FeatureMaturityNotice,
    SummaryRow,
    card_box,
    clamp,
    empty_state,
    page_content,
    set_button_content,
    set_margins,
    summary_list,
)


class MeetingPage(Gtk.Box):
    """Render explicit Meeting capture with a sticky action and a private archive."""

    def __init__(
        self,
        store: MeetingStore,
        export_directory: Path,
        toggle_capture: Callable[[], None],
        copy_text: Callable[[str], None],
        retry_recognition: Callable[[MeetingRecord], None],
        delete_meeting: Callable[[MeetingRecord], bool],
        show_message: Callable[[str], None],
    ) -> None:
        """Build the Meeting-only control and archive around injected application actions."""
        super().__init__(orientation=Gtk.Orientation.VERTICAL)
        self.store = store
        self.export_directory = export_directory
        self.toggle_capture = toggle_capture
        self.copy_text = copy_text
        self.retry_recognition = retry_recognition
        self.delete_meeting = delete_meeting
        self.show_message = show_message
        self.meeting_rows: dict[str, Adw.ExpanderRow] = {}

        content = page_content()
        content.append(FeatureMaturityNotice("meeting_mode"))
        content.append(self._build_capture_card())
        archive_heading = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=SPACE_2)
        archive_title = Gtk.Label(label="Meeting archive", xalign=0, hexpand=True)
        archive_title.add_css_class("title-2")
        archive_heading.append(archive_title)
        self.count_label = Gtk.Label(xalign=1)
        self.count_label.add_css_class("dim-label")
        archive_heading.append(self.count_label)
        content.append(archive_heading)

        self.archive_stack = Gtk.Stack(transition_type=Gtk.StackTransitionType.CROSSFADE)
        self.empty_page = Gtk.ListBox(selection_mode=Gtk.SelectionMode.NONE)
        self.empty_page.add_css_class("boxed-list")
        empty_row = Adw.ActionRow(
            title="No saved meetings yet",
            subtitle="Only explicit non-Incognito Meeting captures appear here.",
        )
        empty_icon = Gtk.Image.new_from_icon_name("system-users-symbolic")
        empty_icon.set_pixel_size(32)
        empty_row.add_prefix(empty_icon)
        self.empty_page.append(empty_row)
        empty_wrapper = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        empty_wrapper.set_valign(Gtk.Align.START)
        empty_wrapper.append(self.empty_page)
        self.archive_stack.add_named(empty_wrapper, "empty")
        self.error_page = empty_state(
            "Meeting archive needs repair",
            "The malformed archive was preserved; retained Meeting writes remain disabled.",
            "dialog-warning-symbolic",
        )
        self.archive_stack.add_named(self.error_page, "error")
        self.list_box = Gtk.ListBox(selection_mode=Gtk.SelectionMode.NONE)
        self.list_box.add_css_class("boxed-list")
        self.archive_stack.add_named(self.list_box, "meetings")
        self.archive_stack.set_vexpand(True)
        content.append(self.archive_stack)

        self.scroll = Gtk.ScrolledWindow(
            vexpand=True,
            hscrollbar_policy=Gtk.PolicyType.NEVER,
            vscrollbar_policy=Gtk.PolicyType.AUTOMATIC,
        )
        self.scroll.set_child(clamp(content))
        self.append(self.scroll)
        self.append(self._build_action_bar())
        self.set_privacy(False)
        self.set_audio_routes("Default (automatic)", "Default (automatic)")
        self.refresh()

    def _build_capture_card(self) -> Gtk.Box:
        """Summarize the exact Meeting boundary without nesting the primary action."""
        card, body = card_box()
        heading = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=SPACE_3)
        icon = Gtk.Image.new_from_icon_name("system-users-symbolic")
        icon.set_pixel_size(32)
        icon.add_css_class("accent")
        icon.set_valign(Gtk.Align.START)
        heading.append(icon)
        status_copy = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=SPACE_1)
        title = Gtk.Label(label="Ready for an explicit Meeting", xalign=0)
        title.add_css_class("title-2")
        status_copy.append(title)
        self.status_label = Gtk.Label(
            label="Meeting is idle",
            xalign=0,
            wrap=True,
            selectable=False,
            accessible_role=Gtk.AccessibleRole.STATUS,
        )
        self.status_label.add_css_class("dim-label")
        status_copy.append(self.status_label)
        heading.append(status_copy)
        body.append(heading)

        self.audio_routes_row = SummaryRow("Audio routes")
        self.privacy_row = SummaryRow()
        body.append(summary_list(self.audio_routes_row, self.privacy_row))
        boundary = Gtk.Expander(label="Capture boundary and cloud processing")
        detail = Gtk.Label(
            label=(
                "System output is desktop-wide sink audio, not per-window capture. After Stop, audio is uploaded "
                "to ElevenLabs Scribe v2 for transcription and speaker diarization. Meeting never pastes text."
            ),
            xalign=0,
            wrap=True,
        )
        detail.set_margin_top(SPACE_2)
        boundary.set_child(detail)
        body.append(boundary)
        return card

    def _build_action_bar(self) -> Gtk.Box:
        """Keep the explicit start or stop action visible below the scrolling archive."""
        bar = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        bar.add_css_class("vs-dock")
        content = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=SPACE_1)
        set_margins(content, SPACE_3)
        self.record_button = Gtk.Button(hexpand=True)
        self.record_button.add_css_class("vs-record")
        self.record_button.add_css_class("suggested-action")
        set_button_content(self.record_button, "system-users-symbolic", "Start Meeting capture")
        self.record_button.set_size_request(-1, PRIMARY_ACTION_HEIGHT)
        self.record_button.connect("clicked", self._toggle_capture)
        content.append(self.record_button)
        hint = Gtk.Label(
            label="Always explicit · microphone + system output · never auto-pastes",
            xalign=0.5,
            wrap=True,
        )
        hint.add_css_class("caption")
        content.append(hint)
        bar.append(clamp(content))
        return bar

    def set_audio_routes(self, microphone_name: str, system_output_name: str) -> None:
        """Show the exact configured source and sink before explicit Meeting start."""
        self.audio_routes_row.set_subtitle(f"{microphone_name} + everything playing through {system_output_name}")

    def set_privacy(self, incognito: bool) -> None:
        """Make the frozen next-session retention behavior visible before capture."""
        if incognito:
            self.privacy_row.set_title("Incognito for the next Meeting")
            self.privacy_row.set_subtitle(
                "Uploaded to ElevenLabs after Stop; local audio and Meeting metadata are erased"
            )
        else:
            self.privacy_row.set_title("Private local archive")
            self.privacy_row.set_subtitle(
                "Uploaded to ElevenLabs after Stop; mixed audio is retained for explicit retry"
            )

    def set_status(self, message: str) -> None:
        """Expose Meeting state and controlled recovery messages on its own page."""
        self.status_label.set_label(message)

    def set_capture_state(self, *, recording: bool, processing: bool = False) -> None:
        """Reflect one explicit capture lifecycle without changing dictation controls."""
        if processing:
            set_button_content(self.record_button, "content-loading-symbolic", "Transcribing Meeting…")
            self.record_button.set_sensitive(False)
            self.record_button.remove_css_class("destructive-action")
            self.record_button.remove_css_class("suggested-action")
            return
        self.record_button.set_sensitive(True)
        if recording:
            set_button_content(self.record_button, "media-playback-stop-symbolic", "Stop and transcribe Meeting")
            self.record_button.remove_css_class("suggested-action")
            self.record_button.add_css_class("destructive-action")
        else:
            set_button_content(self.record_button, "system-users-symbolic", "Start Meeting capture")
            self.record_button.remove_css_class("destructive-action")
            self.record_button.add_css_class("suggested-action")

    def refresh(self) -> None:
        """Rebuild the archive while preserving open records and scroll position."""
        expanded = {identifier for identifier, row in self.meeting_rows.items() if row.get_expanded()}
        scroll_position = self.scroll.get_vadjustment().get_value()
        self._clear_list(self.list_box)
        self.meeting_rows.clear()
        meetings = self.store.recent()
        self.count_label.set_label(f"{len(meetings)} saved")
        if self.store.persistence_error is not None:
            self.error_page.set_description(
                "The existing file was preserved unchanged. Repair it before retaining another Meeting, "
                "or use Incognito for a non-persistent session."
            )
            self.archive_stack.set_visible_child_name("error")
        elif not meetings:
            self.archive_stack.set_visible_child_name("empty")
        else:
            self.archive_stack.set_visible_child_name("meetings")
            for meeting in meetings:
                row = self._build_meeting(meeting)
                row.set_expanded(meeting.identifier in expanded)
                self.meeting_rows[meeting.identifier] = row
                self.list_box.append(row)
        GLib.idle_add(self._restore_scroll_position, scroll_position)

    def _restore_scroll_position(self, position: float) -> bool:
        """Restore the bounded archive position after GTK measures rebuilt rows."""
        adjustment = self.scroll.get_vadjustment()
        maximum = max(adjustment.get_lower(), adjustment.get_upper() - adjustment.get_page_size())
        adjustment.set_value(min(position, maximum))
        return GLib.SOURCE_REMOVE

    def _build_meeting(self, meeting: MeetingRecord) -> Adw.ExpanderRow:
        """Build one concise Meeting record with insights and transcript disclosed separately."""
        preview = " ".join(meeting.transcript.split())
        title = meeting.title or preview[:80] or "Transcription failed — audio retained"
        row = Adw.ExpanderRow(title=title, subtitle=self._meeting_subtitle(meeting))

        title_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=SPACE_2)
        set_margins(title_box, SPACE_3)
        title_entry = Gtk.Entry(hexpand=True, placeholder_text="Optional title")
        title_entry.set_text(meeting.title or "")
        title_box.append(title_entry)
        save_title = Gtk.Button(label="Save title")
        save_title.connect("clicked", self._save_title, meeting, title_entry)
        title_box.append(save_title)
        row.add_row(title_box)

        notes = Adw.ExpanderRow(
            title="Summary and actions",
            subtitle="Review generated notes, decisions, action items, and speaker turns",
        )
        notes.add_row(self._text_block("Summary", meeting.insights.summary or "No summary available."))
        notes.add_row(self._text_block("Decisions", _line_list(meeting.insights.decisions)))
        notes.add_row(self._text_block("Action items", _line_list(meeting.insights.action_items)))
        speaker_text = "\n".join(
            f"[{_timestamp(segment.started_at_seconds)}] {segment.speaker}: {segment.text}"
            for segment in meeting.speakers
        )
        notes.add_row(self._text_block("Speakers", speaker_text or "Speaker labels unavailable."))
        row.add_row(notes)

        transcript_preview = preview[:100] or "Transcription has not completed"
        transcript = Adw.ExpanderRow(title="Transcript", subtitle=transcript_preview)
        transcript.add_row(
            self._text_block("Full transcript", meeting.transcript or "Transcription has not completed.")
        )
        row.add_row(transcript)
        if meeting.warnings:
            warnings = Adw.ExpanderRow(
                title="Capture warnings",
                subtitle=f"{len(meeting.warnings)} item{'s' if len(meeting.warnings) != 1 else ''}",
            )
            warnings.add_row(self._text_block("Warnings", "\n".join(meeting.warnings)))
            row.add_row(warnings)

        actions = Gtk.FlowBox(selection_mode=Gtk.SelectionMode.NONE, row_spacing=SPACE_2, column_spacing=SPACE_2)
        actions.set_max_children_per_line(3)
        set_margins(actions, SPACE_3)
        copy_transcript = Gtk.Button(label="Copy transcript")
        copy_transcript.set_sensitive(bool(meeting.transcript))
        copy_transcript.connect("clicked", self._copy_transcript, meeting)
        actions.append(copy_transcript)
        retry = Gtk.Button(label="Retry transcription")
        retry.set_visible(
            meeting.recognition_status is MeetingRecognitionStatus.FAILED
            and self.store.recording_path(meeting) is not None
        )
        retry.connect("clicked", self._retry, meeting)
        actions.append(retry)
        export_markdown = Gtk.Button(label="Export Markdown")
        export_markdown.connect("clicked", self._export, meeting, "markdown")
        actions.append(export_markdown)
        export_json = Gtk.Button(label="Export JSON")
        export_json.connect("clicked", self._export, meeting, "json")
        actions.append(export_json)
        delete = Gtk.Button(label="Delete")
        delete.add_css_class("destructive-action")
        delete.connect("clicked", self._confirm_delete, meeting)
        actions.append(delete)
        row.add_row(actions)
        return row

    @staticmethod
    def _text_block(title: str, text: str) -> Gtk.Box:
        """Render a selectable wrapped record section without editing durable content."""
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=SPACE_1)
        set_margins(box, SPACE_3)
        heading = Gtk.Label(label=title, xalign=0)
        heading.add_css_class("heading")
        body = Gtk.Label(label=text, xalign=0, wrap=True, selectable=True)
        box.append(heading)
        box.append(body)
        return box

    def _meeting_subtitle(self, meeting: MeetingRecord) -> str:
        """Summarize status, duration, recording, and local time."""
        try:
            captured = datetime.fromisoformat(meeting.timestamp).astimezone().strftime("%Y-%m-%d %H:%M")
        except ValueError:
            captured = meeting.timestamp
        retained = " · recovery audio" if self.store.recording_path(meeting) is not None else ""
        return (
            f"{meeting.recognition_status.value.title()} · {_duration(meeting.duration_seconds)}{retained} · {captured}"
        )

    def _toggle_capture(self, _button: Gtk.Button) -> None:
        """Delegate only the explicit Meeting button action to the application."""
        self.toggle_capture()

    def _save_title(self, _button: Gtk.Button, meeting: MeetingRecord, title_entry: Gtk.Entry) -> None:
        """Persist a trimmed optional title and refresh the row label."""
        try:
            self.store.rename(meeting.identifier, title_entry.get_text().strip() or None)
        except Exception as error:
            self.show_message(f"Meeting title could not be saved: {error}")
            return
        self.refresh()
        self.show_message("Meeting title saved.")

    def _copy_transcript(self, _button: Gtk.Button, meeting: MeetingRecord) -> None:
        """Copy only after the user selects this archived transcript action."""
        self.copy_text(meeting.transcript)

    def _retry(self, _button: Gtk.Button, meeting: MeetingRecord) -> None:
        """Request one explicit background retry for retained Meeting audio."""
        self.retry_recognition(meeting)

    def _export(self, _button: Gtk.Button, meeting: MeetingRecord, export_format: str) -> None:
        """Export one record and expose its exact owner-local path."""
        try:
            output_path = self.store.export(meeting, self.export_directory, export_format)
        except Exception as error:
            self.show_message(f"Meeting export failed: {error}")
            return
        self.show_message(f"Exported Meeting to {output_path}")

    def _confirm_delete(self, _button: Gtk.Button, meeting: MeetingRecord) -> None:
        """Require confirmation before erasing a Meeting and its retained recording."""
        dialog = Adw.AlertDialog.new(
            "Delete this Meeting?",
            "The transcript, notes, metadata, and retained recording will be permanently deleted.",
        )
        dialog.add_response("cancel", "Cancel")
        dialog.add_response("delete", "Delete permanently")
        dialog.set_default_response("cancel")
        dialog.set_close_response("cancel")
        dialog.set_response_appearance("delete", Adw.ResponseAppearance.DESTRUCTIVE)
        dialog.choose(self, None, self._delete_chosen, meeting)

    def _delete_chosen(
        self,
        dialog: Adw.AlertDialog,
        result: Gio.AsyncResult,
        meeting: MeetingRecord,
    ) -> None:
        """Apply a confirmed Meeting deletion and refresh the archive."""
        if dialog.choose_finish(result) != "delete" or not self.delete_meeting(meeting):
            return
        self.refresh()
        self.show_message("Meeting and its retained recording permanently deleted.")

    @staticmethod
    def _clear_list(list_box: Gtk.ListBox) -> None:
        """Remove every current child without relying on stale row indexes."""
        child = list_box.get_first_child()
        while child is not None:
            next_child = child.get_next_sibling()
            list_box.remove(child)
            child = next_child


def _line_list(values: tuple[str, ...]) -> str:
    """Render literal insight values or an honest empty state."""
    return "\n".join(f"• {value}" for value in values) if values else "None recorded."


def _timestamp(seconds: float) -> str:
    """Format a non-negative speaker offset as minutes and seconds."""
    total_seconds = max(0, int(seconds + 0.5))
    return f"{total_seconds // 60}:{total_seconds % 60:02d}"


def _duration(seconds: float) -> str:
    """Format a non-negative Meeting duration compactly."""
    total_seconds = max(0, int(seconds + 0.5))
    hours, remainder = divmod(total_seconds, 3_600)
    minutes, seconds = divmod(remainder, 60)
    return f"{hours}:{minutes:02d}:{seconds:02d}" if hours else f"{minutes}:{seconds:02d}"
