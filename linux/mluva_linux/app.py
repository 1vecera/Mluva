"""GTK 4 desktop application for Mluva on Linux."""

import os
import sqlite3
import sys
import threading
import time
import uuid
from dataclasses import replace
from datetime import UTC, datetime
from pathlib import Path

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, Gdk, Gio, GLib, Gtk, Pango  # noqa: E402

from mluva_linux.audio import PipeWireMeetingRecorder, PipeWireRecorder
from mluva_linux.batch_preview import BatchPreviewClient, BatchPreviewSession
from mluva_linux.codex_client import CodexAppServerClient, CodexModel, select_model
from mluva_linux.command_palette import CommandPalette, application_commands
from mluva_linux.config import (
    FUNCTION_KEY_OPTIONS,
    TRANSCRIPTION_LANGUAGE_OPTIONS,
    AppConfig,
    AudioRetentionPolicy,
    default_config_dir,
    default_data_dir,
    default_runtime_dir,
    elevenlabs_api_key,
    load_config,
    save_config,
)
from mluva_linux.conversation import (
    MAX_REWRITE_CHARACTERS,
    ConversationStore,
    rewrite_prompt,
)
from mluva_linux.conversation_titles import clean_title, fallback_title, save_generated_title, title_prompt
from mluva_linux.conversation_view import ConversationWorkspace
from mluva_linux.delivery import deliver_text, keyboard_paste_available
from mluva_linux.diagnostics import (
    DiagnosticOutcome,
    DiagnosticProvider,
    DiagnosticsStore,
    DiagnosticStage,
)
from mluva_linux.elevenlabs import ElevenLabsClient
from mluva_linux.feature_maturity import (
    FeatureMaturity,
    capabilities_with_maturity,
    feature_capability,
    maturity_description,
    maturity_title,
)
from mluva_linux.global_shortcuts import GlobalShortcutService
from mluva_linux.history import (
    ENHANCEMENT_PROVIDER_CODEX_APP_SERVER,
    RECOGNITION_FALLBACK_STARTUP_FAILED,
    RECOGNITION_FALLBACK_STREAM_FAILED,
    RECOGNITION_FALLBACK_UNAVAILABLE,
    HistoryEntry,
    HistoryStore,
)
from mluva_linux.history_view import HistoryPage
from mluva_linux.live_rewrite import TEMPLATE_CHOICES, LiveRewriteSchedule, initial_draft, live_prompt
from mluva_linux.meeting import (
    MeetingFailure,
    MeetingRecord,
    MeetingStore,
    MeetingWorkflow,
    MeetingWorkflowResult,
)
from mluva_linux.meeting_view import MeetingPage
from mluva_linux.overlay_state import RecordingOverlayPublisher, RecordingOverlayState
from mluva_linux.personalization import BUILT_IN_STYLES, PersonalizationStore, SavedStyle
from mluva_linux.personalization_view import PersonalizationPage
from mluva_linux.pipewire import PipeWireCatalogError, PipeWireDeviceCatalog, PipeWireDeviceKind
from mluva_linux.prompt_editor import PromptEditor, PromptsPage, prompt_control
from mluva_linux.prompts import PromptStore
from mluva_linux.provider_settings import ProviderSettings
from mluva_linux.providers import LiteLLMClient, transcription_client
from mluva_linux.realtime import (
    ElevenLabsRealtimeClient,
    RealtimeTranscriptionSession,
)
from mluva_linux.rewrite_settings import RewriteSettings
from mluva_linux.scratchpad import ScratchpadDraft, ScratchpadDraftStore
from mluva_linux.segment_cleanup import (
    CodexSegmentCleanupAttempt,
    SegmentCleanupSession,
)
from mluva_linux.text_target import (
    FocusedTextTargetTracker,
    TextSelectionTooLargeError,
    TextTargetSnapshot,
)
from mluva_linux.theme import ThemeController
from mluva_linux.ui import (
    COMPACT_LAYOUT_MAX_WIDTH,
    RECORDING_KIND_PREPARING,
    RECORDING_KIND_RECORDING,
    RESULT_EDITOR_MIN_HEIGHT,
    SPACE_1,
    SPACE_2,
    SPACE_3,
    SPACE_4,
    FeatureMaturityNotice,
    NavigationRail,
    RecordingBarState,
    RecordingStatusBar,
    SummaryRow,
    card_box,
    maturity_badge,
    set_button_content,
    set_margins,
)
from mluva_linux.workflow import (
    DictationWorkflow,
    TranscriptPreparationSnapshot,
    WorkflowFailure,
    WorkflowResult,
    reprocess_history_entry,
)
from mluva_linux.workspace_settings import WorkspaceSettings

CAPTURE_MODE_OPTIONS = (
    (
        "dictation",
        "Dictate",
        "Press the configured global function key once to start and again to finish, or use the copy-only button.",
    ),
    ("command", "Command", "Speak an instruction for the explicitly selected text or captured caret."),
    ("scratchpad", "Notes", "Capture a longer editable draft that remains acceptance-gated."),
)
CAPTURE_MODE_IDS = tuple(option[0] for option in CAPTURE_MODE_OPTIONS)
CAPTURE_MODE_LABELS = tuple(option[1] for option in CAPTURE_MODE_OPTIONS)
CAPTURE_MODE_FEATURE_IDS = ("dictation", "command_mode", "notes_mode")
CAPTURE_MODE_TOOLTIP = (
    "Dictate: press the configured global function key to start and again to finish.\n"
    "Command (Experimental): review a spoken edit or drafting instruction before delivery.\n"
    "Notes (Experimental): keep a longer editable draft until you copy or delete it.\n"
    "Meeting (Experimental): use the separate Meeting tab for explicit microphone and system-audio capture."
)
GLOBAL_RECORDING_KEY_GUIDANCE = (
    "F13–F24 typically have the fewest conflicts but usually need a programmable keyboard layer; "
    "F9 is the practical default on a standard keyboard"
)
GLOBAL_RECORDING_KEY_TOOLTIP = (
    "Press once to start and again to stop. F1–F12 may already be used by applications, desktop features, "
    "or keyboard firmware. F13–F24 are usually least used but are absent from most standard keyboards."
)
MAX_IN_MEMORY_HISTORY_TARGETS = 32
CAPTURE_CONTENT_MAX_WIDTH = 1120


def capture_mode_description(selected_index: int) -> str:
    """Return the complete pre-recording explanation for one visible Capture mode."""
    return maturity_description(CAPTURE_MODE_FEATURE_IDS[selected_index], CAPTURE_MODE_OPTIONS[selected_index][2])


def _callout_summary(message: str) -> str:
    """Collapse one diagnostic message to its first sentence for the compact callout."""
    first_sentence = message.split(". ")[0].strip()
    return first_sentence if first_sentence else message


def _elapsed_seconds(elapsed: str) -> int:
    """Convert the in-window ``MM:SS`` display to the bounded overlay value."""
    try:
        minute_text, second_text = elapsed.split(":", maxsplit=1)
        minutes = int(minute_text)
        seconds = int(second_text)
    except (TypeError, ValueError):
        return 0
    if minutes < 0 or not 0 <= seconds < 60:
        return 0
    return min((minutes * 60) + seconds, 86_400)


def _smooth_motion_enabled(config: AppConfig) -> bool:
    """Carry the desktop's reduced-motion preference across the GTK-to-QML boundary."""
    settings = Gtk.Settings.get_default()
    return (
        config.smooth_scrolling
        and config.scroll_duration_ms > 0
        and (settings is None or settings.get_property("gtk-enable-animations"))
    )


def _overlay_detail_and_route(detail: str) -> tuple[str, str]:
    """Separate a concise phase from provider/device detail for the Shell layout."""
    phase, separator, route = detail.partition(" · ")
    return (phase, route) if separator else (detail, "")


class MluvaApplication(Adw.Application):
    """Own the Linux window and one-at-a-time recording lifecycle."""

    def __init__(self) -> None:
        """Construct the application before GTK activates its first window."""
        super().__init__(application_id="com.mluva.Linux", flags=Gio.ApplicationFlags.DEFAULT_FLAGS)
        self.connect("activate", self._on_activate)
        self.connect("shutdown", self._on_shutdown)
        self.window: Adw.ApplicationWindow | None = None
        self.page_stack: Adw.ViewStack | None = None
        self.header_bar: Adw.HeaderBar | None = None
        self.navigation_rail: NavigationRail | None = None
        self.page_title_label: Gtk.Label | None = None
        self.navigation_bar: Adw.ViewSwitcherBar | None = None
        self.toast_overlay: Adw.ToastOverlay | None = None
        self.settings_dialog: Adw.PreferencesDialog | None = None
        self.settings_button: Gtk.Button | None = None
        self.setup_callout: Gtk.Revealer | None = None
        self.setup_callout_title: Gtk.Label | None = None
        self.setup_callout_body: Gtk.Label | None = None
        self.setup_callout_button: Gtk.Button | None = None
        self.capture_initialization_failed = False
        self.record_button: Gtk.Button | None = None
        self.status_label: Gtk.Label | None = None
        self.capture_status_title: Gtk.Label | None = None
        self.capture_summary_box: Gtk.Widget | None = None
        self.capture_mode_status_row: SummaryRow | None = None
        self.capture_mode_maturity: FeatureMaturityNotice | None = None
        self.capture_segment_box: Gtk.Box | None = None
        self.capture_segment_buttons: list[Gtk.ToggleButton] = []
        self.recording_bar: RecordingStatusBar | None = None
        self.recording_bar_slot: Gtk.Revealer | None = None
        self.recording_overlay_publisher: RecordingOverlayPublisher | None = None
        self.capture_delivery_status_row: SummaryRow | None = None
        self.capture_privacy_status_row: SummaryRow | None = None
        self.capture_action_hint: Gtk.Label | None = None
        self.capture_action_bar: Gtk.Box | None = None
        self.mode: Adw.ComboRow | None = None
        self.language: Adw.ComboRow | None = None
        self.language_codes: list[str] = []
        self.microphone_device: Adw.ComboRow | None = None
        self.system_audio_device: Adw.ComboRow | None = None
        self.microphone_targets: list[str | None] = []
        self.system_audio_targets: list[str | None] = []
        self.refresh_audio_button: Gtk.Button | None = None
        self.refreshing_audio_devices = False
        self.output_style: Adw.ComboRow | None = None
        self.output_style_instructions: Adw.ActionRow | None = None
        self.remember_application_switch: Adw.SwitchRow | None = None
        self.style_identifiers: list[str | None] = []
        self.refreshing_style_controls = False
        self.cleanup_switch: Adw.SwitchRow | None = None
        self.spoken_commands_switch: Adw.SwitchRow | None = None
        self.auto_paste_switch: Adw.SwitchRow | None = None
        self.global_recording_key: Adw.ComboRow | None = None
        self.global_shortcut_status_row: Adw.ActionRow | None = None
        self.incognito_switch: Adw.SwitchRow | None = None
        self.audio_retention: Adw.ComboRow | None = None
        self.history_retention: Adw.ComboRow | None = None
        self.output_view: Gtk.TextView | None = None
        self.output_buffer: Gtk.TextBuffer | None = None
        self.command_source_label: Gtk.Label | None = None
        self.command_actions: Gtk.Box | None = None
        self.accept_command_button: Gtk.Button | None = None
        self.scratchpad_actions: Gtk.Box | None = None
        self.output_section: Gtk.Widget | None = None
        self.recorder: PipeWireRecorder | None = None
        self.meeting_recorder: PipeWireMeetingRecorder | None = None
        self.realtime_client: ElevenLabsRealtimeClient | None = None
        self.realtime_session: RealtimeTranscriptionSession | None = None
        self.segment_cleanup_session: SegmentCleanupSession | None = None
        self.workflow: DictationWorkflow | None = None
        self.meeting_workflow: MeetingWorkflow | None = None
        self.history_store: HistoryStore
        self.meeting_store: MeetingStore
        self.scratchpad_store: ScratchpadDraftStore
        self.diagnostics_store: DiagnosticsStore
        self.personalization_store: PersonalizationStore
        self.history_page: HistoryPage | None = None
        self.history_delivery_targets: dict[str, TextTargetSnapshot] = {}
        self.meeting_page: MeetingPage | None = None
        self.personalization_page: PersonalizationPage | None = None
        self.audio_path: Path | None = None
        self.shortcut_service: GlobalShortcutService | None = None
        self.approved_recording_trigger: str | None = None
        self.approved_rewrite_trigger: str | None = None
        self.rewrite_shortcut_status_row: Adw.ActionRow | None = None
        self.focus_tracker: FocusedTextTargetTracker | None = None
        self.pending_mode = "dictation"
        self.pending_cleanup = False
        self.pending_incognito = False
        self.cleanup_before_incognito: bool | None = None
        self.pending_audio_retention = AudioRetentionPolicy.FAILURES
        self.capture_allows_auto_paste = False
        self.editing_scratchpad = False
        self.pending_command_target: TextTargetSnapshot | None = None
        self.pending_delivery_target: TextTargetSnapshot | None = None
        self.pending_application_identifier: str | None = None
        self.profile_application_identifier: str | None = None
        self.pending_style_identifier: str | None = None
        self.pending_style: SavedStyle | None = None
        self.pending_use_saved_style = False
        self.pending_codex_model_identifier: str | None = None
        self.pending_transcript_preparation: TranscriptPreparationSnapshot | None = None
        self.pending_command_result: WorkflowResult | None = None
        self.retry_in_progress = False
        self.retry_identifier: str | None = None
        self.capture_processing = False
        self.capture_preparing = False
        self.capture_stop_requested = False
        self.pending_realtime_fallback_reason: str | None = None
        self.shutting_down = False
        self.meeting_processing = False
        self.meeting_retry_in_progress = False
        self.meeting_retry_identifier: str | None = None
        self.meeting_identifier: str | None = None
        self.meeting_audio_path: Path | None = None
        self.meeting_started_at: datetime | None = None
        self.meeting_capture_started_at: float | None = None
        self.meeting_capture_status_timeout_id: int | None = None
        self.meeting_pending_incognito = False
        self.capture_status_timeout_id: int | None = None
        self.capture_started_at: float | None = None
        self.pending_session_identifier: str | None = None
        self.data_directory: Path
        self.codex_workspace: Path
        self.config_path: Path
        self.config: AppConfig
        self.pipewire_catalog = PipeWireDeviceCatalog()
        self.pipewire_catalog_error: str | None = None
        self.conversation_workspace: ConversationWorkspace | None = None
        self.conversation_store: ConversationStore
        self.live_rewrite_client = None
        self.live_schedule = None
        self.live_config = None
        self.live_session_identifier = None
        self.live_final_entry = None
        self.live_final_text = ""
        self.live_last_model = ""
        self.live_revision = 0
        self.live_updating = False
        self.live_mode_switch = None
        self.live_template_buttons = {}
        self.prompt_editor = None
        self.prompts_page = None
        self.live_prompts = None
        self.live_prompt_session = None
        self.rewrite_client: CodexAppServerClient | LiteLLMClient | None = None
        self.model_catalog_client: CodexAppServerClient | None = None
        self.rewrite_settings: RewriteSettings | None = None
        self.title_client: CodexAppServerClient | None = None
        self.title_queue: list[tuple[str, str]] = []
        self.automatic_titles_switch: Adw.SwitchRow | None = None
        self.rewrite_draft: str | None = None
        self.rewrite_identifier: str | None = None
        self.overlay_review_identifier: str | None = None
        self.overlay_timeout_id: int | None = None
        self.theme_controller = ThemeController()
        self.command_palette: CommandPalette | None = None
        for name, callback in (
            ("latest", self._open_latest_conversation),
            ("record", self._shell_record),
            ("cancel", self._shortcut_cancelled),
            ("status", self._replay_overlay_status),
            ("history", self._open_history),
            ("settings", lambda: self._show_settings(self.settings_button)),
            ("commands", self._show_commands),
            ("meeting", lambda: self._navigate_to_page("meeting")),
            ("personalization", lambda: self._navigate_to_page("personalization")),
            ("quit", self.quit),
        ):
            action = Gio.SimpleAction.new(name, None)
            action.connect("activate", lambda _action, _parameter, handler=callback: handler())
            self.add_action(action)
        review_action = Gio.SimpleAction.new("review", GLib.VariantType.new("(sss)"))
        review_action.connect("activate", self._review_action)
        self.add_action(review_action)
        self.set_accels_for_action("app.commands", ["<Control>p"])

    def _on_activate(self, _application: Adw.Application) -> None:
        """Build the adaptive primary window and validate external runtime dependencies."""
        if self.window is not None:
            self.window.present()
            return
        self._initialize_recording_overlay()
        self.theme_controller.apply()
        Gtk.IconTheme.get_for_display(Gdk.Display.get_default()).add_search_path(
            str(Path(__file__).parents[1] / "resources")
        )
        self._initialize_local_services()
        self.window = Adw.ApplicationWindow(application=self, title="Mluva")
        self.window.set_default_size(1060, 780)
        self.window.set_size_request(420, 520)
        self.window.connect("close-request", self._hide_window)
        self.hold()
        key_controller = Gtk.EventControllerKey()
        key_controller.connect("key-pressed", self._key_pressed)
        self.window.add_controller(key_controller)
        command_keys = Gtk.EventControllerKey(propagation_phase=Gtk.PropagationPhase.CAPTURE)
        command_keys.connect("key-pressed", self._command_key_pressed)
        self.window.add_controller(command_keys)

        toolbar = Adw.ToolbarView()
        self.header_bar = Adw.HeaderBar()
        self.page_title_label = Gtk.Label(xalign=0)
        self.page_title_label.add_css_class("vs-page-title")
        self.header_bar.set_title_widget(self.page_title_label)
        toolbar.add_top_bar(self.header_bar)
        stack = Adw.ViewStack()
        self.page_stack = stack
        stack.add_titled_with_icon(
            self._build_capture_page(),
            "capture",
            "Conversations",
            "audio-input-microphone-symbolic",
        )
        self.conversation_workspace.live_header.connect("notify::visible", self._sync_header_title)
        self.meeting_page = MeetingPage(
            store=self.meeting_store,
            export_directory=self.data_directory / "exports" / "meetings",
            toggle_capture=self._toggle_meeting_capture,
            copy_text=self._copy_text,
            retry_recognition=self._retry_meeting_recognition,
            delete_meeting=self._delete_meeting,
            show_message=self._set_meeting_status,
            time_format=self.config.time_format,
        )
        self.meeting_page.set_privacy(self.config.incognito_mode)
        self._update_meeting_audio_routes()
        stack.add_titled_with_icon(self.meeting_page, "meeting", "Meeting", "system-users-symbolic")
        self.history_page = HistoryPage(
            store=self.history_store,
            conversations=self.conversation_store,
            export_directory=self.data_directory / "exports",
            copy_text=self._copy_text,
            can_retry_delivery=self._can_retry_history_delivery,
            retry_delivery=self._retry_history_delivery,
            retry_recognition=self._retry_history_recognition,
            reprocess_entry=self._reprocess_history_entry,
            delete_entry=self._delete_history_entry,
            history_changed=self._history_changed,
            show_message=self._show_toast,
            time_format=self.config.time_format,
        )
        stack.add_titled_with_icon(self.history_page, "history", "History", "document-open-recent-symbolic")
        self.personalization_page = PersonalizationPage(
            store=self.personalization_store,
            history_store=self.history_store,
            show_message=self._show_toast,
            styles_changed=self._refresh_style_controls,
            edit_prompt=self._open_prompt_editor,
        )
        stack.add_titled_with_icon(
            self.personalization_page,
            "personalization",
            "Personalization",
            "document-edit-symbolic",
        )
        stack.connect("notify::visible-child", self._visible_page_changed)

        self.settings_button = Gtk.Button(icon_name="preferences-system-symbolic")
        self.settings_button.add_css_class("vs-utility")
        self.settings_button.set_tooltip_text("Mluva settings")
        self.settings_button.connect("clicked", self._show_settings)
        self.header_bar.pack_end(self.settings_button)

        sidebar_button = Gtk.Button(icon_name="sidebar-show-symbolic")
        sidebar_button.set_tooltip_text("Show or hide history")
        sidebar_button.connect("clicked", self._toggle_history_sidebar)
        self.header_bar.pack_start(sidebar_button)
        home = Gtk.Button(icon_name="go-home-symbolic", has_frame=False)
        home.set_tooltip_text("Conversations")
        home.connect("clicked", lambda _button: self._navigate_to_page("capture"))
        self.header_bar.pack_start(home)
        menu = Gio.Menu()
        for label, action_name in (
            ("Commands", "commands"),
            ("Latest conversation", "latest"),
            ("Manage history", "history"),
            ("Meeting recording", "meeting"),
            ("Vocabulary and prompts", "personalization"),
            ("Quit Mluva", "quit"),
        ):
            menu.append(label, f"app.{action_name}")
        self.main_menu_button = Gtk.MenuButton(icon_name="open-menu-symbolic", menu_model=menu)
        self.main_menu_button.set_tooltip_text("Mluva menu")
        self.header_bar.pack_end(self.main_menu_button)

        shell = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        shell.append(stack)
        shell.set_hexpand(True)
        self.toast_overlay = Adw.ToastOverlay()
        self.toast_overlay.set_child(shell)
        workspace = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL)
        workspace.append(self.toast_overlay)
        toolbar.set_content(workspace)
        self.window.set_content(toolbar)
        self._install_navigation_breakpoint()

        try:
            self._initialize_capture_services()
        except Exception as error:
            self._set_initialization_error(str(error))
        self._restore_scratchpad()
        self._update_capture_status_rows()
        self._visible_page_changed()
        latest = self.conversation_store.search(limit=1)
        if latest and self.conversation_workspace is not None:
            self.conversation_workspace.show_conversation(
                latest[0], self.conversation_store.replies(latest[0].identifier)
            )
        self.window.set_focus(self.conversation_workspace.prompt)
        self.window.present()
        if self.config_load_error:
            self._show_toast(self.config_load_error)

    def _hide_window(self, window: Adw.ApplicationWindow) -> bool:
        """Keep the app and approved shortcuts running when its window is closed."""
        window.set_visible(False)
        return True

    def _replay_overlay_status(self) -> None:
        """Synchronize an attaching shell without changing any capture or delivery state."""
        if self.recording_overlay_publisher is not None:
            self.recording_overlay_publisher.replay()

    def _shell_record(self) -> None:
        """Use the visible-button copy boundary for a deliberate shell-menu action."""
        if self.window is None:
            self.activate()
        self._toggle_recording(self.record_button)

    def _open_latest_conversation(self) -> bool:
        """Present the newest completed conversation and focus its rewrite composer."""
        self.activate()
        self._navigate_to_page("capture")
        workspace = self.conversation_workspace
        entries = self.conversation_store.search(limit=1)
        if workspace is not None:
            if entries:
                workspace.show_conversation(entries[0], self.conversation_store.replies(entries[0].identifier))
            workspace.prompt.grab_focus()
        return GLib.SOURCE_REMOVE

    def _open_history(self) -> None:
        """Present recovery and management tools without crowding daily dictation."""
        self.activate()
        workspace = self.conversation_workspace
        if self.history_page is not None:
            self.history_page.focus_entry(workspace.entry.identifier if workspace and workspace.entry else None)
        self._navigate_to_page("history")

    def _toggle_history_sidebar(self, _button: Gtk.Button) -> None:
        """Make history available even when the window uses the compact layout."""
        self._navigate_to_page("capture")
        if self.conversation_workspace is not None:
            split = self.conversation_workspace.split
            visible = not split.get_show_sidebar()
            if self._apply_workspace_settings({"history_sidebar_visible": visible}):
                split.set_show_sidebar(visible)

    def _start_pasted_conversation(self, text: str) -> None:
        """Import explicitly supplied text without replacing the clipboard."""
        workspace = self.conversation_workspace
        if workspace is None or not text.strip():
            return
        if self.config.incognito_mode:
            workspace.show_transient(text, text)
            return
        try:
            entry = self.history_store.add(
                raw_text=text,
                delivered_text=text,
                mode="dictation",
                language_code=self.config.language_code,
                transcription_id=None,
                delivery_outcome="imported",
            )
        except Exception:
            workspace.set_busy(False, "Could not save this conversation. Your text is still in the editor.")
            return
        workspace.prompt.get_buffer().set_text("")
        workspace.show_conversation(entry, [])
        self._history_changed()
        self._queue_conversation_title(entry)

    def _queue_conversation_title(self, entry: HistoryEntry) -> None:
        """Label new completions locally, then queue at most one background model at a time."""
        if self.shutting_down or self.config.incognito_mode or entry.title or not entry.raw_text.strip():
            return
        fallback = fallback_title(entry.raw_text)
        try:
            saved = save_generated_title(self.history_store, entry.identifier, fallback)
        except sqlite3.Error:
            return
        if not saved:
            return
        self._refresh_conversation_title(entry.identifier)
        if self.config.automatic_titles and len(self.title_queue) < 20:
            self.title_queue.append((entry.identifier, fallback))
            self._start_next_title()

    def _start_next_title(self) -> None:
        """Skip removed notes; never replay old history or retry a failed provider automatically."""
        if (
            self.title_client is not None
            or self.shutting_down
            or self.config.incognito_mode
            or not self.config.automatic_titles
        ):
            return
        while self.title_queue:
            identifier, fallback = self.title_queue.pop(0)
            try:
                entry = self.history_store.find(identifier)
            except KeyError:
                continue
            if entry.title != fallback:
                continue
            client = self._new_rewrite_client(request_timeout_seconds=10, turn_timeout_seconds=20)
            self.title_client = client
            threading.Thread(
                target=self._title_worker,
                args=(
                    client,
                    entry,
                    fallback,
                    self.config.litellm_model if self.config.rewrite_provider == "litellm" else self.config.codex_model,
                ),
                name="conversation-title",
                daemon=True,
            ).start()
            break

    def _title_worker(
        self, client: CodexAppServerClient, entry: HistoryEntry, fallback: str, model: str | None
    ) -> None:
        """Use the existing isolated, tool-free transport and keep failures out of the capture path."""
        title = None
        try:
            resolved = client.resolve_model(model)
            title = clean_title(
                client.transform(
                    title_prompt(entry, self.prompt_store.read("title").text),
                    self.codex_workspace,
                    resolved,
                    max_output_characters=128,
                )
            )
        except Exception:
            pass
        finally:
            client.close()
            GLib.idle_add(self._title_finished, client, entry.identifier, fallback, title)

    def _title_finished(self, client: CodexAppServerClient, identifier: str, fallback: str, title: str | None) -> bool:
        """Discard cancelled, private, deleted or manually renamed results at the commit boundary."""
        if client is not self.title_client:
            return GLib.SOURCE_REMOVE
        self.title_client = None
        if not self.shutting_down and not self.config.incognito_mode and self.config.automatic_titles:
            try:
                if title and save_generated_title(self.history_store, identifier, title, expected=fallback):
                    self._refresh_conversation_title(identifier)
            except sqlite3.Error:
                pass
            self._start_next_title()
        return GLib.SOURCE_REMOVE

    def _refresh_conversation_title(self, identifier: str) -> None:
        """Update labels in place so a late title never changes selection, scroll or draft text."""
        workspace = self.conversation_workspace
        if workspace is not None:
            workspace.refresh_title(identifier)
        if self.history_page is not None:
            self.history_page.refresh_title(identifier)

    def _cancel_titles(self, wait: bool = False) -> None:
        """Invalidate queued callbacks before asking the exact worker to stop."""
        self.title_queue.clear()
        client, self.title_client = self.title_client, None
        if client is not None:
            if wait:
                client.cancel()
            else:
                threading.Thread(target=client.cancel, name="cancel-title", daemon=True).start()

    def _automatic_titles_changed(self, row: Adw.SwitchRow, _param: object) -> None:
        """Save the cloud-title preference independently of transcription settings."""
        config = replace(self.config, automatic_titles=row.get_active())
        try:
            save_config(config, self.config_path)
        except OSError:
            self._show_toast("Could not save the title preference. It applies for this session.")
        self.config = config
        self._synchronize_workflow_config()
        if not config.automatic_titles:
            self._cancel_titles()

    def _live_mode_changed(self, button) -> None:
        """Start or pause Live during dictation without replacing the speech transport."""
        if button.get_active() == self.config.live_rewrite_enabled:
            return
        if not self._apply_workspace_settings({"live_rewrite_enabled": button.get_active()}):
            button.set_active(self.config.live_rewrite_enabled)
            self._show_toast(
                "Live rewrite is available during dictation; wait for preparation or finalization to finish."
            )

    def _live_template_changed(self, button, template: str) -> None:
        """Persist a menu choice through the same settings path as the Workspace page."""
        if not button.get_active() or template == self.config.live_rewrite_template:
            return
        if not self._apply_workspace_settings({"live_rewrite_template": template}):
            self._sync_live_mode()
            self._show_toast("Finish active work before changing the Live rewrite template.")
        self.live_mode_menu.popdown()

    def _sync_live_mode(self) -> None:
        """Reflect saved settings in the single Live rewrite control and its selected menu item."""
        if self.live_mode_switch is None:
            return
        self.live_mode_switch.set_active(self.config.live_rewrite_enabled)
        template = self.config.live_rewrite_template
        self.live_template_buttons[template].set_active(True)
        label = dict(TEMPLATE_CHOICES)[template]
        self.live_mode_switch.set_tooltip_text(
            f"Build a {label.lower()} draft from provisional speech; reconcile with the final transcript at Stop"
        )
        self.live_mode_menu.set_tooltip_text(f"Live rewrite template: {label}")

    def _live_preview_callback(self, session: str):
        """Offer provisional recognition only to the explicitly enabled Live rewrite session."""
        previous = ""

        def preview_changed(preview):
            """Keep the provider thread off GTK and discard callbacks from cancelled captures."""
            nonlocal previous
            text = preview.display_text
            if text and text != previous and session == self.live_session_identifier:
                previous = text
                GLib.idle_add(self._live_preview_arrived, session, text)

        return preview_changed

    def _live_preview_arrived(self, session: str, text: str) -> bool:
        """Start an eligible draft without waiting for the recording header's refresh timer."""
        if session == self.live_session_identifier and not self.capture_processing and self.live_final_entry is None:
            self._maybe_live_rewrite(text)
        return GLib.SOURCE_REMOVE

    def _start_live_rewrite(self, *, preserve_draft: bool = False) -> None:
        """Start a fresh scheduler, preserving deliberate edits when toggled during capture."""
        if self.live_prompt_session != self.pending_session_identifier or self.live_prompts is None:
            self.live_prompts = self.prompt_store.snapshot()
            self.live_prompt_session = self.pending_session_identifier
        draft = (
            self.conversation_workspace.live_draft()
            if preserve_draft
            else initial_draft(self.config, self.live_prompts)
        )
        self._cancel_live_rewrite()
        self.live_updating = True
        self.conversation_workspace.show_live_draft(draft, "Waiting for speech…")
        self.live_updating = False
        if not self.config.live_rewrite_enabled or self.pending_incognito or self.pending_mode != "dictation":
            self.conversation_workspace.live_draft_box.set_visible(False)
            return
        self.live_config = self.config
        self.live_session_identifier = self.pending_session_identifier
        self.live_schedule = LiveRewriteSchedule(
            self.config.live_rewrite_min_characters, self.config.live_rewrite_interval_seconds
        )
        self.live_final_entry = None
        self.live_final_text = ""
        self.live_revision = 0
        self.live_last_model = ""
        self.conversation_workspace.live_draft_follower.follow(snap=True)

    def _live_draft_edited(self, _buffer) -> None:
        """Protect edits made while a provider is preparing an older draft snapshot."""
        if not self.live_updating:
            self.live_revision += 1

    def _maybe_live_rewrite(self, text: str, *, final: bool = False) -> None:
        """Coalesce speech snapshots, keeping at most one bounded rewrite in flight."""
        schedule = self.live_schedule
        if schedule is None or self.config.incognito_mode or self.shutting_down:
            return
        if schedule.paused:
            if final:
                self._save_live_draft(stale=True)
            return
        if (
            final
            and not schedule.in_flight
            and ((schedule.last_final and text.strip() == schedule.last_text) or schedule.failed)
        ):
            self._save_live_draft(stale=schedule.failed)
            return
        snapshot = schedule.take(text, time.monotonic(), final=final)
        if snapshot is None:
            return
        try:
            prompt = live_prompt(
                self.live_config,
                snapshot,
                self.conversation_workspace.live_draft(),
                final=final,
                prompts=self.live_prompts,
            )
            client = self._new_rewrite_client(self.live_config)
        except Exception as error:
            schedule.finish(False)
            self.conversation_workspace.live_draft_status.set_label(str(error))
            if final:
                self._save_live_draft(stale=True)
            return
        self.live_rewrite_client = client
        self.conversation_workspace.live_draft_status.set_label(
            "Reconciling the final transcript…" if final else "Updating live draft · provisional recognition…"
        )
        revision = self.live_revision
        session = self.live_session_identifier
        config = self.live_config

        def run():
            """Compute one whole draft, never streaming half a template into the editor."""
            try:
                if isinstance(client, LiteLLMClient):
                    model = client.resolve_model(config.litellm_model)
                    result = client.transform(
                        prompt, self.codex_workspace, model, max_output_characters=MAX_REWRITE_CHARACTERS
                    )
                else:
                    selected = select_model(client.list_models(), config.rewrite_model or config.codex_model)
                    if config.rewrite_fast_mode and selected.fast_tier is None:
                        raise ValueError("Fast mode unavailable")
                    model = selected.identifier
                    result = client.transform(
                        prompt,
                        self.codex_workspace,
                        model,
                        max_output_characters=MAX_REWRITE_CHARACTERS,
                        effort=selected.rewrite_effort,
                        service_tier=selected.fast_tier if config.rewrite_fast_mode else "default",
                    )
            except Exception:
                result, model = "", ""
            finally:
                client.close()
            GLib.idle_add(self._live_rewrite_finished, session, client, revision, result, model)

        threading.Thread(target=run, name="live-rewrite", daemon=True).start()

    def _live_rewrite_finished(self, session, client, revision, result, model) -> bool:
        """Reject stale sessions and preserve edits typed after this request began."""
        if (
            self.shutting_down
            or self.config.incognito_mode
            or session != self.live_session_identifier
            or client is not self.live_rewrite_client
            or self.live_schedule is None
        ):
            return GLib.SOURCE_REMOVE
        self.live_rewrite_client = None
        self.live_schedule.finish(bool(result))
        if result and revision == self.live_revision:
            self.live_updating = True
            self.conversation_workspace.show_live_draft(
                result,
                "Live draft · final transcript reconciled"
                if self.live_schedule.last_final
                else "Live draft · provisional until Stop",
            )
            self.live_updating = False
            self.live_last_model = model
        elif result:
            self.live_schedule.last_text = ""
            self.conversation_workspace.live_draft_status.set_label("Your edit kept · awaiting next update")
        else:
            self.conversation_workspace.live_draft_status.set_label(
                "Live rewrite paused · provider failed; your draft is kept"
            )
        if self.live_final_entry is not None:
            self._maybe_live_rewrite(self.live_final_text, final=True)
        return GLib.SOURCE_REMOVE

    def _save_live_draft(self, *, stale: bool = False) -> None:
        """Save one final document with its source; only a complete final rewrite may auto-copy."""
        identifier = self.live_final_entry
        workspace = self.conversation_workspace
        text = workspace.live_draft()
        config = self.live_config
        paused = self.live_schedule is not None and self.live_schedule.paused
        if identifier is None:
            return
        review_phase = "ready"
        review_message = (
            "Paused draft · not reconciled." if paused else "Partial draft · final update failed." if stale else ""
        )
        try:
            entry = self.history_store.find(identifier)
            instruction = "Live rewrite · " + dict(TEMPLATE_CHOICES)[config.live_rewrite_template]
            if paused:
                instruction += " (paused; final transcript not reconciled)"
            elif stale:
                instruction += " (partial draft; final update failed)"
            if text.strip():
                self.conversation_store.append(identifier, instruction, text, self.live_last_model or "live-draft")
            viewing_live = workspace.viewing_live
            workspace.finish_live()
            if viewing_live or (workspace.entry is not None and workspace.entry.identifier == identifier):
                workspace.show_conversation(entry, self.conversation_store.replies(identifier))
            workspace.set_busy(
                False,
                "Paused draft saved. Review it against the final transcript."
                if paused
                else "Live draft saved. Final update failed; review the remaining gaps."
                if stale
                else "Live draft copied."
                if config.auto_copy_rewrite
                else "Live draft saved.",
            )
            workspace.refresh_history()
        except Exception:
            review_phase = "review-error"
            review_message = "Could not save the live draft. Open Mluva to copy it."
            workspace.finish_live()
            workspace.show_transient(self.live_final_text, text)
            workspace.set_busy(
                False, "Could not save the live draft. Your original and draft remain available to copy."
            )
        else:
            if text.strip() and config.auto_copy_rewrite and not stale and not paused:
                try:
                    deliver_text(text, auto_paste=False)
                except Exception:
                    review_message = "Automatic copy failed. Use Copy."
                    workspace.set_busy(False, "Live draft saved. Automatic copy failed; use the Copy icon.")
        self.live_schedule = None
        self.live_final_entry = None
        self.live_session_identifier = None
        self._publish_review(identifier, review_phase, review_message)

    def _cancel_live_rewrite(self) -> None:
        """Invalidate pending callbacks before cancelling provider work off GTK."""
        client = self.live_rewrite_client
        self.live_rewrite_client = None
        self.live_schedule = None
        self.live_session_identifier = None
        self.live_final_entry = None
        if client is not None:
            threading.Thread(target=client.cancel, name="cancel-live-rewrite", daemon=True).start()
        if self.conversation_workspace is not None:
            self.conversation_workspace.live_draft_box.set_visible(False)
            self.conversation_workspace.set_busy(False, "")

    def _pause_live_rewrite(self) -> None:
        """Stop model requests while keeping an edited draft available and recoverable at Stop."""
        client, self.live_rewrite_client = self.live_rewrite_client, None
        if client is not None:
            threading.Thread(target=client.cancel, name="pause-live-rewrite", daemon=True).start()
        if self.live_schedule is not None:
            self.live_schedule.paused = True
            self.live_schedule.in_flight = False
            self.conversation_workspace.live_draft_status.set_label("Live rewrite paused · draft kept")

    def _new_rewrite_client(self, config: AppConfig | None = None, **options):
        """Freeze an isolated provider client without requiring another provider's credentials."""
        config = config or self.config
        if config.rewrite_provider == "litellm":
            return LiteLLMClient(config.litellm_base_url, config.litellm_api_key_env, config.litellm_model)
        return CodexAppServerClient(**options)

    def _apply_workspace_settings(self, changes: dict) -> bool:
        """Persist UI choices to the shared dotfile and replace idle provider transports."""
        config = replace(self.config, **changes)
        changed = {name for name in changes if getattr(self.config, name) != getattr(config, name)}
        if not changed:
            return True
        presentation = {
            "history_sidebar_visible",
            "time_format",
            "widget_position",
            "show_copy_action",
            "show_save_action",
            "smooth_scrolling",
            "scroll_duration_ms",
            "scroll_lookahead_lines",
            "review_timeout_seconds",
        }
        live = {
            "live_rewrite_enabled",
            "live_rewrite_template",
            "live_rewrite_custom_instructions",
            "live_rewrite_min_characters",
            "live_rewrite_interval_seconds",
        }
        if changed <= presentation | live:
            recording = self.recorder is not None and self.recorder.process is not None
            if changed & live and (
                self.capture_preparing
                or self.capture_processing
                or self.live_final_entry is not None
                or self.rewrite_client is not None
                or (recording and (self.pending_incognito or self.pending_mode != "dictation"))
            ):
                return False
            try:
                save_config(config, self.config_path)
            except OSError:
                return False
            self.config = config
            self.conversation_workspace.set_config(config)
            for page in (self.history_page, self.meeting_page):
                if page is not None:
                    page.time_format = config.time_format
                    if "time_format" in changed:
                        page.refresh()
            self._sync_live_mode()
            if recording and changed & live:
                if isinstance(self.realtime_session, BatchPreviewSession):
                    self.realtime_session.set_preview_enabled(config.live_rewrite_enabled)
                if config.live_rewrite_enabled:
                    self._start_live_rewrite(preserve_draft=True)
                    if self.realtime_session is not None:
                        self._maybe_live_rewrite(self.realtime_session.snapshot().display_text)
                else:
                    self._pause_live_rewrite()
                self._configure_realtime_provider()
            elif not (recording or self.capture_preparing or self.capture_processing):
                self._synchronize_workflow_config()
                if changed & live:
                    self._configure_realtime_provider()
            if self.overlay_review_identifier is not None and self.rewrite_client is None:
                self._publish_review(self.overlay_review_identifier)
            return True
        if self.capture_preparing or self.capture_processing or (self.recorder and self.recorder.process):
            return False
        if self.rewrite_client is not None or self.live_schedule is not None:
            return False
        try:
            speech = transcription_client(config)
        except RuntimeError:
            speech = None
        try:
            save_config(config, self.config_path)
        except OSError:
            return False
        self._cancel_titles()
        if self.model_catalog_client is not None:
            client = self.model_catalog_client
            self.model_catalog_client = None
            threading.Thread(target=client.close, daemon=True).start()
        self.config = config
        if speech is None:
            if self.workflow is not None:
                self.workflow.codex.close()
            self.workflow = None
        elif self.workflow is not None:
            self.workflow.elevenlabs = speech
            self.workflow.codex.close()
            self.workflow.codex = self._new_rewrite_client()
        else:
            self.workflow = DictationWorkflow(
                config,
                speech,
                self._new_rewrite_client(),
                self.history_store,
                self.codex_workspace,
                self.personalization_store,
                self.diagnostics_store,
            )
        self._configure_realtime_provider()
        self._synchronize_workflow_config()
        self.conversation_workspace.set_config(config)
        if self.rewrite_settings is not None:
            self.rewrite_settings.models = []
            self.rewrite_settings.set_config(config)
        self._sync_live_mode()
        self._reset_record_button()
        if self.workflow is None:
            self.record_button.set_sensitive(False)
            self._set_status("Speech provider needs credentials. Editing and rewriting remain available.")
        return True

    def _configure_realtime_provider(self) -> None:
        """Keep Scribe streaming native and enable batch previews only for explicit live mode."""
        config = self.config
        if config.transcription_provider == "elevenlabs":
            try:
                self.realtime_client = ElevenLabsRealtimeClient(api_key=elevenlabs_api_key())
            except RuntimeError:
                self.realtime_client = None
        else:
            self.realtime_client = BatchPreviewClient(
                lambda: transcription_client(config),
                self.codex_workspace.parent / "speech-previews",
                config.transcription_chunk_seconds,
                preview_enabled=config.live_rewrite_enabled,
            )

    def _request_rewrite(self, instruction: str) -> None:
        """Freeze the selected conversation before starting one background rewrite."""
        workspace = self.conversation_workspace
        if workspace is None or workspace.entry is None:
            return
        if workspace.save_edits():
            self._begin_rewrite(workspace.entry.identifier, instruction)

    def _load_rewrite_models(self) -> None:
        """Discover installed models only when requested, without sending conversation text."""
        if self.shutting_down or self.model_catalog_client is not None or self.rewrite_settings is None:
            return
        client = self._new_rewrite_client(request_timeout_seconds=10)
        self.model_catalog_client = client
        self.rewrite_settings.set_loading()

        def load() -> None:
            """Keep catalog startup, errors and process cleanup off GTK."""
            try:
                models = client.list_models()
            except Exception:
                models = None
            finally:
                client.close()
            GLib.idle_add(self._rewrite_models_loaded, client, models)

        threading.Thread(target=load, name="rewrite-models", daemon=True).start()

    def _rewrite_models_loaded(self, client: CodexAppServerClient, models: list[CodexModel] | None) -> bool:
        """Discard stale discovery after shutdown and let the picker recover from offline Codex."""
        if self.shutting_down or client is not self.model_catalog_client:
            return GLib.SOURCE_REMOVE
        self.model_catalog_client = None
        if self.rewrite_settings is not None:
            self.rewrite_settings.set_models(models)
        return GLib.SOURCE_REMOVE

    def _save_rewrite_settings(self, model: str | None, fast: bool) -> None:
        """Persist rewrite-only preferences, restoring the controls when the write fails."""
        config = (
            replace(self.config, litellm_model=model)
            if self.config.rewrite_provider == "litellm"
            else replace(self.config, rewrite_model=model, rewrite_fast_mode=fast)
        )
        try:
            save_config(config, self.config_path)
        except OSError:
            if self.rewrite_settings is not None:
                self.rewrite_settings.set_config(self.config)
                self.rewrite_settings.status.set_label("Could not save rewrite settings. Your previous choices remain.")
            return
        self.config = config
        self._synchronize_workflow_config()
        if self.rewrite_settings is not None:
            self.rewrite_settings.set_config(config)

    def _begin_rewrite(self, identifier: str, instruction: str) -> None:
        """Run either rewrite surface against one explicit conversation, regardless of selection."""
        workspace = self.conversation_workspace
        if workspace is None or self.shutting_down:
            return
        if self.live_schedule is not None and self.live_final_entry is not None:
            return
        if self.rewrite_client is not None:
            if self.overlay_review_identifier == identifier and self.rewrite_identifier != identifier:
                self._publish_review(identifier, message="Another note is rewriting. Try again shortly.")
            return
        if self.capture_preparing or self.capture_processing or (self.recorder is not None and self.recorder.process):
            return
        if self.config.incognito_mode:
            workspace.set_busy(False, "Rewriting is unavailable in Incognito.")
            return
        try:
            entry = self.history_store.find(identifier)
            prompt = rewrite_prompt(
                entry,
                self.conversation_store.replies(entry.identifier),
                instruction,
                self.conversation_store.source_text(entry),
            )
        except (KeyError, ValueError):
            workspace.set_busy(False, "This conversation cannot be rewritten. Reopen it or start with shorter text.")
            if self.overlay_review_identifier == identifier:
                self._publish_review(identifier, "review-error", "Open this note to review it.")
            return
        client = self._new_rewrite_client()
        self.rewrite_auto_copy = self.config.auto_copy_rewrite
        self.rewrite_client = client
        self.rewrite_identifier = identifier
        self.rewrite_draft = instruction if instruction == workspace.prompt_text() else None
        workspace.set_busy(True, "Rewriting…")
        workspace.set_rewrite_preview(identifier, "")
        if workspace.entry is not None and workspace.entry.identifier == identifier:
            workspace.scroll_to_latest()
        self._publish_review(identifier, "rewriting")
        threading.Thread(
            target=self._rewrite_worker,
            args=(
                client,
                entry.identifier,
                instruction,
                prompt,
                (
                    self.config.litellm_model
                    if self.config.rewrite_provider == "litellm"
                    else self.config.rewrite_model or self.config.codex_model
                ),
                self.config.rewrite_fast_mode if self.config.rewrite_provider == "codex" else False,
                time.monotonic(),
            ),
            name="conversation-rewrite",
            daemon=True,
        ).start()

    def _publish_review(self, identifier: str, phase: str = "ready", message: str = "") -> None:
        """Offer the completed note to the shell for timed, deliberate review."""
        if self.config.incognito_mode:
            self._dismiss_review()
            return
        try:
            entry = self.history_store.find(identifier)
            replies = self.conversation_store.replies(identifier)
        except KeyError:
            self._dismiss_review()
            if self.rewrite_identifier == identifier:
                self._cancel_rewrite()
            return
        self._clear_overlay_timeout()
        self.overlay_review_identifier = identifier
        source = self.conversation_store.source_text(entry, delivered_fallback=True)
        preview = replies[-1].text if replies else source
        workspace = self.conversation_workspace
        if identifier == self.live_final_entry and workspace is not None:
            phase = "rewriting"
            preview = workspace.live_draft()
            message = "Reconciling the final transcript…"
        if phase == "rewriting" and workspace is not None and workspace.rewrite_preview_identifier == identifier:
            preview = workspace.rewrite_preview_text or preview
        if self.recording_overlay_publisher is not None:
            self.recording_overlay_publisher.publish(
                RecordingOverlayState(
                    phase=phase,
                    preview=preview,
                    review_identifier=identifier,
                    review_options=tuple((style.identifier, style.name) for style in self.personalization_store.styles),
                    message=message,
                    review_timeout_seconds=self.config.review_timeout_seconds,
                    show_copy_action=self.config.show_copy_action,
                    smooth_scrolling=_smooth_motion_enabled(self.config),
                    scroll_duration_ms=self.config.scroll_duration_ms,
                    scroll_lookahead_lines=self.config.scroll_lookahead_lines,
                    widget_position=self.config.widget_position,
                )
            )

    def _dismiss_review(self) -> None:
        """Erase the widget's cached note without deleting history or changing the clipboard."""
        self.overlay_review_identifier = None
        self._clear_overlay_timeout()
        if self.recording_overlay_publisher is not None:
            self.recording_overlay_publisher.clear()

    def _review_action(self, _action: Gio.SimpleAction, parameters: GLib.Variant) -> None:
        """Accept only actions for the note currently offered by this application instance."""
        operation, identifier, option = parameters.unpack()
        if (
            self.shutting_down
            or self.config.incognito_mode
            or not identifier
            or identifier != self.overlay_review_identifier
        ):
            return
        if operation == "dismiss":
            self._dismiss_review()
            return
        try:
            entry = self.history_store.find(identifier)
        except KeyError:
            self._dismiss_review()
            return
        if operation == "rewrite":
            presets = {
                "polish": self.prompt_store.read("rewrite-polish").text,
                "structure": self.prompt_store.read("rewrite-structure").text,
            }
            style = self.personalization_store.style(option)
            instruction = presets.get(option) or (style.instructions if style is not None else None)
            if instruction is not None:
                self._begin_rewrite(identifier, instruction)
        elif operation == "cancel" and identifier in {self.rewrite_identifier, self.live_final_entry}:
            self._cancel_rewrite()
        elif operation == "copy" and identifier not in {self.rewrite_identifier, self.live_final_entry}:
            replies = self.conversation_store.replies(identifier)
            try:
                deliver_text(
                    replies[-1].text
                    if replies
                    else self.conversation_store.source_text(entry, delivered_fallback=True),
                    auto_paste=False,
                )
            except Exception:
                self._publish_review(identifier, "review-error", "Could not copy. Try again.")
            else:
                self._publish_review(identifier, message="Copied")
        elif operation == "open":
            self.activate()
            self._navigate_to_page("capture")
            self.conversation_workspace.show_conversation(entry, self.conversation_store.replies(identifier))
            if self.live_final_entry == identifier:
                self.conversation_workspace.live_draft_text.grab_focus()
            else:
                self.conversation_workspace.prompt.grab_focus()
            self._dismiss_review()

    def _rewrite_worker(
        self,
        client: CodexAppServerClient,
        identifier: str,
        instruction: str,
        prompt: str,
        configured_model: str | None,
        fast_mode: bool,
        started_at: float,
    ) -> None:
        """Resolve and run the model away from GTK, leaving durable writes to the completion gate."""
        parts: list[str] = []
        last_update = 0.0
        first_text_seconds: float | None = None
        failure_message = "Rewrite failed. Check the selected provider, then try again. Your text is safe."

        def progress(delta: str) -> None:
            """Send bounded snapshots at most twenty times a second without blocking the model."""
            nonlocal last_update, first_text_seconds
            if not delta:
                return
            parts.append(delta)
            now = time.monotonic()
            if first_text_seconds is None:
                first_text_seconds = now - started_at
            if now - last_update >= 0.05:
                last_update = now
                GLib.idle_add(self._rewrite_progress, client, identifier, "".join(parts))

        try:
            model = (
                CodexModel(configured_model, configured_model, configured_model, True)
                if isinstance(client, LiteLLMClient) and configured_model
                else select_model(client.list_models(), configured_model)
            )
            if fast_mode and model.fast_tier is None:
                failure_message = "Fast mode is unavailable for this model. Turn it off or choose another model."
                raise ValueError("Unsupported rewrite service tier")
            result = client.transform(
                prompt,
                self.codex_workspace,
                model.identifier,
                max_output_characters=MAX_REWRITE_CHARACTERS,
                on_delta=progress,
                effort=model.rewrite_effort,
                service_tier=model.fast_tier if fast_mode else "default",
            )
        except Exception:
            GLib.idle_add(self._rewrite_finished, client, identifier, instruction, "", "", None, failure_message)
        else:
            GLib.idle_add(
                self._rewrite_finished, client, identifier, instruction, result, model.identifier, first_text_seconds
            )
        finally:
            client.close()

    def _rewrite_progress(self, client: CodexAppServerClient, identifier: str, text: str) -> bool:
        """Display only the active stream, discarding queued updates after cancellation or privacy changes."""
        if (
            self.shutting_down
            or self.config.incognito_mode
            or client is not self.rewrite_client
            or identifier != self.rewrite_identifier
        ):
            return GLib.SOURCE_REMOVE
        workspace = self.conversation_workspace
        if workspace is not None:
            workspace.set_rewrite_preview(identifier, text)
        if self.overlay_review_identifier == identifier and self.recording_overlay_publisher is not None:
            self.recording_overlay_publisher.publish(
                RecordingOverlayState(
                    phase="rewriting",
                    preview=text,
                    review_identifier=identifier,
                    review_timeout_seconds=self.config.review_timeout_seconds,
                    show_copy_action=self.config.show_copy_action,
                    smooth_scrolling=_smooth_motion_enabled(self.config),
                    scroll_duration_ms=self.config.scroll_duration_ms,
                    scroll_lookahead_lines=self.config.scroll_lookahead_lines,
                    widget_position=self.config.widget_position,
                )
            )
        return GLib.SOURCE_REMOVE

    def _rewrite_finished(
        self,
        client: CodexAppServerClient,
        identifier: str,
        instruction: str,
        result: str,
        model: str,
        first_text_seconds: float | None = None,
        failure_message: str = "Rewrite failed. Check the selected provider, then try again. Your text is safe.",
    ) -> bool:
        """Commit a complete reply only while its request, source and privacy policy remain valid."""
        if self.shutting_down or client is not self.rewrite_client:
            return GLib.SOURCE_REMOVE
        self.rewrite_client = None
        self.rewrite_identifier = None
        workspace = self.conversation_workspace
        if workspace is None:
            return GLib.SOURCE_REMOVE
        workspace.clear_rewrite_preview()
        if self.config.incognito_mode:
            workspace.set_busy(False, "Rewrite discarded because Incognito is enabled.")
            return GLib.SOURCE_REMOVE
        if not result:
            workspace.set_busy(False, failure_message)
            if self.overlay_review_identifier == identifier:
                self._publish_review(identifier, "review-error", "Rewrite failed. Try again or open the note.")
            return GLib.SOURCE_REMOVE
        try:
            entry = self.history_store.find(identifier)
            self.conversation_store.append(identifier, instruction, result, model)
        except Exception:
            workspace.set_busy(False, "Could not save the rewrite. The conversation may have been deleted.")
            if self.overlay_review_identifier == identifier:
                self._publish_review(identifier, "review-error", "Could not save the rewrite. Original unchanged.")
            return GLib.SOURCE_REMOVE
        if workspace.entry is not None and workspace.entry.identifier == identifier:
            if workspace.prompt_text() == self.rewrite_draft:
                workspace.prompt.get_buffer().set_text("")
            workspace.show_conversation(entry, self.conversation_store.replies(identifier))
        timing = f" · first text {first_text_seconds:.1f} s" if first_text_seconds is not None else ""
        copied = False
        if getattr(self, "rewrite_auto_copy", self.config.auto_copy_rewrite):
            try:
                deliver_text(result, auto_paste=False)
                copied = True
            except Exception:
                workspace.set_busy(False, f"Rewrite saved{timing}. Automatic copy failed; use the Copy icon.")
        if copied or not getattr(self, "rewrite_auto_copy", self.config.auto_copy_rewrite):
            workspace.set_busy(False, f"Rewrite {'copied' if copied else 'saved'}{timing}.")
        workspace.refresh_history()
        if self.overlay_review_identifier == identifier:
            self._publish_review(identifier)
        return GLib.SOURCE_REMOVE

    def _cancel_rewrite(self) -> None:
        """Invalidate the pending result immediately, then stop its provider off the UI thread."""
        if self.live_schedule is not None and self.live_final_entry is not None:
            identifier = self.live_final_entry
            self._cancel_live_rewrite()
            self.conversation_workspace.finish_live()
            self.conversation_workspace.set_busy(False, "Live rewrite cancelled. Your original is safe.")
            self._publish_review(identifier, message="Live rewrite cancelled. Original kept.")
            return
        client = self.rewrite_client
        if client is None:
            return
        self.rewrite_client = None
        identifier = self.rewrite_identifier
        self.rewrite_identifier = None
        threading.Thread(target=client.cancel, name="cancel-rewrite", daemon=True).start()
        if self.conversation_workspace is not None:
            self.conversation_workspace.clear_rewrite_preview()
            self.conversation_workspace.set_busy(False, "Rewrite cancelled. Your original is safe.")
        if identifier is not None and self.overlay_review_identifier == identifier:
            self._publish_review(identifier)

    def _save_rewrite_prompt(self, instruction: str) -> None:
        """Name an explicitly supplied prompt using the existing saved-style store."""
        if not instruction.strip() or self.config.incognito_mode:
            return
        dialog = Adw.AlertDialog(heading="Save prompt", body="Give this rewrite prompt a short name.")
        name = Gtk.Entry(placeholder_text="Prompt name", activates_default=True)
        dialog.set_extra_child(name)
        dialog.add_response("cancel", "Cancel")
        dialog.add_response("save", "Save")
        dialog.set_default_response("save")
        dialog.set_response_appearance("save", Adw.ResponseAppearance.SUGGESTED)
        dialog.connect("response", self._prompt_name_chosen, name, instruction)
        dialog.present(self.window)

    def _prompt_name_chosen(self, _dialog: Adw.AlertDialog, response: str, name: Gtk.Entry, instruction: str) -> None:
        """Save a named prompt only after an explicit action and a fresh privacy check."""
        if response != "save" or self.config.incognito_mode:
            return
        try:
            if any(
                style.name.casefold() == name.get_text().strip().casefold()
                for style in self.personalization_store.styles
            ):
                raise ValueError("Choose a new prompt name.")
            self.personalization_store.save_style(name.get_text(), instruction)
        except (ValueError, OSError):
            self._show_toast("Could not save the prompt. Use a unique name and a shorter instruction.")
            return
        self._refresh_style_controls()
        self._show_toast("Prompt saved.")

    def _navigate_to_page(self, name: str) -> None:
        """Move the workspace to one rail or bottom-bar destination."""
        if self.page_stack is not None:
            self.page_stack.set_visible_child_name(name)

    def _visible_page_changed(self, *_args: object) -> None:
        """Keep the rail selection and the utility title aligned with the stack."""
        if self.page_stack is None:
            return
        child = self.page_stack.get_visible_child()
        name = self.page_stack.get_page(child).get_name() if child is not None else "capture"
        if self.navigation_rail is not None:
            self.navigation_rail.select_page(name)
        if self.page_title_label is not None:
            page = self.page_stack.get_page(child) if child is not None else None
            self.page_title_label.set_label(page.get_title() if page is not None and name != "capture" else "")

    def _build_capture_page(self) -> Adw.ToolbarView:
        """Make the full conversation primary and keep recording controls in reach."""
        page = Adw.ToolbarView()
        body = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        self.setup_callout = self._build_setup_callout()
        body.append(self.setup_callout)
        self.conversation_workspace = ConversationWorkspace(
            store=self.conversation_store,
            copy_text=self._copy_text,
            rewrite=self._request_rewrite,
            paste_text=self._start_pasted_conversation,
            open_archive=self._open_history,
            save_prompt=self._save_rewrite_prompt,
            cancel_rewrite=self._cancel_rewrite,
        )
        self.conversation_workspace.set_vexpand(True)
        self.rewrite_settings = RewriteSettings(self.config, self._load_rewrite_models, self._save_rewrite_settings)
        self.conversation_workspace.set_rewrite_settings(self.rewrite_settings)
        body.append(self.conversation_workspace)
        # Existing acceptance-gated recovery remains available for older drafts and advanced modes.
        body.append(self._build_output_section())
        page.set_content(body)
        self.status_label = Gtk.Label(
            xalign=0,
            ellipsize=Pango.EllipsizeMode.END,
            valign=Gtk.Align.END,
            accessible_role=Gtk.AccessibleRole.STATUS,
        )
        self.status_label.add_css_class("ml-capture-status")
        self.capture_status_title = Gtk.Label(label="Ready to dictate", xalign=0)
        self.capture_status_title.add_css_class("heading")
        self.capture_action_bar = self._build_capture_action_bar()
        self.conversation_workspace.live_draft_text.get_buffer().connect("changed", self._live_draft_edited)
        self.conversation_workspace.set_config(self.config)
        self.conversation_workspace.set_capture_controls(self.capture_action_bar)
        self.settings_dialog = self._build_settings_dialog()
        self._refresh_style_controls()
        self._update_capture_status_rows()
        return page

    def _build_setup_callout(self) -> Gtk.Revealer:
        """Collapse setup and runtime errors into one compact dismissible strip."""
        revealer = Gtk.Revealer(reveal_child=False)
        card = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=SPACE_3)
        card.add_css_class("vs-callout")
        set_margins(card, SPACE_3)
        icon = Gtk.Image.new_from_icon_name("dialog-warning-symbolic")
        icon.add_css_class("warning")
        icon.set_valign(Gtk.Align.START)
        card.append(icon)
        copy = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=SPACE_1, hexpand=True)
        self.setup_callout_title = Gtk.Label(label="Setup required", xalign=0)
        self.setup_callout_title.add_css_class("vs-callout-title")
        copy.append(self.setup_callout_title)
        self.setup_callout_body = Gtk.Label(xalign=0, wrap=True)
        self.setup_callout_body.add_css_class("vs-callout-body")
        copy.append(self.setup_callout_body)
        card.append(copy)
        self.setup_callout_button = Gtk.Button(label="Dismiss", valign=Gtk.Align.CENTER)
        self.setup_callout_button.connect("clicked", self._callout_action_clicked)
        card.append(self.setup_callout_button)
        revealer.set_child(card)
        return revealer

    def _build_output_section(self) -> Gtk.Box:
        """Build the result editor and reveal it only when recoverable text exists."""
        card, body = card_box()
        self.output_section = card
        heading = Gtk.Label(label="Latest result", xalign=0)
        heading.add_css_class("heading")
        body.append(heading)
        self.command_source_label = Gtk.Label(xalign=0, wrap=True, selectable=True)
        self.command_source_label.add_css_class("caption")
        self.command_source_label.set_visible(False)
        body.append(self.command_source_label)
        self.output_view = Gtk.TextView(editable=True, wrap_mode=Gtk.WrapMode.WORD_CHAR)
        self.output_view.set_tooltip_text("Recognized text and unresolved Command or Notes output")
        self.output_buffer = self.output_view.get_buffer()
        self.output_buffer.connect("changed", self._scratchpad_text_changed)
        self.output_buffer.connect("changed", self._output_visibility_changed)
        output_scroll = Gtk.ScrolledWindow(min_content_height=RESULT_EDITOR_MIN_HEIGHT)
        output_scroll.set_child(self.output_view)
        body.append(output_scroll)
        self.command_actions = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=SPACE_2)
        self.accept_command_button = Gtk.Button(label="Apply command", hexpand=True)
        self.accept_command_button.add_css_class("suggested-action")
        self.accept_command_button.connect("clicked", self._accept_command_preview)
        self.command_actions.append(self.accept_command_button)
        discard_command = Gtk.Button(label="Discard")
        discard_command.connect("clicked", self._discard_command_preview)
        self.command_actions.append(discard_command)
        self.command_actions.set_visible(False)
        body.append(self.command_actions)
        self.scratchpad_actions = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=SPACE_2)
        copy_draft = Gtk.Button(label="Copy and resolve", hexpand=True)
        copy_draft.add_css_class("suggested-action")
        copy_draft.connect("clicked", self._copy_and_resolve_scratchpad)
        self.scratchpad_actions.append(copy_draft)
        delete_draft = Gtk.Button(label="Delete draft")
        delete_draft.add_css_class("destructive-action")
        delete_draft.connect("clicked", self._confirm_delete_current_scratchpad)
        self.scratchpad_actions.append(delete_draft)
        self.scratchpad_actions.set_visible(False)
        body.append(self.scratchpad_actions)
        card.set_visible(False)
        return card

    def _build_capture_action_bar(self) -> Gtk.Box:
        """Keep recording status and a compact start/stop action outside the transcript scroll."""
        dock = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=SPACE_3)
        dock.add_css_class("ml-recording-dock")
        set_margins(dock, SPACE_4)
        dock.set_margin_top(SPACE_2)
        status = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=SPACE_1, hexpand=True, valign=Gtk.Align.END)
        self.capture_status_title.set_visible(False)
        status.append(self.capture_status_title)
        status.append(self.status_label)
        self.capture_action_hint = Gtk.Label(xalign=0, wrap=True)
        self.capture_action_hint.add_css_class("caption")
        self.capture_action_hint.set_visible(False)
        status.append(self.capture_action_hint)
        dock.append(status)
        self.record_button = Gtk.Button(valign=Gtk.Align.CENTER)
        self.record_button.set_tooltip_text("Start or stop dictation · F9")
        self.record_button.add_css_class("suggested-action")
        self.record_button.add_css_class("ml-record-toggle")
        set_button_content(self.record_button, "audio-input-microphone-symbolic", "Dictate")
        self.record_button.connect("clicked", self._toggle_recording)
        dock.append(self.record_button)
        live_control = Gtk.Box(valign=Gtk.Align.CENTER, css_classes=["linked"])
        self.live_mode_switch = Gtk.ToggleButton(label="Live rewrite", active=self.config.live_rewrite_enabled)
        self.live_mode_switch.connect("toggled", self._live_mode_changed)
        live_control.append(self.live_mode_switch)
        self.live_mode_menu = Gtk.MenuButton(icon_name="pan-down-symbolic")
        popover = Gtk.Popover()
        choices = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        set_margins(choices, 8)
        group = None
        for template, label in TEMPLATE_CHOICES:
            choice = Gtk.CheckButton(label=label)
            if group is not None:
                choice.set_group(group)
            else:
                group = choice
            choice.connect("toggled", self._live_template_changed, template)
            self.live_template_buttons[template] = choice
            choices.append(
                prompt_control(
                    choice,
                    label,
                    lambda key=template: (self.live_mode_menu.popdown(), self._open_prompt_editor("live-" + key)),
                )
            )
        popover.set_child(choices)
        self.live_mode_menu.set_popover(popover)
        live_control.append(self.live_mode_menu)
        self._sync_live_mode()
        dock.append(live_control)
        return dock

    def _build_settings_dialog(self) -> Adw.PreferencesDialog:
        """Move infrequent capture, audio, privacy, and diagnostic controls off the primary surface."""
        dialog = Adw.PreferencesDialog(title="Mluva settings")
        dialog.set_search_enabled(True)
        self.workspace_settings_pages = (
            WorkspaceSettings(self.config, self._apply_workspace_settings, self._open_prompt_editor),
            ProviderSettings(self.config, self._apply_workspace_settings),
        )
        for page in self.workspace_settings_pages:
            dialog.add(page)

        capture_page = Adw.PreferencesPage(
            name="capture",
            title="Capture",
            icon_name="audio-input-microphone-symbolic",
        )
        defaults = Adw.PreferencesGroup(
            title="Capture defaults",
            description="These choices apply to the next capture and remain unchanged while recording.",
        )
        self.mode = Adw.ComboRow(title="Capture mode")
        self.mode.set_model(Gtk.StringList.new(list(CAPTURE_MODE_LABELS)))
        self.mode.set_selected(CAPTURE_MODE_IDS.index(self.config.default_mode))
        self.mode.set_subtitle(capture_mode_description(self.mode.get_selected()))
        self.mode.set_tooltip_text(CAPTURE_MODE_TOOLTIP)
        self.mode.connect("notify::selected", self._mode_changed)
        self.mode.connect("notify::selected", self._capture_configuration_changed)
        defaults.add(self.mode)
        self.language = Adw.ComboRow(title="Transcription language")
        self.language_codes = [code for code, _name in TRANSCRIPTION_LANGUAGE_OPTIONS]
        language_names = [name for _code, name in TRANSCRIPTION_LANGUAGE_OPTIONS]
        if self.config.language_code not in self.language_codes:
            self.language_codes.append(self.config.language_code)
            language_names.append(f"Custom ISO code ({self.config.language_code})")
        self.language.set_model(Gtk.StringList.new(language_names))
        self.language.set_selected(self.language_codes.index(self.config.language_code))
        self.language.set_subtitle("Auto-detect lets the selected speech provider choose the language")
        self.language.connect("notify::selected", self._language_changed)
        self.language.connect("notify::selected", self._capture_configuration_changed)
        defaults.add(self.language)
        self.output_style = Adw.ComboRow(title="Output style")
        self.output_style.connect("notify::selected", self._output_style_changed)
        self.output_style.connect("notify::selected", self._capture_configuration_changed)
        defaults.add(self.output_style)
        self.output_style_instructions = Adw.ActionRow(title="Selected style instructions")
        self.output_style_instructions.set_subtitle_lines(3)
        defaults.add(self.output_style_instructions)
        output_style_management = Adw.ActionRow(
            title="Custom output modes",
            subtitle="Create or edit reusable instructions in Personalization",
        )
        manage_output_styles = Gtk.Button(label="Manage", valign=Gtk.Align.CENTER)
        manage_output_styles.connect("clicked", self._show_personalization_page)
        output_style_management.add_suffix(manage_output_styles)
        defaults.add(output_style_management)
        capture_page.add(defaults)

        shortcut = Adw.PreferencesGroup(
            title="Global shortcut",
            description="Press once to start and again to stop. F9 is the practical default.",
        )
        self.global_recording_key = Adw.ComboRow(title="Recording key")
        self.global_recording_key.set_model(Gtk.StringList.new(list(FUNCTION_KEY_OPTIONS)))
        self.global_recording_key.set_selected(FUNCTION_KEY_OPTIONS.index(self.config.global_recording_key))
        self.global_recording_key.set_subtitle(GLOBAL_RECORDING_KEY_GUIDANCE)
        self.global_recording_key.set_tooltip_text(GLOBAL_RECORDING_KEY_TOOLTIP)
        self.global_recording_key.connect("notify::selected", self._global_recording_key_changed)
        self.global_recording_key.connect("notify::selected", self._capture_configuration_changed)
        shortcut.add(self.global_recording_key)
        self.global_shortcut_status_row = Adw.ActionRow(
            title="Desktop approval",
            subtitle=f"Requesting {self.config.global_recording_key} through the desktop portal…",
        )
        shortcut.add(self.global_shortcut_status_row)
        self.rewrite_shortcut_status_row = Adw.ActionRow(
            title="Open latest conversation",
            subtitle="Shift+F9 needs desktop approval. The shell menu also opens your latest dictation.",
        )
        shortcut.add(self.rewrite_shortcut_status_row)
        capture_page.add(shortcut)

        behavior = Adw.PreferencesGroup(title="Behavior")
        self.remember_application_switch = Adw.SwitchRow(
            title=maturity_title("application_memory", "Remember mode and style per application")
        )
        self.remember_application_switch.set_subtitle("Uses a local executable identity that is never sent to Codex")
        self.remember_application_switch.set_active(self.config.remember_per_application)
        self.remember_application_switch.connect("notify::active", self._general_setting_changed)
        behavior.add(self.remember_application_switch)
        self.cleanup_switch = Adw.SwitchRow(title=maturity_title("faithful_cleanup", "Faithful cleanup"))
        self.cleanup_switch.set_subtitle(
            "Remove obvious filler and repair punctuation through the selected rewrite provider"
        )
        behavior.add(self.cleanup_switch)
        self.automatic_titles_switch = Adw.SwitchRow(
            title=maturity_title("conversations", "Automatic conversation titles")
        )
        self.automatic_titles_switch.set_subtitle(
            "Send an excerpt of each new conversation to the rewrite provider for a title. "
            "A local label is used if unavailable."
        )
        self.automatic_titles_switch.set_active(self.config.automatic_titles)
        self.automatic_titles_switch.connect("notify::active", self._automatic_titles_changed)
        behavior.add(self.automatic_titles_switch)
        self.spoken_commands_switch = Adw.SwitchRow(title=maturity_title("spoken_structure"))
        self.spoken_commands_switch.set_subtitle(
            "Apply explicit punctuation, new line, new paragraph, and scratch-that commands"
        )
        self.spoken_commands_switch.set_active(self.config.spoken_commands_enabled)
        self.spoken_commands_switch.connect("notify::active", self._general_setting_changed)
        behavior.add(self.spoken_commands_switch)
        auto_paste_capability = feature_capability("automatic_paste")
        self.auto_paste_switch = Adw.SwitchRow(title=maturity_title("automatic_paste"))
        if self.focus_tracker is None:
            self.auto_paste_switch.set_subtitle(
                "Experimental · GNOME accessibility is disabled or unavailable; completed text remains on the clipboard"
            )
            self.auto_paste_switch.set_sensitive(False)
        else:
            self.auto_paste_switch.set_subtitle(auto_paste_capability.summary)
        self.auto_paste_switch.set_active(self.config.auto_paste)
        self.auto_paste_switch.connect("notify::active", self._general_setting_changed)
        self.auto_paste_switch.connect("notify::active", self._capture_configuration_changed)
        behavior.add(self.auto_paste_switch)
        capture_page.add(behavior)
        dialog.add(capture_page)

        audio_page = Adw.PreferencesPage(name="audio", title="Audio", icon_name="audio-card-symbolic")
        audio_routing = Adw.PreferencesGroup(title="Audio routing")
        self.microphone_device = Adw.ComboRow(title="Microphone")
        audio_routing.add(self.microphone_device)
        self.system_audio_device = Adw.ComboRow(title=maturity_title("meeting_mode", "Meeting system output"))
        self.system_audio_device.set_subtitle("Meeting captures all audio playing through the selected sink")
        audio_routing.add(self.system_audio_device)
        refresh_row = Adw.ActionRow(
            title="PipeWire device snapshot",
            subtitle="Refresh reads node metadata only; it never opens or records a device",
        )
        self.refresh_audio_button = Gtk.Button(label="Refresh", valign=Gtk.Align.CENTER)
        self.refresh_audio_button.connect("clicked", self._refresh_audio_devices)
        refresh_row.add_suffix(self.refresh_audio_button)
        audio_routing.add(refresh_row)
        self._populate_audio_device_rows()
        self.microphone_device.connect("notify::selected", self._audio_device_changed)
        self.system_audio_device.connect("notify::selected", self._audio_device_changed)
        audio_page.add(audio_routing)
        dialog.add(audio_page)

        privacy_page = Adw.PreferencesPage(
            name="privacy",
            title="Privacy",
            icon_name="changes-prevent-symbolic",
        )
        privacy = Adw.PreferencesGroup(title="Privacy and retention")
        self.incognito_switch = Adw.SwitchRow(title=maturity_title("recovery_privacy", "Incognito"))
        self.incognito_switch.set_subtitle(
            "New sessions write no local history or recovery audio. Recognition uses your selected speech provider."
        )
        self.incognito_switch.set_active(self.config.incognito_mode)
        self.incognito_switch.connect("notify::active", self._privacy_setting_changed)
        self.incognito_switch.connect("notify::active", self._capture_configuration_changed)
        privacy.add(self.incognito_switch)
        self.audio_retention = Adw.ComboRow(title=maturity_title("recovery_privacy", "Keep audio"))
        self.audio_retention.set_model(Gtk.StringList.new(["Never", "Failures", "Always"]))
        self.audio_retention.set_selected(
            (AudioRetentionPolicy.NEVER, AudioRetentionPolicy.FAILURES, AudioRetentionPolicy.ALWAYS).index(
                self.config.audio_retention_policy
            )
        )
        self.audio_retention.connect("notify::selected", self._privacy_setting_changed)
        self.audio_retention.connect("notify::selected", self._capture_configuration_changed)
        privacy.add(self.audio_retention)
        self.history_retention = Adw.ComboRow(title=maturity_title("recovery_privacy", "Keep history"))
        self.history_retention.set_model(Gtk.StringList.new(["Forever", "7 days", "30 days", "90 days"]))
        retention_values = (0, 7, 30, 90)
        selected_retention = (
            retention_values.index(self.config.history_retention_days)
            if self.config.history_retention_days in retention_values
            else 0
        )
        self.history_retention.set_selected(selected_retention)
        self.history_retention.connect("notify::selected", self._privacy_setting_changed)
        self.history_retention.connect("notify::selected", self._capture_configuration_changed)
        privacy.add(self.history_retention)
        privacy_page.add(privacy)
        dialog.add(privacy_page)
        self._apply_incognito_controls()

        dialog.add(self._build_feature_maturity_page())

        advanced_page = Adw.PreferencesPage(
            name="advanced",
            title="Advanced",
            icon_name="applications-engineering-symbolic",
        )
        diagnostics = Adw.PreferencesGroup(title="Diagnostics")
        diagnostics_row = Adw.ActionRow(
            title=maturity_title("diagnostics", "Privacy-safe timing export"),
            subtitle="Configuration and stage timings only; excludes audio, text, targets, credentials, and errors",
        )
        export_diagnostics = Gtk.Button(label="Export", valign=Gtk.Align.CENTER)
        export_diagnostics.connect("clicked", self._export_diagnostics)
        diagnostics_row.add_suffix(export_diagnostics)
        diagnostics.add(diagnostics_row)
        advanced_page.add(diagnostics)
        dialog.add(advanced_page)
        self.prompts_page = PromptsPage(self.prompt_store, self._open_prompt_editor)
        if self.config_load_error:
            self.prompts_page.group.set_description(self.config_load_error)
        dialog.add(self.prompts_page)
        self.settings_pages = (
            self.prompts_page,
            *self.workspace_settings_pages,
            capture_page,
            audio_page,
            privacy_page,
            advanced_page,
        )
        return dialog

    @staticmethod
    def _build_feature_maturity_page() -> Adw.PreferencesPage:
        """Render the complete acceptance matrix from the canonical capability registry."""
        page = Adw.PreferencesPage(
            name="feature-maturity",
            title="Feature maturity",
            icon_name="emblem-important-symbolic",
        )
        for maturity, title, description in (
            (
                FeatureMaturity.VERIFIED,
                FeatureMaturity.VERIFIED.label,
                "Part of the daily, end-to-end tested Omarchy workflow. Fedora has not been verified recently.",
            ),
            (
                FeatureMaturity.EXPERIMENTAL,
                "Experimental",
                (
                    "Available for testing, but not yet accepted as reliable. "
                    "Automated evidence alone does not promote it."
                ),
            ),
        ):
            group = Adw.PreferencesGroup(title=title, description=description)
            for capability in capabilities_with_maturity(maturity):
                row = Adw.ActionRow(title=capability.title, subtitle=capability.summary)
                row.add_suffix(maturity_badge(capability.maturity))
                group.add(row)
            page.add(group)
        return page

    def _install_navigation_breakpoint(self) -> None:
        """Move primary navigation to a bottom bar when the header becomes narrow."""
        if self.window is None:
            return
        condition = Adw.BreakpointCondition.parse(f"max-width: {COMPACT_LAYOUT_MAX_WIDTH}sp")
        breakpoint = Adw.Breakpoint.new(condition)
        breakpoint.connect("apply", self._apply_narrow_navigation)
        breakpoint.connect("unapply", self._apply_wide_navigation)
        self.window.add_breakpoint(breakpoint)

    def _apply_narrow_navigation(self, *_args: object) -> None:
        """Trade the wide left rail for compact bottom navigation."""
        if self.conversation_workspace is not None:
            self.conversation_workspace.split.set_collapsed(True)
            self.conversation_workspace.live_box.set_orientation(Gtk.Orientation.VERTICAL)
            self.conversation_workspace.split.set_show_sidebar(False)
        if self.navigation_rail is not None:
            self.navigation_rail.set_visible(False)
        self._sync_header_title()
        if self.navigation_bar is not None:
            self.navigation_bar.set_reveal(True)
        if self.recording_bar is not None:
            self.recording_bar.set_compact(True)

    def _apply_wide_navigation(self, *_args: object) -> None:
        """Restore the stable left rail and the utility title above the workspace."""
        if self.conversation_workspace is not None:
            self.conversation_workspace.split.set_collapsed(False)
            self.conversation_workspace.live_box.set_orientation(Gtk.Orientation.HORIZONTAL)
            self.conversation_workspace.split.set_show_sidebar(self.config.history_sidebar_visible)
        if self.navigation_rail is not None:
            self.navigation_rail.set_visible(True)
        self._sync_header_title()
        if self.navigation_bar is not None:
            self.navigation_bar.set_reveal(False)
        if self.recording_bar is not None:
            self.recording_bar.set_compact(False)

    def _sync_header_title(self, *_args: object) -> None:
        """Keep utility page titles out of the dictation workspace's quiet header."""
        if self.header_bar is None or self.window is None:
            return
        title = None if self.window.get_width() <= COMPACT_LAYOUT_MAX_WIDTH else self.page_title_label
        self.header_bar.set_title_widget(title)

    def _show_settings(self, _button: Gtk.Button) -> None:
        """Present all infrequent configuration in one searchable native dialog."""
        self.activate()
        if self.settings_dialog is not None and self.window is not None:
            for page in self.workspace_settings_pages:
                page.refresh_config(self.config)
            self.prompts_page.refresh()
            self.settings_dialog.present(self.window)

    def _show_commands(self) -> None:
        """Offer the current app actions through one searchable Ctrl-P panel."""
        self.activate()
        if self.command_palette is not None:
            self.command_palette.close()
            return
        if self.window.get_visible_dialog() is not None:
            return
        self.command_palette = CommandPalette(application_commands(self))
        self.command_palette.connect("closed", self._commands_closed)
        self.command_palette.present(self.window)

    def _commands_closed(self, _dialog: CommandPalette) -> None:
        """Allow the next invocation to reflect fresh recording and document state."""
        self.command_palette = None

    def _focus_rewrite_prompt(self) -> None:
        """Return from any page to the current conversation's editable instruction."""
        self._navigate_to_page("capture")
        self.conversation_workspace.focus_prompt()

    def _capture_configuration_changed(self, *_args: object) -> None:
        """Refresh the compact idle summary after a visible settings mutation."""
        self._update_capture_status_rows()

    def _pending_target_and_insert_readiness(self) -> tuple[TextTargetSnapshot | None, bool]:
        """Return the frozen target and whether one reviewed insertion route is ready."""
        pending_target = getattr(self, "pending_delivery_target", None) or getattr(
            self,
            "pending_command_target",
            None,
        )
        target_can_insert = pending_target is not None and (
            pending_target.editable_text is not None or keyboard_paste_available()
        )
        return pending_target, target_can_insert

    def _automatic_paste_armed(self) -> bool:
        """Require an authorized capture, a frozen target, and a usable delivery route."""
        _pending_target, target_can_insert = MluvaApplication._pending_target_and_insert_readiness(self)
        return (
            self.config.auto_paste
            and self.focus_tracker is not None
            and self.capture_allows_auto_paste
            and target_can_insert
        )

    def _update_capture_status_rows(self) -> None:
        """Project mode, delivery consequence, privacy, and shortcut state near the primary action."""
        recording = self.recorder is not None and self.recorder.process is not None
        capture_active = self.capture_preparing or self.capture_processing or recording
        review_active = self.pending_command_result is not None or self.scratchpad_store.draft is not None
        if self.capture_status_title is not None:
            if self.capture_initialization_failed:
                title = "Capture unavailable"
            elif self.capture_processing:
                title = "Finishing capture"
            elif self.capture_preparing:
                title = "Preparing capture"
            elif recording:
                title = "Recording"
            elif review_active:
                title = "Review required"
            else:
                title = "Ready to dictate"
            self.capture_status_title.set_label(title)

        if self.capture_summary_box is not None:
            self.capture_summary_box.set_visible(not capture_active and not review_active)

        if self.capture_mode_status_row is not None:
            language_name = dict(TRANSCRIPTION_LANGUAGE_OPTIONS).get(
                self.config.language_code,
                self.config.language_code,
            )
            style_name = "Faithful"
            if self.output_style is not None:
                style_index = self.output_style.get_selected()
                identifier = self.style_identifiers[style_index] if style_index < len(self.style_identifiers) else None
                style = self.personalization_store.style(identifier)
                if style is not None:
                    style_name = style.name
            self.capture_mode_status_row.set_title(f"{language_name} · {style_name}")
            self.capture_mode_status_row.set_subtitle("Language · Output style")

        approved_recording_trigger = self.approved_recording_trigger
        automatic_paste_available = self.config.auto_paste and self.focus_tracker is not None
        automatic_paste_ready = automatic_paste_available and approved_recording_trigger is not None
        pending_target, target_can_insert = MluvaApplication._pending_target_and_insert_readiness(self)
        automatic_paste_armed = capture_active and MluvaApplication._automatic_paste_armed(self)
        if self.capture_delivery_status_row is not None:
            if automatic_paste_armed:
                self.capture_delivery_status_row.set_title("Automatic paste armed")
                self.capture_delivery_status_row.set_subtitle(
                    "This global-key capture can insert into the accessible focused text field"
                )
            elif capture_active:
                self.capture_delivery_status_row.set_title("Copy-only capture")
                if self.capture_allows_auto_paste and pending_target is None:
                    delivery_explanation = "No accessible focused text field was captured"
                elif self.capture_allows_auto_paste and not target_can_insert:
                    delivery_explanation = "The target needs a keyboard paste helper, but none is ready"
                else:
                    delivery_explanation = "This result will not type into another application"
                self.capture_delivery_status_row.set_subtitle(delivery_explanation)
            elif automatic_paste_ready:
                self.capture_delivery_status_row.set_title("Global paste · button copy")
                self.capture_delivery_status_row.set_subtitle(
                    f"{approved_recording_trigger} inserts when an accessible text target is captured; "
                    "the on-screen button always copies"
                )
            else:
                self.capture_delivery_status_row.set_title("Copy-only delivery")
                if not self.config.auto_paste:
                    delivery_explanation = "Automatic paste is turned off"
                elif self.focus_tracker is None:
                    delivery_explanation = "GNOME accessibility is disabled or target tracking is unavailable"
                elif self.shortcut_service is None:
                    delivery_explanation = "The global shortcut service is disabled or unavailable"
                else:
                    delivery_explanation = (
                        f"{self.config.global_recording_key} is waiting for desktop shortcut approval"
                    )
                self.capture_delivery_status_row.set_subtitle(delivery_explanation)

        if self.capture_privacy_status_row is not None:
            if self.config.incognito_mode:
                self.capture_privacy_status_row.set_title("Incognito")
                self.capture_privacy_status_row.set_subtitle(
                    "No local history or recovery audio; recognition uses your selected speech provider"
                )
            else:
                audio_policy = self.config.audio_retention_policy.value.title()
                history_policy = (
                    "history kept"
                    if self.config.history_retention_days == 0
                    else f"history kept {self.config.history_retention_days} days"
                )
                self.capture_privacy_status_row.set_title("Private local recovery")
                self.capture_privacy_status_row.set_subtitle(f"Audio: {audio_policy.lower()} · {history_policy}")

        if self.capture_action_hint is not None:
            if approved_recording_trigger is None and self.shortcut_service is not None:
                action_hint = f"{self.config.global_recording_key} awaits desktop approval · this button always copies"
            elif approved_recording_trigger is None:
                action_hint = "Use the on-screen button for copy-only capture"
            elif automatic_paste_available:
                action_hint = (
                    f"{approved_recording_trigger} toggles global capture and can insert into a captured text field · "
                    "this button always copies"
                )
            elif self.config.auto_paste:
                action_hint = f"{approved_recording_trigger} toggles global capture · automatic paste is unavailable"
            else:
                action_hint = f"{approved_recording_trigger} toggles global capture · automatic paste is off"
            rewrite_trigger = getattr(self, "approved_rewrite_trigger", None)
            if rewrite_trigger:
                action_hint += f" · {rewrite_trigger} opens rewriting"
            self.capture_action_hint.set_label(action_hint)

    def _output_visibility_changed(self, buffer: Gtk.TextBuffer) -> None:
        """Reveal the editor only when it contains or governs unresolved output."""
        if self.output_section is None:
            return
        start, end = buffer.get_bounds()
        has_text = bool(buffer.get_text(start, end, include_hidden_chars=True).strip())
        has_command = self.command_actions is not None and self.command_actions.get_visible()
        has_scratchpad = self.scratchpad_actions is not None and self.scratchpad_actions.get_visible()
        self.output_section.set_visible(
            (has_text and self.pending_mode != "dictation") or has_command or has_scratchpad
        )

    def _show_toast(self, message: str) -> None:
        """Show transient feedback on the page where the user performed the action."""
        if self.toast_overlay is not None:
            failure_markers = ("failed", "could not", "unavailable", "needs repair", "malformed")
            is_failure = any(marker in message.casefold() for marker in failure_markers)
            toast = Adw.Toast(title=message, timeout=8 if is_failure else 5)
            if is_failure:
                toast.set_priority(Adw.ToastPriority.HIGH)
            self.toast_overlay.add_toast(toast)

    def _set_meeting_status(self, message: str) -> None:
        """Update only the explicit Meeting workflow state."""
        if self.meeting_page is not None:
            self.meeting_page.set_status(message)

    def _initialize_local_services(self) -> None:
        """Open XDG recovery stores before network credentials are required."""
        try:
            self.focus_tracker = FocusedTextTargetTracker()
        except Exception:
            self.focus_tracker = None
        self.config_path = default_config_dir(os.environ) / "config.json"
        self.config_load_error = ""
        try:
            self.config = load_config(self.config_path)
        except (OSError, ValueError, TypeError):
            self.config = AppConfig()
            self.config_load_error = (
                "config.json needs repair; using defaults. The file is preserved. Prompts remain editable."
            )
        self.personalization_store = PersonalizationStore(default_config_dir(os.environ) / "personalization.json")
        self.prompt_store = PromptStore(
            default_config_dir(os.environ) / "prompts",
            self.config.live_rewrite_custom_instructions,
            self.personalization_store.styles,
        )
        self.personalization_store.prompt_store = self.prompt_store
        self.data_directory = default_data_dir(os.environ)
        self.codex_workspace = default_runtime_dir(os.environ) / "codex-workspace"
        self.codex_workspace.mkdir(mode=0o700, parents=True, exist_ok=True)
        self.codex_workspace.chmod(0o700)
        try:
            self.pipewire_catalog = PipeWireDeviceCatalog.from_system()
            self.pipewire_catalog_error = None
        except PipeWireCatalogError as error:
            self.pipewire_catalog = PipeWireDeviceCatalog()
            self.pipewire_catalog_error = str(error)
        self.history_store = HistoryStore(self.data_directory / "history.sqlite3")
        self.history_store.initialize()
        self.conversation_store = ConversationStore(self.history_store)
        self.conversation_store.initialize()
        self.meeting_store = MeetingStore(self.data_directory / "meetings" / "meetings.json")
        self.scratchpad_store = ScratchpadDraftStore(self.data_directory / "scratchpad-draft.json")
        self.diagnostics_store = DiagnosticsStore(self.data_directory / "diagnostics.sqlite3")
        self.diagnostics_store.initialize()
        self._prune_history()

    def _initialize_capture_services(self) -> None:
        """Resolve microphone capture, credentials, and both processing clients."""
        self.approved_recording_trigger = None
        self.recorder = PipeWireRecorder.from_system(target=self.config.microphone_target)
        self.meeting_recorder = PipeWireMeetingRecorder.from_system(
            microphone_target=self.config.microphone_target,
            system_target=self.config.system_audio_target,
        )
        elevenlabs = transcription_client(self.config)
        self._configure_realtime_provider()
        self.workflow = DictationWorkflow(
            config=self.config,
            elevenlabs=elevenlabs,
            codex=self._new_rewrite_client(),
            history=self.history_store,
            cwd=self.codex_workspace,
            personalization=self.personalization_store,
            diagnostics=self.diagnostics_store,
        )
        try:
            meeting_client = ElevenLabsClient(elevenlabs_api_key())
        except RuntimeError:
            self.meeting_workflow = None
        else:
            self.meeting_workflow = MeetingWorkflow(self.config, meeting_client, self.meeting_store)
        if "MLUVA_DISABLE_GLOBAL_SHORTCUT" not in os.environ:
            self.shortcut_service = GlobalShortcutService(
                on_toggle_recording=lambda: GLib.idle_add(self._shortcut_toggled),
                on_cancel=lambda: GLib.idle_add(self._shortcut_cancelled),
                on_binding_changed=lambda function_key, trigger: GLib.idle_add(
                    self._global_shortcut_binding_changed,
                    function_key,
                    trigger,
                ),
                on_error=lambda message: GLib.idle_add(self._set_status, f"Global shortcut unavailable: {message}"),
                preferred_recording_trigger=self.config.global_recording_key,
                on_open_rewrite=lambda: GLib.idle_add(self._open_latest_conversation),
                on_rewrite_binding_changed=lambda trigger: GLib.idle_add(self._rewrite_binding_changed, trigger),
            )
            self.shortcut_service.start()
        elif self.global_shortcut_status_row is not None:
            self.global_shortcut_status_row.set_subtitle("Disabled for this Mluva process")

    def _toggle_meeting_capture(self) -> None:
        """Start or stop system-audio capture only from the explicit Meeting control."""
        if self.meeting_recorder is None or self.meeting_workflow is None or self.meeting_page is None:
            self._set_meeting_status("Meeting capture is unavailable until PipeWire and ElevenLabs are configured.")
            return
        if self.meeting_processing:
            self._set_meeting_status("The stopped Meeting is still being transcribed.")
            return
        if self.meeting_retry_in_progress:
            self._set_meeting_status("Finish the retained Meeting retry before starting another capture.")
            return
        if self._meeting_capture_active():
            self._stop_meeting_capture()
        else:
            self._start_meeting_capture()

    def _start_meeting_capture(self) -> None:
        """Freeze privacy and start both Meeting sources after explicit button activation."""
        if self.meeting_recorder is None or self.meeting_page is None:
            return
        if (
            self.capture_preparing
            or self.capture_processing
            or (self.recorder is not None and self.recorder.process is not None)
        ):
            self._set_meeting_status("Finish the active dictation capture before starting Meeting.")
            return
        if self.retry_in_progress:
            self._set_meeting_status("Finish the retained dictation retry before starting Meeting.")
            return
        if self.pending_command_result is not None or self.scratchpad_store.draft is not None:
            self._set_meeting_status("Resolve the active Command or Scratchpad review before starting Meeting.")
            return
        if not self.config.incognito_mode and self.meeting_store.persistence_error is not None:
            self._set_meeting_status(
                "The private Meeting archive is malformed. Repair it before a retained Meeting, or enable "
                "Incognito for a non-persistent session."
            )
            return
        identifier = str(uuid.uuid4())
        audio_path = self.data_directory / "meetings" / "recordings" / f"{identifier}.wav"
        self.meeting_identifier = identifier
        self.meeting_audio_path = audio_path
        self.meeting_started_at = datetime.now(UTC)
        self.meeting_pending_incognito = self.config.incognito_mode
        capture_ready_started_at = time.monotonic()
        try:
            self.meeting_recorder.start(audio_path)
        except Exception as error:
            self._record_meeting_diagnostic(
                DiagnosticStage.CAPTURE_READY,
                DiagnosticProvider.PIPEWIRE,
                DiagnosticOutcome.FAILED,
                time.monotonic() - capture_ready_started_at,
            )
            self._clear_meeting_state()
            self._set_meeting_status(f"Meeting capture could not start: {error}")
            return
        self._record_meeting_diagnostic(
            DiagnosticStage.CAPTURE_READY,
            DiagnosticProvider.PIPEWIRE,
            DiagnosticOutcome.COMPLETED,
            time.monotonic() - capture_ready_started_at,
        )
        self.meeting_capture_started_at = time.monotonic()
        self.meeting_page.set_capture_state(recording=True)
        if self.record_button is not None:
            self.record_button.set_sensitive(False)
        if self.global_recording_key is not None:
            self.global_recording_key.set_sensitive(False)
        if self.incognito_switch is not None:
            self.incognito_switch.set_sensitive(False)
        if self.language is not None:
            self.language.set_sensitive(False)
        if self.microphone_device is not None:
            self.microphone_device.set_sensitive(False)
        if self.system_audio_device is not None:
            self.system_audio_device.set_sensitive(False)
        if self.refresh_audio_button is not None:
            self.refresh_audio_button.set_sensitive(False)
        self._update_meeting_capture_status()
        self.meeting_capture_status_timeout_id = GLib.timeout_add(250, self._update_meeting_capture_status)

    def _stop_meeting_capture(self) -> None:
        """Finalize Meeting audio and run batch diarization away from GTK's thread."""
        if self.meeting_page is None or not self._meeting_capture_active():
            return
        self._clear_meeting_capture_status_timeout()
        self.meeting_processing = True
        self.meeting_page.set_capture_state(recording=False, processing=True)
        self._set_meeting_status("Finalizing microphone and system audio, then transcribing with Scribe v2…")
        threading.Thread(target=self._finish_meeting_recording, name="meeting-workflow", daemon=True).start()

    def _finish_meeting_recording(self) -> None:
        """Finalize, diarize, and archive one Meeting without any clipboard or paste operation."""
        if self.meeting_recorder is None or self.meeting_workflow is None:
            return
        identifier = self.meeting_identifier
        started_at = self.meeting_started_at
        capture_path = self.meeting_audio_path
        capture_started_at = self.meeting_capture_started_at
        if identifier is None or started_at is None:
            GLib.idle_add(self._meeting_failed, "Meeting session state was incomplete.", None, None)
            return
        capture = None
        try:
            capture = self.meeting_recorder.stop()
        except Exception as error:
            capture_seconds = 0.0 if capture_started_at is None else time.monotonic() - capture_started_at
            self._record_meeting_diagnostic(
                DiagnosticStage.CAPTURE,
                DiagnosticProvider.PIPEWIRE,
                DiagnosticOutcome.FAILED,
                capture_seconds,
            )
            if self.meeting_pending_incognito and capture_path is not None:
                capture_path.unlink(missing_ok=True)
            retained_path = capture_path if capture_path is not None and capture_path.exists() else None
            GLib.idle_add(self._meeting_failed, str(error), None, retained_path)
            return
        capture_seconds = 0.0 if capture_started_at is None else time.monotonic() - capture_started_at
        self._record_meeting_diagnostic(
            DiagnosticStage.CAPTURE,
            DiagnosticProvider.PIPEWIRE,
            DiagnosticOutcome.COMPLETED,
            capture_seconds,
        )
        recognition_started_at = time.monotonic()
        try:
            result = self.meeting_workflow.complete(
                capture,
                incognito=self.meeting_pending_incognito,
                started_at=started_at,
                identifier=identifier,
            )
        except MeetingFailure as error:
            self._record_meeting_diagnostic(
                DiagnosticStage.RECOGNITION,
                DiagnosticProvider.ELEVENLABS_SCRIBE_V2,
                DiagnosticOutcome.FAILED,
                time.monotonic() - recognition_started_at,
            )
            GLib.idle_add(
                self._meeting_failed,
                str(error),
                error.meeting,
                error.retained_audio_path,
            )
            return
        except Exception as error:
            self._record_meeting_diagnostic(
                DiagnosticStage.RECOGNITION,
                DiagnosticProvider.ELEVENLABS_SCRIBE_V2,
                DiagnosticOutcome.FAILED,
                time.monotonic() - recognition_started_at,
            )
            if self.meeting_pending_incognito:
                capture.path.unlink(missing_ok=True)
            retained_path = capture.path if capture.path.exists() else None
            GLib.idle_add(self._meeting_failed, str(error), None, retained_path)
            return
        self._record_meeting_diagnostic(
            DiagnosticStage.RECOGNITION,
            DiagnosticProvider.ELEVENLABS_SCRIBE_V2,
            DiagnosticOutcome.COMPLETED,
            time.monotonic() - recognition_started_at,
        )
        GLib.idle_add(self._meeting_finished, result)

    def _meeting_finished(self, result: MeetingWorkflowResult) -> bool:
        """Refresh the separate archive and return both capture surfaces to idle."""
        warnings = " ".join(result.meeting.warnings)
        if result.incognito:
            status = "Meeting transcribed in memory. Incognito audio and metadata were erased."
        else:
            status = "Meeting transcribed and saved to the private Meeting archive."
        if warnings:
            status = f"{status} {warnings}"
        self._clear_meeting_state()
        if self.meeting_page is not None:
            self.meeting_page.refresh()
        self._set_meeting_status(status)
        return GLib.SOURCE_REMOVE

    def _meeting_failed(
        self,
        message: str,
        meeting: MeetingRecord | None,
        retained_audio_path: Path | None,
    ) -> bool:
        """Expose controlled Meeting recovery state without disabling dictation permanently."""
        recovery = (
            f"The private recording remains available at {retained_audio_path}."
            if retained_audio_path is not None
            else "No Meeting audio was retained."
        )
        self._clear_meeting_state()
        if self.meeting_page is not None:
            self.meeting_page.refresh()
        if meeting is not None and meeting.transcript:
            if self.output_buffer is not None:
                self.output_buffer.set_text(meeting.transcript)
            recovery = f"The completed transcript is available in the Capture editor for manual review. {recovery}"
        self._set_meeting_status(f"Meeting could not complete: {message} {recovery}")
        return GLib.SOURCE_REMOVE

    def _retry_meeting_recognition(self, meeting: MeetingRecord) -> None:
        """Start an explicit retained-audio retry without copying or delivering text."""
        if self.meeting_workflow is None or self.meeting_page is None:
            self._set_meeting_status("Meeting retry is unavailable until capture services are configured.")
            return
        if self.meeting_processing or self._meeting_capture_active():
            self._set_meeting_status("Finish the active Meeting capture before retrying retained audio.")
            return
        if (
            self.capture_preparing
            or self.capture_processing
            or (self.recorder is not None and self.recorder.process is not None)
        ):
            self._set_meeting_status("Finish the active dictation capture before retrying this Meeting.")
            return
        if self.pending_command_result is not None or self.scratchpad_store.draft is not None:
            self._set_meeting_status("Resolve the active Command or Scratchpad review before retrying this Meeting.")
            return
        if self.retry_in_progress or self.meeting_retry_in_progress:
            self._set_meeting_status("A transcription retry is already in progress.")
            return
        self.meeting_retry_in_progress = True
        self.meeting_retry_identifier = meeting.identifier
        self.meeting_page.record_button.set_sensitive(False)
        if self.record_button is not None:
            self.record_button.set_sensitive(False)
        if self.global_recording_key is not None:
            self.global_recording_key.set_sensitive(False)
        if self.language is not None:
            self.language.set_sensitive(False)
        if self.microphone_device is not None:
            self.microphone_device.set_sensitive(False)
        if self.system_audio_device is not None:
            self.system_audio_device.set_sensitive(False)
        if self.refresh_audio_button is not None:
            self.refresh_audio_button.set_sensitive(False)
        self._set_meeting_status("Retrying retained Meeting audio with ElevenLabs Scribe v2…")
        threading.Thread(
            target=self._retry_meeting_recognition_worker,
            args=(meeting.identifier,),
            name="meeting-recognition-retry",
            daemon=True,
        ).start()

    def _retry_meeting_recognition_worker(self, identifier: str) -> None:
        """Run Meeting retry network traffic away from GTK's thread."""
        if self.meeting_workflow is None:
            return
        recognition_started_at = time.monotonic()
        try:
            result = self.meeting_workflow.retry(identifier)
        except MeetingFailure as error:
            self._record_session_diagnostic(
                identifier,
                "meeting",
                DiagnosticStage.RECOGNITION,
                DiagnosticProvider.ELEVENLABS_SCRIBE_V2,
                DiagnosticOutcome.FAILED,
                time.monotonic() - recognition_started_at,
            )
            GLib.idle_add(self._meeting_retry_failed, str(error), error.meeting)
            return
        except Exception as error:
            self._record_session_diagnostic(
                identifier,
                "meeting",
                DiagnosticStage.RECOGNITION,
                DiagnosticProvider.ELEVENLABS_SCRIBE_V2,
                DiagnosticOutcome.FAILED,
                time.monotonic() - recognition_started_at,
            )
            GLib.idle_add(self._meeting_retry_failed, str(error), None)
            return
        self._record_session_diagnostic(
            identifier,
            "meeting",
            DiagnosticStage.RECOGNITION,
            DiagnosticProvider.ELEVENLABS_SCRIBE_V2,
            DiagnosticOutcome.COMPLETED,
            time.monotonic() - recognition_started_at,
        )
        GLib.idle_add(self._meeting_retry_finished, result)

    def _meeting_retry_finished(self, _result: MeetingWorkflowResult) -> bool:
        """Refresh the recovered Meeting while leaving copy as a separate user action."""
        self.meeting_retry_in_progress = False
        self.meeting_retry_identifier = None
        if self.meeting_page is not None:
            self.meeting_page.set_capture_state(recording=False)
            self.meeting_page.refresh()
        self._reset_record_button()
        self._set_meeting_status("Meeting transcription recovered. Review or copy it explicitly from Meeting.")
        return GLib.SOURCE_REMOVE

    def _meeting_retry_failed(self, message: str, meeting: MeetingRecord | None) -> bool:
        """Leave failed Meeting audio retryable after another controlled provider failure."""
        self.meeting_retry_in_progress = False
        self.meeting_retry_identifier = None
        if self.meeting_page is not None:
            self.meeting_page.set_capture_state(recording=False)
            self.meeting_page.refresh()
        recovery = ""
        if meeting is not None and meeting.transcript and self.output_buffer is not None:
            self.output_buffer.set_text(meeting.transcript)
            recovery = " The completed transcript is available in the Capture editor for manual review."
        self._reset_record_button()
        self._set_meeting_status(f"Meeting retry failed: {message} The private recording remains available.{recovery}")
        return GLib.SOURCE_REMOVE

    def _delete_meeting(self, meeting: MeetingRecord) -> bool:
        """Delete one reviewed Meeting unless its retained audio is being retried."""
        if meeting.identifier == self.meeting_retry_identifier:
            self._set_meeting_status("Wait for the active Meeting retry before deleting this record.")
            return False
        try:
            self.meeting_store.delete(meeting.identifier)
        except Exception as error:
            self._set_meeting_status(f"Meeting deletion failed: {error}")
            return False
        return True

    def _meeting_capture_active(self) -> bool:
        """Return whether either explicit Meeting source process is still owned by the app."""
        return self.meeting_recorder is not None and (
            self.meeting_recorder.microphone_process is not None or self.meeting_recorder.system_process is not None
        )

    def _update_meeting_capture_status(self) -> bool:
        """Expose elapsed time and the exact desktop-wide source boundary during Meeting."""
        if self.meeting_capture_started_at is None or not self._meeting_capture_active():
            self.meeting_capture_status_timeout_id = None
            return GLib.SOURCE_REMOVE
        elapsed_seconds = int(time.monotonic() - self.meeting_capture_started_at)
        minutes, seconds = divmod(elapsed_seconds, 60)
        retention = "Incognito · erase after transcription" if self.meeting_pending_incognito else "private archive"
        microphone_name = self.pipewire_catalog.display_name(
            PipeWireDeviceKind.MICROPHONE,
            self.config.microphone_target,
        )
        system_output_name = self.pipewire_catalog.display_name(
            PipeWireDeviceKind.SYSTEM_OUTPUT,
            self.config.system_audio_target,
        )
        self._set_meeting_status(
            f"Meeting {minutes:02d}:{seconds:02d} · {microphone_name} + {system_output_name} · {retention}"
        )
        return GLib.SOURCE_CONTINUE

    def _clear_meeting_capture_status_timeout(self) -> None:
        """Stop Meeting elapsed-time updates without losing the frozen wall timestamp."""
        if self.meeting_capture_status_timeout_id is not None:
            GLib.source_remove(self.meeting_capture_status_timeout_id)
            self.meeting_capture_status_timeout_id = None

    def _clear_meeting_state(self) -> None:
        """Release frozen Meeting state and restore the two independent capture surfaces."""
        self._clear_meeting_capture_status_timeout()
        self.meeting_processing = False
        self.meeting_identifier = None
        self.meeting_audio_path = None
        self.meeting_started_at = None
        self.meeting_capture_started_at = None
        self.meeting_pending_incognito = False
        if self.meeting_page is not None:
            self.meeting_page.set_capture_state(recording=False)
            self.meeting_page.set_privacy(self.config.incognito_mode)
        self._reset_record_button()

    def _record_meeting_diagnostic(
        self,
        stage: DiagnosticStage,
        provider: DiagnosticProvider,
        outcome: DiagnosticOutcome,
        duration_seconds: float,
    ) -> None:
        """Record content-free Meeting timing unless its frozen session is Incognito."""
        if self.meeting_identifier is None or self.meeting_pending_incognito:
            return
        self._record_session_diagnostic(
            self.meeting_identifier,
            "meeting",
            stage,
            provider,
            outcome,
            duration_seconds,
        )

    def _toggle_recording(self, _button: Gtk.Button) -> None:
        """Start or finalize capture from the primary control."""
        if self.record_button is None or self.recorder is None:
            return
        if self.live_final_entry is not None:
            self._set_status("Finishing the live draft. Cancel it before starting another recording.")
            return
        if self.capture_processing:
            self._set_status("The stopped recording is still being processed.")
            return
        if self.capture_preparing:
            self._cancel_capture()
            return
        if self.recorder.process is None:
            self.capture_allows_auto_paste = False
            self._start_capture()
            return
        self._stop_capture()

    def _start_capture(self) -> None:
        """Start PipeWire capture after the caller establishes delivery safety."""
        if self.record_button is None or self.recorder is None or self.workflow is None:
            return
        if self.capture_preparing or self.capture_processing:
            self._set_status("Finish the active dictation preparation or processing first.")
            return
        if self.meeting_processing or self.meeting_retry_in_progress or self._meeting_capture_active():
            self._set_status("Finish the explicit Meeting capture or retry before starting dictation.")
            return
        if self.retry_in_progress:
            self._set_status("Finish the retained-audio retry before starting another recording.")
            return
        if self.pending_command_result is not None:
            self._set_status("Apply or discard the current Command preview before recording again.")
            return
        if self.scratchpad_store.draft is not None:
            self._set_status("Resolve or delete the current Scratchpad draft before recording again.")
            return
        if self.mode is not None and self.cleanup_switch is not None:
            focus_tracker = self.focus_tracker
            self.pending_delivery_target = (
                focus_tracker.capture_delivery_target()
                if self.capture_allows_auto_paste and focus_tracker is not None
                else None
            )
            self.pending_application_identifier = (
                self.pending_delivery_target.application_identifier
                if self.pending_delivery_target is not None
                else focus_tracker.capture_application_identifier()
                if self.capture_allows_auto_paste and focus_tracker is not None
                else None
            )
            self.profile_application_identifier = self.pending_application_identifier
            fallback_mode = (
                self.config.default_mode
                if self.config.remember_per_application and self.pending_application_identifier is not None
                else CAPTURE_MODE_IDS[self.mode.get_selected()]
            )
            self.pending_mode = self._capture_mode_for_application(fallback_mode)
            self.mode.set_selected(CAPTURE_MODE_IDS.index(self.pending_mode))
            self.pending_incognito = self.incognito_switch is not None and self.incognito_switch.get_active()
            self.pending_audio_retention = self._selected_audio_retention()
            if self.pending_incognito and self.pending_mode == "command":
                self.pending_delivery_target = None
                self.pending_application_identifier = None
                self._set_status(
                    "Command mode is unavailable in Incognito because Codex durability cannot be guaranteed."
                )
                return
            self.pending_cleanup = self.cleanup_switch.get_active() and not self.pending_incognito
            self._freeze_output_style()
            self.pending_codex_model_identifier = None
            self.pending_transcript_preparation = None
            self.pending_command_target = None
            if self.pending_mode == "command" and self.capture_allows_auto_paste:
                self.pending_delivery_target = None
                try:
                    self.pending_command_target = (
                        focus_tracker.capture_text_target() if focus_tracker is not None else None
                    )
                except TextSelectionTooLargeError as error:
                    self.pending_delivery_target = None
                    self.pending_application_identifier = None
                    self.pending_style_identifier = None
                    self.pending_style = None
                    self.pending_use_saved_style = False
                    self._set_status(str(error))
                    return
                if self.pending_command_target is not None:
                    self.pending_application_identifier = self.pending_command_target.application_identifier
            elif self.pending_mode != "dictation":
                self.pending_delivery_target = None
            self.workflow.cleanup_instructions = self.prompt_store.read("cleanup").text
            self.pending_transcript_preparation = self.workflow.freeze_transcript_preparation(
                self.pending_mode,
                self.pending_application_identifier,
            )
            self.mode.set_sensitive(False)
            self.cleanup_switch.set_sensitive(False)
            if self.spoken_commands_switch is not None:
                self.spoken_commands_switch.set_sensitive(False)
            if self.auto_paste_switch is not None:
                self.auto_paste_switch.set_sensitive(False)
            if self.global_recording_key is not None:
                self.global_recording_key.set_sensitive(False)
            if self.incognito_switch is not None:
                self.incognito_switch.set_sensitive(False)
            if self.audio_retention is not None:
                self.audio_retention.set_sensitive(False)
            if self.history_retention is not None:
                self.history_retention.set_sensitive(False)
            if self.output_style is not None:
                self.output_style.set_sensitive(False)
            if self.remember_application_switch is not None:
                self.remember_application_switch.set_sensitive(False)
            if self.language is not None:
                self.language.set_sensitive(False)
            if self.microphone_device is not None:
                self.microphone_device.set_sensitive(False)
            if self.system_audio_device is not None:
                self.system_audio_device.set_sensitive(False)
            if self.refresh_audio_button is not None:
                self.refresh_audio_button.set_sensitive(False)
        data_dir = self.data_directory / "recordings"
        stamp = datetime.now(UTC).strftime("%Y%m%dT%H%M%S%f")
        self.audio_path = data_dir / f"{stamp}.wav"
        self.pending_session_identifier = str(uuid.uuid4())
        self._start_live_rewrite()
        self.capture_preparing = True
        self.capture_stop_requested = False
        self.pending_realtime_fallback_reason = None
        self.realtime_session = None
        if self.segment_cleanup_session is not None:
            self.segment_cleanup_session.cancel()
        self.segment_cleanup_session = None
        set_button_content(self.record_button, "process-stop-symbolic", "Cancel preparation")
        self.record_button.remove_css_class("suggested-action")
        self.record_button.add_css_class("destructive-action")
        self._present_preparing_bar()
        self._update_capture_status_rows()
        codex_required = self.pending_mode == "command" or self.pending_cleanup or self.pending_use_saved_style
        self._set_status(
            "Preparing recognition and resolving the Codex model before microphone capture…"
            if codex_required
            else "Preparing recognition before microphone capture…"
        )
        threading.Thread(
            target=self._prepare_capture,
            args=(
                self.pending_session_identifier,
                self.audio_path,
                self.config.language_code,
                codex_required,
                self.config.codex_model,
                self.pending_mode,
                self.pending_cleanup,
                self.pending_transcript_preparation,
            ),
            name="dictation-readiness",
            daemon=True,
        ).start()

    def _prepare_capture(
        self,
        session_identifier: str,
        audio_path: Path,
        language_code: str,
        codex_required: bool,
        configured_codex_model: str | None,
        mode: str,
        use_codex_cleanup: bool,
        transcript_preparation: TranscriptPreparationSnapshot,
    ) -> None:
        """Open the realtime recognition route away from GTK before allowing speech capture."""
        codex_model_identifier = None
        codex_resolution_seconds = None
        if codex_required:
            codex_started_at = time.monotonic()
            try:
                if self.workflow is None:
                    raise RuntimeError("Codex processing is unavailable.")
                codex_model_identifier = self.workflow.codex.resolve_model(configured_codex_model)
            except Exception as error:
                GLib.idle_add(
                    self._capture_preparation_failed,
                    session_identifier,
                    audio_path,
                    str(error),
                    time.monotonic() - codex_started_at,
                )
                return
            codex_resolution_seconds = time.monotonic() - codex_started_at
        if self.shutting_down or self.pending_session_identifier != session_identifier:
            return
        segment_cleanup_session = None
        if (
            use_codex_cleanup
            and mode != "command"
            and self.config.rewrite_provider == "codex"
            and self.config.transcription_provider == "elevenlabs"
        ):
            if self.workflow is None or codex_model_identifier is None:
                GLib.idle_add(
                    self._capture_preparation_failed,
                    session_identifier,
                    audio_path,
                    "Codex segment cleanup could not be prepared.",
                    codex_resolution_seconds or 0,
                )
                return
            parent_client = self.workflow.codex
            if not isinstance(parent_client, CodexAppServerClient):
                GLib.idle_add(
                    self._capture_preparation_failed,
                    session_identifier,
                    audio_path,
                    "Codex segment cleanup requires an isolated app-server client.",
                    codex_resolution_seconds or 0,
                )
                return
            codex_workspace = self.codex_workspace
            model_identifier = codex_model_identifier
            cleanup_instructions = self.workflow.cleanup_instructions
            segment_cleanup_session = SegmentCleanupSession(
                session_identifier=session_identifier,
                provider_identifier=ENHANCEMENT_PROVIDER_CODEX_APP_SERVER,
                model_identifier=model_identifier,
                prepare_text=transcript_preparation.process,
                protected_vocabulary=transcript_preparation.protected_vocabulary,
                attempt_factory=lambda: CodexSegmentCleanupAttempt(
                    client=parent_client.spawn(),
                    cwd=codex_workspace,
                    model_identifier=model_identifier,
                    instructions=cleanup_instructions,
                ),
            )
        recognition_started_at = time.monotonic()
        realtime_session = None
        fallback_reason = None
        if self.realtime_client is None:
            fallback_reason = RECOGNITION_FALLBACK_UNAVAILABLE
        else:
            try:
                realtime_session = self.realtime_client.start(
                    language_code,
                    on_preview=self._live_preview_callback(session_identifier) if mode == "dictation" else None,
                    on_committed_segment=(
                        None
                        if segment_cleanup_session is None
                        else lambda segment: segment_cleanup_session.accept_stable_segment(
                            segment.identifier,
                            segment.text,
                        )
                    ),
                )
                if isinstance(realtime_session, BatchPreviewSession):
                    realtime_session.set_preview_enabled(
                        mode == "dictation" and self.live_session_identifier == session_identifier
                    )
            except Exception:
                fallback_reason = RECOGNITION_FALLBACK_STARTUP_FAILED
        if self.shutting_down or self.pending_session_identifier != session_identifier:
            if realtime_session is not None:
                realtime_session.cancel()
            if segment_cleanup_session is not None:
                segment_cleanup_session.cancel()
            return
        GLib.idle_add(
            self._capture_prepared,
            session_identifier,
            audio_path,
            realtime_session,
            segment_cleanup_session,
            codex_model_identifier,
            codex_resolution_seconds,
            fallback_reason,
            time.monotonic() - recognition_started_at,
        )

    def _capture_preparation_failed(
        self,
        session_identifier: str,
        audio_path: Path,
        message: str,
        codex_resolution_seconds: float,
    ) -> bool:
        """Return to idle without opening a microphone when required Codex preparation fails."""
        if self.pending_session_identifier != session_identifier or self.audio_path != audio_path:
            return GLib.SOURCE_REMOVE
        self._record_app_diagnostic(
            DiagnosticStage.CAPTURE_READY,
            DiagnosticOutcome.FAILED,
            codex_resolution_seconds,
            provider=DiagnosticProvider.CODEX_APP_SERVER,
        )
        self.capture_preparing = False
        self.capture_stop_requested = False
        self.audio_path = None
        self.pending_command_target = None
        self.pending_delivery_target = None
        self.pending_session_identifier = None
        self.pending_application_identifier = None
        self.pending_style_identifier = None
        self.pending_style = None
        self.pending_use_saved_style = False
        self.pending_codex_model_identifier = None
        self.pending_transcript_preparation = None
        self.pending_realtime_fallback_reason = None
        self._clear_live_capture()
        self._reset_record_button()
        self._set_status(f"Codex preparation failed before microphone capture: {message}")
        MluvaApplication._publish_completion_status(self, "error", "Could not prepare dictation. Open Mluva.")
        return GLib.SOURCE_REMOVE

    def _capture_prepared(
        self,
        session_identifier: str,
        audio_path: Path,
        realtime_session: RealtimeTranscriptionSession | None,
        segment_cleanup_session: SegmentCleanupSession | None,
        codex_model_identifier: str | None,
        codex_resolution_seconds: float | None,
        fallback_reason: str | None,
        recognition_ready_seconds: float,
    ) -> bool:
        """Start PipeWire only after the readiness worker reaches a terminal route."""
        if (
            self.shutting_down
            or not self.capture_preparing
            or self.pending_session_identifier != session_identifier
            or self.audio_path != audio_path
        ):
            if realtime_session is not None:
                realtime_session.cancel()
            if segment_cleanup_session is not None:
                segment_cleanup_session.cancel()
            return GLib.SOURCE_REMOVE
        self.capture_preparing = False
        self.pending_realtime_fallback_reason = fallback_reason
        self.pending_codex_model_identifier = codex_model_identifier
        self.realtime_session = realtime_session
        self.segment_cleanup_session = segment_cleanup_session
        if codex_resolution_seconds is not None:
            self._record_app_diagnostic(
                DiagnosticStage.CAPTURE_READY,
                DiagnosticOutcome.COMPLETED,
                codex_resolution_seconds,
                provider=DiagnosticProvider.CODEX_APP_SERVER,
            )
        self._record_app_diagnostic(
            DiagnosticStage.CAPTURE_READY,
            DiagnosticOutcome.SAFE_FALLBACK if fallback_reason is not None else DiagnosticOutcome.COMPLETED,
            recognition_ready_seconds,
            provider=DiagnosticProvider.ELEVENLABS_SCRIBE_V2,
        )
        if self.capture_stop_requested:
            if realtime_session is not None:
                realtime_session.cancel()
            if segment_cleanup_session is not None:
                segment_cleanup_session.cancel()
            self.realtime_session = None
            self.segment_cleanup_session = None
            self.audio_path = None
            self.pending_command_target = None
            self.pending_delivery_target = None
            self.pending_session_identifier = None
            self.pending_application_identifier = None
            self.pending_style_identifier = None
            self.pending_style = None
            self.pending_use_saved_style = False
            self.pending_codex_model_identifier = None
            self.pending_transcript_preparation = None
            self.pending_realtime_fallback_reason = None
            self.capture_stop_requested = False
            self._clear_live_capture()
            self._reset_record_button()
            self._set_status("Capture ended before recognition was ready; no audio was recorded or sent.")
            return GLib.SOURCE_REMOVE
        pipewire_ready_started_at = time.monotonic()
        try:
            self.recorder.start(
                audio_path,
                (
                    None
                    if realtime_session is None
                    else lambda frames, level: self._submit_realtime_audio(realtime_session, frames, level)
                ),
            )
        except Exception as error:
            if realtime_session is not None:
                realtime_session.cancel()
            if segment_cleanup_session is not None:
                segment_cleanup_session.cancel()
            self.realtime_session = None
            self.segment_cleanup_session = None
            self._record_app_diagnostic(
                DiagnosticStage.CAPTURE_READY,
                DiagnosticOutcome.FAILED,
                time.monotonic() - pipewire_ready_started_at,
                provider=DiagnosticProvider.PIPEWIRE,
            )
            self.audio_path = None
            self.pending_command_target = None
            self.pending_delivery_target = None
            self.pending_session_identifier = None
            self.pending_application_identifier = None
            self.pending_style_identifier = None
            self.pending_style = None
            self.pending_use_saved_style = False
            self.pending_codex_model_identifier = None
            self.pending_transcript_preparation = None
            self.pending_realtime_fallback_reason = None
            self._clear_live_capture()
            self._set_error(str(error))
            self._reset_record_button()
            MluvaApplication._publish_completion_status(self, "error", "Microphone could not start. Open Mluva.")
            return GLib.SOURCE_REMOVE
        self._record_app_diagnostic(
            DiagnosticStage.CAPTURE_READY,
            DiagnosticOutcome.COMPLETED,
            time.monotonic() - pipewire_ready_started_at,
            provider=DiagnosticProvider.PIPEWIRE,
        )
        set_button_content(self.record_button, "media-playback-stop-symbolic", "Stop")
        self.record_button.set_sensitive(True)
        self.record_button.remove_css_class("suggested-action")
        self.record_button.add_css_class("destructive-action")
        self.capture_started_at = time.monotonic()
        route = "batch fallback" if fallback_reason is not None else "realtime"
        self._show_live_capture(f"Ready · Listening with Scribe v2 {route}")
        self._set_status(f"Ready · microphone capture started with Scribe v2 {route}.")
        self._update_capture_status()
        self.capture_status_timeout_id = GLib.timeout_add(250, self._update_capture_status)
        return GLib.SOURCE_REMOVE

    @staticmethod
    def _submit_realtime_audio(
        realtime_session: RealtimeTranscriptionSession,
        frames: bytes,
        _level: float,
    ) -> None:
        """Offer one complete chunk to the nonblocking realtime queue."""
        realtime_session.submit_audio(frames)

    def _stop_capture(self) -> None:
        """Freeze UI options and finish active capture away from GTK's thread."""
        if self.record_button is None:
            return
        if self.capture_started_at is not None:
            self._record_app_diagnostic(
                DiagnosticStage.CAPTURE,
                DiagnosticOutcome.COMPLETED,
                time.monotonic() - self.capture_started_at,
            )
        self._clear_capture_status_timeout()
        self.capture_processing = True
        self.record_button.set_sensitive(False)
        MluvaApplication._publish_completion_status(self, "processing", "Finishing your dictation…")
        if self.realtime_session is not None and self.realtime_session.is_healthy:
            self._set_status("Finalizing committed ElevenLabs Scribe v2 Realtime text…")
        else:
            self._set_status("Transcribing with the selected speech provider…")
        threading.Thread(target=self._finish_recording, name="dictation-workflow", daemon=True).start()

    def _finish_recording(self) -> None:
        """Run network and Codex work away from the GTK event loop."""
        if self.recorder is None or self.workflow is None:
            return
        audio_path = self.audio_path
        realtime_session = self.realtime_session
        segment_cleanup_session = self.segment_cleanup_session
        segment_cleanup = None
        recognized_transcription = None
        recognition_duration_seconds = None
        fallback_reason = self.pending_realtime_fallback_reason
        mode = self.pending_mode
        use_codex_cleanup = self.pending_cleanup
        allow_auto_paste = self.capture_allows_auto_paste
        incognito = self.pending_incognito
        audio_retention_policy = self.pending_audio_retention
        command_target = self.pending_command_target
        session_identifier = self.pending_session_identifier
        application_identifier = self.pending_application_identifier
        style_identifier = self.pending_style_identifier
        use_saved_style = self.pending_use_saved_style
        delivery_target = self.pending_delivery_target
        codex_model_identifier = self.pending_codex_model_identifier
        transcript_preparation = self.pending_transcript_preparation
        frozen_style = self.pending_style
        try:
            audio_path = self.recorder.stop()
            if realtime_session is not None:
                try:
                    realtime_result = realtime_session.finish()
                except Exception:
                    fallback_reason = RECOGNITION_FALLBACK_STREAM_FAILED
                else:
                    recognized_transcription = realtime_result.transcription
                    recognition_duration_seconds = realtime_result.finalization_seconds
                    if segment_cleanup_session is not None:
                        candidate_cleanup = segment_cleanup_session.stop_and_drain()
                        if candidate_cleanup.raw_text == recognized_transcription.text:
                            segment_cleanup = candidate_cleanup
                        else:
                            segment_cleanup_session.cancel()
            elif fallback_reason is None:
                fallback_reason = RECOGNITION_FALLBACK_UNAVAILABLE
            if recognized_transcription is None and segment_cleanup_session is not None:
                segment_cleanup_session.cancel()
            result = self.workflow.complete(
                audio_path,
                mode=mode,
                use_codex_cleanup=use_codex_cleanup,
                allow_auto_paste=allow_auto_paste,
                incognito=incognito,
                audio_retention_policy=audio_retention_policy,
                selected_text=command_target.selected_text if command_target is not None else None,
                session_identifier=session_identifier,
                application_identifier=application_identifier,
                style_identifier=style_identifier,
                use_saved_style=use_saved_style,
                recognized_transcription=recognized_transcription,
                recognition_duration_seconds=recognition_duration_seconds,
                recognition_used_batch_fallback=fallback_reason is not None,
                recognition_fallback_reason=fallback_reason,
                delivery_target=delivery_target,
                codex_model_identifier=codex_model_identifier,
                transcript_preparation=transcript_preparation,
                segment_cleanup=segment_cleanup,
                frozen_style=frozen_style,
                style_is_frozen=True,
            )
        except WorkflowFailure as error:
            GLib.idle_add(
                self._workflow_failed,
                str(error),
                error.retained_audio_path,
                error.history_entry,
                error.output_text,
            )
            return
        except Exception as error:
            if realtime_session is not None:
                realtime_session.cancel()
            if segment_cleanup_session is not None:
                segment_cleanup_session.cancel()
            should_erase = incognito or not audio_retention_policy.should_retain(delivery_succeeded=False)
            if should_erase and audio_path is not None:
                try:
                    audio_path.unlink(missing_ok=True)
                except OSError:
                    pass
            retained_audio_path = audio_path if audio_path is not None and audio_path.exists() else None
            GLib.idle_add(self._workflow_failed, str(error), retained_audio_path, None, "")
            return
        GLib.idle_add(self._workflow_finished, result)

    def _workflow_finished(self, result: WorkflowResult) -> bool:
        """Render the final output and restore the ready state on GTK's thread."""
        workspace = self.conversation_workspace
        viewing_live = workspace is not None and workspace.viewing_live
        self.capture_processing = False
        self.realtime_session = None
        self.segment_cleanup_session = None
        self.audio_path = None
        self.pending_realtime_fallback_reason = None
        finishing_live = (
            self.live_schedule is not None
            and result.mode == "dictation"
            and result.history_entry is not None
            and not result.incognito
        )
        if finishing_live:
            self.live_final_entry = result.history_entry.identifier
            self.live_final_text = result.transcription.text
        else:
            self._clear_live_capture()
        status = result.delivery.guidance
        delivery_target = self.pending_delivery_target
        if result.history_entry is not None and delivery_target is not None:
            self._remember_history_delivery_target(result.history_entry.identifier, delivery_target)
        self.pending_session_identifier = None
        self.pending_application_identifier = None
        self.pending_delivery_target = None
        self.pending_style_identifier = None
        self.pending_style = None
        self.pending_use_saved_style = False
        self.pending_codex_model_identifier = None
        self.pending_transcript_preparation = None
        self.editing_scratchpad = result.mode == "scratchpad" and result.requires_acceptance
        if result.mode == "command" and result.requires_acceptance:
            self.pending_command_result = result
            target = self.pending_command_target
            if self.command_source_label is not None:
                if target is None:
                    source_description = (
                        "No supported external text target was captured. Acceptance will copy the result."
                    )
                elif target.selected_text is None:
                    source_description = "No text was selected. Acceptance will insert at the captured caret."
                else:
                    source_description = f"Selected text:\n{target.selected_text}"
                self.command_source_label.set_label(source_description)
                self.command_source_label.set_visible(True)
            if self.accept_command_button is not None:
                if target is None or not self.config.auto_paste:
                    action_label = "Copy result"
                elif target.has_selection:
                    action_label = "Replace selection"
                else:
                    action_label = "Insert at captured caret"
                self.accept_command_button.set_label(action_label)
            if self.command_actions is not None:
                self.command_actions.set_visible(True)
            if self.output_view is not None:
                self.output_view.set_editable(False)
        elif self.editing_scratchpad:
            try:
                self.scratchpad_store.save(
                    ScratchpadDraft(
                        identifier=str(uuid.uuid4()),
                        history_identifier=(
                            result.history_entry.identifier if result.history_entry is not None else None
                        ),
                        created_at=(
                            result.history_entry.created_at
                            if result.history_entry is not None
                            else datetime.now(UTC).isoformat()
                        ),
                        raw_text=result.transcription.text,
                        text=result.output_text,
                        audio_path=str(result.retained_audio_path) if result.retained_audio_path is not None else None,
                        incognito=result.incognito,
                        audio_retention_policy=self.pending_audio_retention.value,
                        session_identifier=result.session_identifier,
                    ),
                    persist=not result.incognito,
                )
            except Exception as error:
                self.editing_scratchpad = False
                recovery = (
                    "The text and audio remain in History."
                    if result.history_entry is not None
                    else f"The source audio remains at {result.retained_audio_path}."
                )
                status = f"Scratchpad draft persistence failed: {error}. {recovery}"
        if self.output_buffer is not None:
            self.output_buffer.set_text(result.output_text)
        if self.scratchpad_actions is not None:
            self.scratchpad_actions.set_visible(self.editing_scratchpad)
        try:
            self._prune_history()
        except Exception as error:
            status = f"{status} History retention failed: {error}"
        if self.history_page is not None:
            self.history_page.refresh()
        self._history_changed()
        self._set_status(status)
        self._reset_record_button(keep_overlay=finishing_live)
        workspace = getattr(self, "conversation_workspace", None)
        if workspace is not None and result.mode == "dictation":
            if finishing_live:
                workspace.set_live("Finishing live draft…", result.transcription.text, recording=False)
            else:
                workspace.finish_live()
            if result.history_entry is not None:
                if viewing_live and not workspace.prompt_text() and self.rewrite_client is None:
                    workspace.show_conversation(result.history_entry, [], preserve_live=finishing_live)
            else:
                workspace.show_transient(result.transcription.text, result.output_text)
        if not result.requires_acceptance:
            if result.delivery.pasted:
                message = "Inserted. Text also stays on the clipboard."
            elif result.delivery.paste_dispatched:
                message = "Paste unconfirmed. Check the target before pasting again."
            else:
                message = "Copied. Ready to paste." if result.delivery.copied else "Dictation ready."
            if result.mode == "dictation" and result.history_entry is not None and not self.config.incognito_mode:
                self._publish_review(result.history_entry.identifier)
            else:
                MluvaApplication._publish_completion_status(self, "copied", message)
        if result.mode == "dictation" and result.history_entry is not None and not result.incognito:
            self._queue_conversation_title(result.history_entry)
        if self.live_schedule is not None and result.mode == "dictation":
            if result.history_entry is not None and not result.incognito:
                if workspace is not None:
                    workspace.set_busy(True, "Finishing live draft…")
                self._maybe_live_rewrite(self.live_final_text, final=True)
            else:
                self._cancel_live_rewrite()
        return GLib.SOURCE_REMOVE

    def _workflow_failed(
        self,
        message: str,
        retained_audio_path: Path | None,
        history_entry: HistoryEntry | None,
        output_text: str,
    ) -> bool:
        """Expose whether frozen privacy policy retained failure audio for retry."""
        live_text = self.conversation_workspace.live_draft() if self.live_schedule is not None else ""
        self._cancel_live_rewrite()
        self.capture_processing = False
        if history_entry is not None and self.pending_delivery_target is not None:
            self._remember_history_delivery_target(history_entry.identifier, self.pending_delivery_target)
        self.realtime_session = None
        if self.segment_cleanup_session is not None:
            self.segment_cleanup_session.cancel()
        self.segment_cleanup_session = None
        self.audio_path = None
        self.pending_realtime_fallback_reason = None
        self._clear_live_capture()
        self.pending_command_target = None
        self.pending_delivery_target = None
        self.pending_command_result = None
        self.pending_session_identifier = None
        self.pending_application_identifier = None
        self.pending_style_identifier = None
        self.pending_style = None
        self.pending_use_saved_style = False
        self.pending_codex_model_identifier = None
        self.pending_transcript_preparation = None
        recovery = (
            f"The recording was retained at {retained_audio_path}."
            if retained_audio_path is not None
            else "The recording was erased."
        )
        recovered_text = history_entry.delivered_text if history_entry is not None else output_text
        if recovered_text and self.output_buffer is not None:
            self.output_buffer.set_text(recovered_text)
        if self.history_page is not None:
            self.history_page.refresh()
        self._set_error(f"Mluva could not complete: {message}. {recovery}")
        self._reset_record_button()
        workspace = getattr(self, "conversation_workspace", None)
        if workspace is not None:
            workspace.finish_live()
            if history_entry is not None:
                workspace.show_conversation(history_entry, [])
                workspace.refresh_history()
            elif recovered_text:
                workspace.show_transient(recovered_text, recovered_text)
        MluvaApplication._publish_completion_status(self, "error", "Dictation needs attention. Open Mluva.")
        if live_text.strip() and not self.config.incognito_mode:
            self.conversation_workspace.show_transient(live_text, live_text)
            self.conversation_workspace.notice.set_label("Capture failed. Your live draft is available to copy.")
        return GLib.SOURCE_REMOVE

    def _reset_record_button(self, *, keep_overlay: bool = False) -> None:
        """Return the primary action to the idle capture state."""
        self._clear_capture_status_timeout()
        self.capture_preparing = False
        self.capture_stop_requested = False
        self._hide_recording_bar(keep_overlay=keep_overlay)
        if self.record_button is None:
            return
        set_button_content(self.record_button, "audio-input-microphone-symbolic", "Dictate")
        meeting_busy = self.meeting_processing or self.meeting_retry_in_progress or self._meeting_capture_active()
        has_pending_review = self.pending_command_result is not None or self.scratchpad_store.draft is not None
        controls_available = not has_pending_review and not meeting_busy
        if self.capture_action_bar is not None:
            self.capture_action_bar.set_visible(not has_pending_review)
        self.record_button.set_sensitive(controls_available)
        self.record_button.remove_css_class("destructive-action")
        self.record_button.add_css_class("suggested-action")
        if self.meeting_page is not None:
            self.meeting_page.record_button.set_sensitive(controls_available)
        if self.mode is not None:
            self.mode.set_sensitive(controls_available)
        if self.cleanup_switch is not None:
            self.cleanup_switch.set_sensitive(not self.pending_incognito and controls_available)
        if self.spoken_commands_switch is not None:
            self.spoken_commands_switch.set_sensitive(controls_available)
        if self.auto_paste_switch is not None:
            self.auto_paste_switch.set_sensitive(controls_available and self.focus_tracker is not None)
        if self.global_recording_key is not None:
            self.global_recording_key.set_sensitive(controls_available)
        if self.incognito_switch is not None:
            self.incognito_switch.set_sensitive(controls_available)
        if self.audio_retention is not None:
            self.audio_retention.set_sensitive(controls_available)
        if self.history_retention is not None:
            self.history_retention.set_sensitive(controls_available)
        if self.output_style is not None:
            self.output_style.set_sensitive(not self.pending_incognito and controls_available)
        if self.remember_application_switch is not None:
            self.remember_application_switch.set_sensitive(controls_available)
        if self.language is not None:
            self.language.set_sensitive(controls_available)
        if self.microphone_device is not None:
            self.microphone_device.set_sensitive(controls_available)
        if self.system_audio_device is not None:
            self.system_audio_device.set_sensitive(controls_available)
        if self.refresh_audio_button is not None:
            self.refresh_audio_button.set_sensitive(controls_available)
        if self.settings_button is not None:
            self.settings_button.set_sensitive(controls_available)
        self.capture_allows_auto_paste = False
        self._apply_incognito_controls()
        self._update_capture_status_rows()

    def _set_status(self, message: str) -> None:
        """Show a Capture-scoped workflow state without leaking into Meeting."""
        if self.status_label is not None:
            self.status_label.remove_css_class("error")
            self.status_label.set_label(message)
            self.status_label.set_tooltip_text(message)

    def _show_live_capture(self, phase: str) -> None:
        """Expose the transient recording bar and hide the idle summary."""
        self._present_recording_bar(self._recording_bar_state(kind=RECORDING_KIND_RECORDING, detail=phase, preview=""))
        if self.capture_summary_box is not None:
            self.capture_summary_box.set_visible(False)
        self._update_capture_status_rows()

    def _present_preparing_bar(self) -> None:
        """Reveal the bar while recognition readiness is still being prepared."""
        self._present_recording_bar(
            self._recording_bar_state(
                kind=RECORDING_KIND_PREPARING,
                detail="Preparing recognition readiness…",
                preview="Preparing recognition…",
            )
        )

    def _present_recording_bar(self, state: RecordingBarState) -> None:
        """Project one snapshot into both bounded recording surfaces."""
        MluvaApplication._clear_overlay_timeout(self)
        self.overlay_review_identifier = None
        workspace = getattr(self, "conversation_workspace", None)
        if workspace is not None:
            workspace.set_live(
                "Preparing…" if state.kind == "preparing" else state.elapsed,
                state.preview,
                recording=state.kind == RECORDING_KIND_RECORDING,
            )
        revealed = state.kind in {RECORDING_KIND_PREPARING, RECORDING_KIND_RECORDING}
        if self.recording_bar is not None:
            revealed = self.recording_bar.present(state)
        if self.recording_bar_slot is not None:
            self.recording_bar_slot.set_reveal_child(revealed)
        if self.recording_overlay_publisher is not None:
            detail, route = _overlay_detail_and_route(state.detail)
            overlay_state = RecordingOverlayState(
                phase=state.kind,
                detail=detail,
                elapsed_seconds=_elapsed_seconds(state.elapsed),
                mode=state.mode,
                route=route,
                level=state.level,
                preview=state.preview,
                delivery=state.delivery,
                smooth_scrolling=_smooth_motion_enabled(self.config),
                scroll_duration_ms=self.config.scroll_duration_ms,
                scroll_lookahead_lines=self.config.scroll_lookahead_lines,
                widget_position=self.config.widget_position,
            )
            if not self.recording_overlay_publisher.publish(overlay_state):
                self.recording_overlay_publisher = None

    def _hide_recording_bar(self, *, keep_overlay: bool = False) -> None:
        """Erase the capture bar; final Live reconciliation still owns the shell surface."""
        if not keep_overlay:
            self.overlay_review_identifier = None
        if self.recording_bar is not None:
            self.recording_bar.clear()
        if self.recording_bar_slot is not None:
            self.recording_bar_slot.set_reveal_child(False)
        if (
            not keep_overlay
            and self.recording_overlay_publisher is not None
            and not self.recording_overlay_publisher.clear()
        ):
            self.recording_overlay_publisher = None

    def _clear_overlay_timeout(self) -> None:
        """Prevent an older ready indicator from hiding a newer recording."""
        timeout_id = getattr(self, "overlay_timeout_id", None)
        if timeout_id is not None:
            GLib.source_remove(timeout_id)
            self.overlay_timeout_id = None

    def _publish_completion_status(self, phase: str, detail: str) -> None:
        """Keep bottom-screen feedback visible through processing and clipboard readiness."""
        MluvaApplication._clear_overlay_timeout(self)
        publisher = getattr(self, "recording_overlay_publisher", None)
        workspace = getattr(self, "conversation_workspace", None)
        preview = ""
        if workspace is not None and phase == "processing":
            buffer = workspace.live_text.get_buffer()
            preview = buffer.get_text(buffer.get_start_iter(), buffer.get_end_iter(), False)
        if publisher is not None:
            publisher.publish(
                RecordingOverlayState(
                    phase=phase, detail=detail, preview=preview, widget_position=self.config.widget_position
                )
            )
            if phase in {"copied", "error"}:
                self.overlay_timeout_id = GLib.timeout_add_seconds(
                    5 if phase == "copied" else 10,
                    MluvaApplication._expire_completion_status,
                    self,
                )
        if workspace is not None and phase == "processing":
            workspace.set_live_status("Processing…")

    def _expire_completion_status(self) -> bool:
        """Dismiss a terminal indicator without touching microphone or clipboard state."""
        self.overlay_timeout_id = None
        if self.recording_overlay_publisher is not None:
            self.recording_overlay_publisher.clear()
        return GLib.SOURCE_REMOVE

    def _recording_bar_state(
        self,
        *,
        kind: str,
        detail: str = "",
        preview: str | None = None,
        elapsed: str = "00:00",
        level: float = 0.0,
    ) -> RecordingBarState:
        """Project one immutable recording-bar snapshot from live app state.

        Callers own the kind: the live tick only runs while a recorder process
        exists, preparing is driven by the preparation lifecycle, and terminal
        transitions hide the bar through :meth:`_hide_recording_bar`.
        """
        recording = self.recorder is not None and self.recorder.process is not None
        mode_index = (
            self.mode.get_selected()
            if self.mode is not None and recording
            else CAPTURE_MODE_IDS.index(self.pending_mode)
        )
        mode_label = CAPTURE_MODE_LABELS[mode_index]
        delivery = "Paste armed" if MluvaApplication._automatic_paste_armed(self) else "Copy only"
        if not preview:
            # "Listening" is a microphone-live promise only recording can make.
            preview = "Preparing recognition…" if kind == RECORDING_KIND_PREPARING else "Listening…"
        quiet = preview == "Waiting for speech…"
        return RecordingBarState(
            kind=kind,
            detail=detail,
            elapsed=elapsed,
            mode=mode_label,
            delivery=delivery,
            level=level,
            preview=preview,
            quiet=quiet,
        )

    def _clear_live_capture(self) -> None:
        """Erase every volatile projection once capture reaches a terminal state."""
        workspace = getattr(self, "conversation_workspace", None)
        if workspace is not None:
            workspace.finish_live()
        self._hide_recording_bar()
        if self.capture_summary_box is not None:
            self.capture_summary_box.set_visible(True)
        self._update_capture_status_rows()

    def _export_diagnostics(self, _button: Gtk.Button) -> None:
        """Export reviewed configuration and timing fields without user content."""
        try:
            output_path = self.diagnostics_store.export(self.data_directory / "exports", self.config)
        except Exception as error:
            self._set_status(f"Diagnostics export failed: {error}")
            return
        self._set_status(f"Privacy-safe diagnostics exported to {output_path}")

    def _record_app_diagnostic(
        self,
        stage: DiagnosticStage,
        outcome: DiagnosticOutcome,
        duration_seconds: float,
        provider: DiagnosticProvider = DiagnosticProvider.PIPEWIRE,
    ) -> None:
        """Record app-owned capture timing without allowing observability to break capture."""
        if self.pending_session_identifier is None or self.pending_incognito:
            return
        self._record_session_diagnostic(
            self.pending_session_identifier,
            self.pending_mode,
            stage,
            provider,
            outcome,
            duration_seconds,
        )

    def _record_session_diagnostic(
        self,
        session_identifier: str,
        mode: str,
        stage: DiagnosticStage,
        provider: DiagnosticProvider,
        outcome: DiagnosticOutcome,
        duration_seconds: float,
    ) -> None:
        """Write one controlled event while keeping diagnostics outside the critical path."""
        try:
            self.diagnostics_store.record(
                session_identifier,
                mode,
                stage,
                provider,
                outcome,
                duration_seconds,
            )
        except Exception:
            pass

    def _language_changed(self, *_args: object) -> None:
        """Persist one reviewed Scribe language or documented automatic detection."""
        if self.language is None:
            return
        selected = self.language.get_selected()
        if selected >= len(self.language_codes):
            return
        language_code = self.language_codes[selected]
        if language_code == self.config.language_code:
            return
        new_config = replace(self.config, language_code=language_code)
        try:
            save_config(new_config, self.config_path)
        except Exception as error:
            self.language.set_selected(self.language_codes.index(self.config.language_code))
            self._set_status(f"Transcription language could not be saved: {error}")
            return
        self.config = new_config
        self._synchronize_workflow_config()
        language_name = dict(TRANSCRIPTION_LANGUAGE_OPTIONS).get(language_code, language_code)
        self._set_status(f"Transcription language saved: {language_name}.")

    def _populate_audio_device_rows(self) -> None:
        """Populate device controls from one metadata snapshot while retaining disconnected targets."""
        if self.microphone_device is None or self.system_audio_device is None:
            return
        self.refreshing_audio_devices = True
        try:
            microphone_devices = list(self.pipewire_catalog.microphones)
            system_outputs = list(self.pipewire_catalog.system_outputs)
            self.microphone_targets = [None, *(device.target for device in microphone_devices)]
            microphone_names = ["Default (automatic)", *(device.name for device in microphone_devices)]
            if self.config.microphone_target not in self.microphone_targets:
                self.microphone_targets.append(self.config.microphone_target)
                microphone_names.append(
                    self.pipewire_catalog.display_name(
                        PipeWireDeviceKind.MICROPHONE,
                        self.config.microphone_target,
                    )
                )
            self.system_audio_targets = [None, *(device.target for device in system_outputs)]
            system_output_names = ["Default (automatic)", *(device.name for device in system_outputs)]
            if self.config.system_audio_target not in self.system_audio_targets:
                self.system_audio_targets.append(self.config.system_audio_target)
                system_output_names.append(
                    self.pipewire_catalog.display_name(
                        PipeWireDeviceKind.SYSTEM_OUTPUT,
                        self.config.system_audio_target,
                    )
                )
            self.microphone_device.set_model(Gtk.StringList.new(microphone_names))
            self.microphone_device.set_selected(self.microphone_targets.index(self.config.microphone_target))
            self.system_audio_device.set_model(Gtk.StringList.new(system_output_names))
            self.system_audio_device.set_selected(self.system_audio_targets.index(self.config.system_audio_target))
            self.microphone_device.set_subtitle(
                self.pipewire_catalog_error or "Used by Dictation and explicit Meeting capture"
            )
        finally:
            self.refreshing_audio_devices = False
        self._update_meeting_audio_routes()

    def _audio_device_changed(self, *_args: object) -> None:
        """Persist selected PipeWire node names and apply them only to future captures."""
        if self.refreshing_audio_devices or self.microphone_device is None or self.system_audio_device is None:
            return
        microphone_index = self.microphone_device.get_selected()
        system_output_index = self.system_audio_device.get_selected()
        if microphone_index >= len(self.microphone_targets) or system_output_index >= len(self.system_audio_targets):
            return
        microphone_target = self.microphone_targets[microphone_index]
        system_audio_target = self.system_audio_targets[system_output_index]
        new_config = replace(
            self.config,
            microphone_target=microphone_target,
            system_audio_target=system_audio_target,
        )
        try:
            save_config(new_config, self.config_path)
        except Exception as error:
            self._set_status(f"Audio routing could not be saved: {error}")
            self._populate_audio_device_rows()
            return
        self.config = new_config
        self._synchronize_workflow_config()
        if self.recorder is not None:
            self.recorder.target = microphone_target
        if self.meeting_recorder is not None:
            self.meeting_recorder.microphone_target = microphone_target
            self.meeting_recorder.system_target = system_audio_target
        self._update_meeting_audio_routes()
        self._set_status("Audio routing saved for future captures.")

    def _refresh_audio_devices(self, _button: Gtk.Button) -> None:
        """Refresh PipeWire metadata on explicit request without opening an audio stream."""
        if (
            self.capture_preparing
            or self.capture_processing
            or self.retry_in_progress
            or self.meeting_processing
            or self.meeting_retry_in_progress
            or self._meeting_capture_active()
            or (self.recorder is not None and self.recorder.process is not None)
        ):
            self._set_status("Finish the active capture or retry before refreshing audio devices.")
            return
        try:
            self.pipewire_catalog = PipeWireDeviceCatalog.from_system()
            self.pipewire_catalog_error = None
        except PipeWireCatalogError as error:
            self.pipewire_catalog_error = str(error)
            self._set_status(str(error))
            self._populate_audio_device_rows()
            return
        self._populate_audio_device_rows()
        self._set_status(
            f"Audio devices refreshed: {len(self.pipewire_catalog.microphones)} microphone source(s), "
            f"{len(self.pipewire_catalog.system_outputs)} system output(s)."
        )

    def _update_meeting_audio_routes(self) -> None:
        """Show the exact configured routes on the explicit Meeting page."""
        if self.meeting_page is None:
            return
        self.meeting_page.set_audio_routes(
            self.pipewire_catalog.display_name(
                PipeWireDeviceKind.MICROPHONE,
                self.config.microphone_target,
            ),
            self.pipewire_catalog.display_name(
                PipeWireDeviceKind.SYSTEM_OUTPUT,
                self.config.system_audio_target,
            ),
        )

    def _synchronize_workflow_config(self) -> None:
        """Publish one persisted immutable config to both independent workflows."""
        if self.workflow is not None:
            self.workflow.config = self.config
        if self.meeting_workflow is not None:
            self.meeting_workflow.config = self.config

    def _privacy_setting_changed(self, *_args: object) -> None:
        """Persist idle privacy controls and immediately enforce history retention."""
        if self.incognito_switch is None or self.audio_retention is None or self.history_retention is None:
            return
        new_config = replace(
            self.config,
            incognito_mode=self.incognito_switch.get_active(),
            audio_retention_policy=self._selected_audio_retention(),
            history_retention_days=(0, 7, 30, 90)[self.history_retention.get_selected()],
        )
        try:
            save_config(new_config, self.config_path)
        except Exception as error:
            self._set_status(f"Privacy settings could not be saved: {error}")
            self.config = replace(self.config, incognito_mode=new_config.incognito_mode)
            if self.workflow is not None:
                self.workflow.config = self.config
            if self.meeting_workflow is not None:
                self.meeting_workflow.config = self.config
            self._apply_incognito_controls()
            self._set_status("Privacy settings could not be saved. The Incognito choice applies for this session.")
            return
        self.config = new_config
        if self.workflow is not None:
            self.workflow.config = self.config
        if self.meeting_workflow is not None:
            self.meeting_workflow.config = self.config
        if self.meeting_page is not None and not self._meeting_capture_active() and not self.meeting_processing:
            self.meeting_page.set_privacy(self.config.incognito_mode)
        self._apply_incognito_controls()
        try:
            self._prune_history()
        except Exception as error:
            self._set_status(f"Privacy settings saved, but history retention failed: {error}")
            return
        if self.history_page is not None:
            self.history_page.refresh()
        self._history_changed()
        if self.config.incognito_mode:
            self._set_status(
                "Incognito enabled for new sessions: no local history or recovery audio; "
                "Recognition uses your selected speech provider."
            )
        else:
            self._set_status("Privacy and retention settings saved.")

    def _mode_changed(self, *_args: object) -> None:
        """Persist the visible mode globally or for the last identified target application."""
        if self.mode is None:
            return
        self.mode.set_subtitle(capture_mode_description(self.mode.get_selected()))
        selected_mode = CAPTURE_MODE_IDS[self.mode.get_selected()]
        application_identifier = self.profile_application_identifier
        if self.config.remember_per_application and application_identifier is not None:
            try:
                self.personalization_store.select_mode(
                    selected_mode,
                    application_identifier,
                    remember_per_application=True,
                )
            except Exception as error:
                self._set_status(f"Application mode could not be saved: {error}")
                return
            self._set_status("Capture mode saved for the identified application.")
            return
        new_config = replace(self.config, default_mode=selected_mode)
        try:
            save_config(new_config, self.config_path)
        except Exception as error:
            self._set_status(f"Default capture mode could not be saved: {error}")
            return
        self.config = new_config
        if self.workflow is not None:
            self.workflow.config = self.config
        if self.meeting_workflow is not None:
            self.meeting_workflow.config = self.config
        self._set_status("Default capture mode saved.")

    def _global_recording_key_changed(self, *_args: object) -> None:
        """Persist and request a new portal binding while capture is idle."""
        if self.global_recording_key is None:
            return
        selected_index = self.global_recording_key.get_selected()
        if selected_index >= len(FUNCTION_KEY_OPTIONS):
            return
        function_key = FUNCTION_KEY_OPTIONS[selected_index]
        if function_key == self.config.global_recording_key:
            return
        capture_active = (
            self.capture_preparing
            or self.capture_processing
            or (self.recorder is not None and self.recorder.process is not None)
            or self.meeting_processing
            or self.meeting_retry_in_progress
            or self._meeting_capture_active()
        )
        if capture_active:
            self.global_recording_key.set_selected(FUNCTION_KEY_OPTIONS.index(self.config.global_recording_key))
            self._set_status("Stop the active capture before changing the global recording key.")
            return
        new_config = replace(self.config, global_recording_key=function_key)
        try:
            save_config(new_config, self.config_path)
        except Exception as error:
            self.global_recording_key.set_selected(FUNCTION_KEY_OPTIONS.index(self.config.global_recording_key))
            self._set_status(f"Global recording key could not be saved: {error}")
            return
        self.config = new_config
        self.approved_recording_trigger = None
        if self.workflow is not None:
            self.workflow.config = self.config
        if self.meeting_workflow is not None:
            self.meeting_workflow.config = self.config
        if self.global_shortcut_status_row is not None:
            self.global_shortcut_status_row.set_subtitle(
                f"Requesting {function_key} through the desktop portal; approve the replacement if prompted…"
            )
        self._update_capture_status_rows()
        if self.shortcut_service is not None:
            self.shortcut_service.set_recording_key(function_key)
            self._set_status(f"{function_key} saved; the desktop portal is replacing the global binding.")
        else:
            self._set_status(f"{function_key} saved; global shortcuts are disabled for this process.")

    def _general_setting_changed(self, *_args: object) -> None:
        """Persist deterministic transcript and delivery settings while capture is idle."""
        if (
            self.spoken_commands_switch is None
            or self.auto_paste_switch is None
            or self.remember_application_switch is None
        ):
            return
        new_config = replace(
            self.config,
            spoken_commands_enabled=self.spoken_commands_switch.get_active(),
            auto_paste=self.auto_paste_switch.get_active(),
            remember_per_application=self.remember_application_switch.get_active(),
        )
        try:
            save_config(new_config, self.config_path)
        except Exception as error:
            self._set_status(f"General settings could not be saved: {error}")
            return
        self.config = new_config
        if self.workflow is not None:
            self.workflow.config = self.config
        if self.meeting_workflow is not None:
            self.meeting_workflow.config = self.config
        if self.mode is not None:
            visible_mode = self.personalization_store.selected_mode(
                self.profile_application_identifier,
                self.config.remember_per_application,
                self.config.default_mode,
            )
            self.mode.set_selected(CAPTURE_MODE_IDS.index(visible_mode))
        self._refresh_style_controls()
        self._set_status("General settings saved for new captures.")

    def _open_prompt_editor(self, identifier: str) -> None:
        """Deep-link without executing, replacing or silently discarding an existing edit."""
        if self.prompt_editor is not None:
            self._show_toast("Finish or cancel the open prompt edit first.")
            return
        self.prompt_editor = PromptEditor(
            self.prompt_store, identifier, self._prompts_changed, lambda: not self.config.incognito_mode
        )
        self.prompt_editor.connect("closed", lambda _dialog: setattr(self, "prompt_editor", None))
        self.prompt_editor.present(
            self.settings_dialog
            if self.settings_dialog is not None and self.settings_dialog.get_mapped()
            else self.window
        )

    def _prompts_changed(self) -> None:
        """Refresh discovery without touching active request/session snapshots or manual draft text."""
        self._refresh_style_controls()
        if self.prompts_page is not None:
            self.prompts_page.refresh()
        if self.personalization_page is not None:
            self.personalization_page.refresh()
        self._show_toast("Prompt saved · next request, or next recording for Live")

    def _refresh_style_controls(self) -> None:
        """Rebuild capture output modes after load or custom-style mutation."""
        if hasattr(self, "prompt_store"):
            self.prompt_store.sync_styles((*BUILT_IN_STYLES, *self.personalization_store.custom_styles))
        workspace = getattr(self, "conversation_workspace", None)
        if workspace is not None:
            workspace.prompt_store = self.prompt_store
            workspace.edit_prompt = self._open_prompt_editor
            workspace.set_saved_prompts(
                [(style.name, "style-" + style.identifier.lower()) for style in self.personalization_store.styles]
            )
        identifier = getattr(self, "overlay_review_identifier", None)
        if identifier is not None:
            self._publish_review(identifier, "rewriting" if self.rewrite_identifier == identifier else "ready")
        if self.output_style is None:
            return
        application_identifier = self.profile_application_identifier if self.config.remember_per_application else None
        selected_style = self.personalization_store.selected_style(
            application_identifier=application_identifier,
            remember_per_application=self.config.remember_per_application,
        )
        selected_identifier = selected_style.identifier if selected_style is not None else None
        self.style_identifiers = [None, *(style.identifier for style in self.personalization_store.styles)]
        labels = ["Faithful (no saved style)", *(style.name for style in self.personalization_store.styles)]
        self.refreshing_style_controls = True
        try:
            self.output_style.set_model(Gtk.StringList.new(labels))
            selected_index = (
                self.style_identifiers.index(selected_identifier)
                if selected_identifier in self.style_identifiers
                else 0
            )
            self.output_style.set_selected(selected_index)
            self._update_style_instructions()
        finally:
            self.refreshing_style_controls = False

    def _output_style_changed(self, *_args: object) -> None:
        """Persist the visible global style and show its complete instructions."""
        if self.refreshing_style_controls or self.output_style is None:
            return
        index = self.output_style.get_selected()
        identifier = self.style_identifiers[index] if index < len(self.style_identifiers) else None
        try:
            self.personalization_store.select_style(
                identifier,
                application_identifier=(
                    self.profile_application_identifier if self.config.remember_per_application else None
                ),
                remember_per_application=self.config.remember_per_application,
            )
        except Exception as error:
            self._set_status(f"Output style could not be saved: {error}")
            return
        self._update_style_instructions()
        self._set_status(
            "Output style saved for the identified application."
            if self.config.remember_per_application and self.profile_application_identifier is not None
            else "Default output style saved for new captures."
        )

    def _update_style_instructions(self) -> None:
        """Keep the selected output mode's full instructions visible before capture."""
        if self.output_style is None or self.output_style_instructions is None:
            return
        index = self.output_style.get_selected()
        identifier = self.style_identifiers[index] if index < len(self.style_identifiers) else None
        style = self.personalization_store.style(identifier)
        instructions = (
            "No generative style rewrite. Local spoken structure, dictionary, and explicit snippets still apply."
            if style is None
            else style.instructions
        )
        self.output_style_instructions.set_subtitle(instructions)
        style_name = "Faithful" if style is None else style.name
        self.output_style.set_tooltip_text(f"{style_name}: {instructions}")

    def _show_personalization_page(self, _button: Gtk.Button) -> None:
        """Open the existing custom-mode editor from the main Capture surface."""
        if self.page_stack is not None:
            self.page_stack.set_visible_child_name("personalization")

    def _selected_output_style(self) -> SavedStyle | None:
        """Resolve the current capture control without consulting an external application."""
        if self.output_style is None:
            return None
        index = self.output_style.get_selected()
        identifier = self.style_identifiers[index] if index < len(self.style_identifiers) else None
        return self.personalization_store.style(identifier)

    def _capture_mode_for_application(self, fallback: str) -> str:
        """Freeze a remembered application mode or establish its first local profile."""
        application_identifier = self.pending_application_identifier
        if not self.config.remember_per_application or application_identifier is None:
            return fallback
        if application_identifier in self.personalization_store.application_modes:
            return self.personalization_store.selected_mode(
                application_identifier,
                remember_per_application=True,
                fallback=fallback,
            )
        try:
            self.personalization_store.select_mode(
                fallback,
                application_identifier,
                remember_per_application=True,
            )
        except Exception as error:
            self._set_status(f"Application mode could not be remembered: {error}")
        return fallback

    def _freeze_output_style(self) -> None:
        """Freeze a local style profile for this session without provider disclosure."""
        self.pending_style_identifier = None
        self.pending_style = None
        self.pending_use_saved_style = False
        if self.pending_incognito or self.pending_mode == "command":
            return
        application_identifier = self.pending_application_identifier
        if self.config.remember_per_application and application_identifier is not None:
            style = self.personalization_store.selected_style(
                application_identifier=None,
                remember_per_application=False,
            )
            remembered = self.personalization_store.selected_style(
                application_identifier,
                remember_per_application=True,
            )
            if self.personalization_store.has_application_style_selection(application_identifier):
                style = remembered
                self._select_output_style_control(remembered.identifier if remembered is not None else None)
            else:
                self._select_output_style_control(style.identifier if style is not None else None)
                try:
                    self.personalization_store.select_style(
                        style.identifier if style is not None else None,
                        application_identifier,
                        remember_per_application=True,
                    )
                except Exception as error:
                    self._set_status(f"Application style could not be remembered: {error}")
        else:
            style = self._selected_output_style()
        if style is not None:
            self.pending_style_identifier = style.identifier
            self.pending_style = style
            self.pending_use_saved_style = True

    def _select_output_style_control(self, identifier: str | None) -> None:
        """Reflect a frozen application style without rewriting global selection state."""
        if self.output_style is None or identifier not in self.style_identifiers:
            return
        self.refreshing_style_controls = True
        try:
            self.output_style.set_selected(self.style_identifiers.index(identifier))
            self._update_style_instructions()
        finally:
            self.refreshing_style_controls = False

    def _selected_audio_retention(self) -> AudioRetentionPolicy:
        """Map the privacy row selection to its frozen workflow policy."""
        if self.audio_retention is None:
            return self.config.audio_retention_policy
        return (AudioRetentionPolicy.NEVER, AudioRetentionPolicy.FAILURES, AudioRetentionPolicy.ALWAYS)[
            self.audio_retention.get_selected()
        ]

    def _apply_incognito_controls(self) -> None:
        """Suspend incompatible controls in Incognito and restore the user's prior cleanup choice."""
        if self.incognito_switch is None or self.cleanup_switch is None:
            return
        incognito = self.incognito_switch.get_active()
        if incognito:
            self._cancel_live_rewrite()
            self._cancel_titles()
        if self.automatic_titles_switch is not None:
            self.automatic_titles_switch.set_sensitive(not incognito)
        if incognito and getattr(self, "overlay_review_identifier", None) is not None:
            self._dismiss_review()
        workspace = getattr(self, "conversation_workspace", None)
        if workspace is not None:
            workspace.set_private(incognito)
        if incognito and getattr(self, "rewrite_client", None) is not None:
            client = self.rewrite_client
            self.rewrite_client = None
            self.rewrite_identifier = None
            threading.Thread(target=client.cancel, daemon=True).start()
            if workspace is not None:
                workspace.set_busy(False, "Rewrite cancelled for Incognito.")
        if incognito and self.cleanup_before_incognito is None:
            self.cleanup_before_incognito = self.cleanup_switch.get_active()
            self.cleanup_switch.set_active(False)
            self.cleanup_switch.set_subtitle(
                "Unavailable in Incognito because Codex processing cannot guarantee ephemeral handling"
            )
        elif not incognito and self.cleanup_before_incognito is not None:
            self.cleanup_switch.set_active(self.cleanup_before_incognito)
            self.cleanup_before_incognito = None
            self.cleanup_switch.set_subtitle(
                "Remove obvious filler and repair punctuation through the selected rewrite provider"
            )
        recorder_idle = (
            (self.recorder is None or self.recorder.process is None)
            and not self.capture_preparing
            and not self.capture_processing
            and not self._meeting_capture_active()
            and not self.meeting_processing
        )
        has_pending_review = self.pending_command_result is not None or self.scratchpad_store.draft is not None
        self.cleanup_switch.set_sensitive(not incognito and recorder_idle and not has_pending_review)
        if self.output_style is not None:
            self.output_style.set_sensitive(not incognito and recorder_idle and not has_pending_review)

    def _prune_history(self) -> int:
        """Apply configured age retention while preserving the unresolved Scratchpad row."""
        excluded_identifiers: frozenset[str] = frozenset()
        draft = self.scratchpad_store.draft
        if draft is not None and draft.history_identifier is not None:
            excluded_identifiers = frozenset((draft.history_identifier,))
        if self.retry_identifier is not None:
            excluded_identifiers = excluded_identifiers | frozenset((self.retry_identifier,))
        if self.pending_command_result is not None and self.pending_command_result.history_entry is not None:
            excluded_identifiers = excluded_identifiers | frozenset(
                (self.pending_command_result.history_entry.identifier,)
            )
        return self.history_store.prune_older_than(
            self.config.history_retention_days,
            excluded_identifiers=excluded_identifiers,
        )

    def _restore_scratchpad(self) -> None:
        """Restore unresolved editable work without requiring an ElevenLabs credential."""
        if self.scratchpad_store.persistence_error is not None:
            self._set_status(
                "The Scratchpad recovery document is malformed and was preserved. Repair it before saving "
                "another persistent Scratchpad draft."
            )
            return
        draft = self.scratchpad_store.draft
        if draft is None:
            return
        if draft.incognito:
            self.scratchpad_store.clear(remove_audio=True)
            self._set_status("Discarded invalid persisted Incognito recovery state.")
            return
        self.editing_scratchpad = True
        if self.mode is not None:
            self.mode.set_selected(2)
        if self.output_buffer is not None:
            self.output_buffer.set_text(draft.text)
        if self.scratchpad_actions is not None:
            self.scratchpad_actions.set_visible(True)
        if self.mode is not None:
            self.mode.set_sensitive(False)
        if self.output_view is not None:
            self.output_view.set_editable(True)
        self._reset_record_button()
        self._set_status("Recovered an unresolved Scratchpad draft and its source audio.")

    def _accept_command_preview(self, _button: Gtk.Button) -> None:
        """Deliver one reviewed Command result only after restoring its captured target."""
        result = self.pending_command_result
        if result is None:
            return
        delivery_started_at = time.monotonic()
        target = self.pending_command_target
        try:
            restored_target = target is not None and self.config.auto_paste and target.restore()
        except Exception:
            restored_target = False
        try:
            receipt = (
                deliver_text(
                    result.output_text,
                    auto_paste=True,
                    confirm_paste=lambda: target.confirm_insertion(result.output_text),
                    insert_directly=target.insert_text,
                    authorize_keyboard_paste=target.restore,
                )
                if restored_target and target is not None
                else deliver_text(result.output_text, auto_paste=False)
            )
        except Exception as error:
            self._record_session_diagnostic(
                result.session_identifier,
                result.mode,
                DiagnosticStage.DELIVERY,
                DiagnosticProvider.DESKTOP,
                DiagnosticOutcome.FAILED,
                time.monotonic() - delivery_started_at,
            )
            self._set_status(f"Command result remains in preview because delivery failed: {error}")
            return
        delivery_seconds = time.monotonic() - delivery_started_at
        self._record_session_diagnostic(
            result.session_identifier,
            result.mode,
            DiagnosticStage.DELIVERY,
            DiagnosticProvider.DESKTOP,
            (
                DiagnosticOutcome.SAFE_FALLBACK
                if receipt.paste_dispatched and not receipt.pasted
                else DiagnosticOutcome.COMPLETED
            ),
            delivery_seconds,
        )
        if target is not None and self.config.auto_paste and not restored_target:
            receipt = replace(
                receipt,
                guidance=f"{receipt.guidance} The captured target could not be restored, so no paste was attempted.",
            )
        history_warning: str | None = None
        if result.history_entry is not None:
            try:
                self.history_store.mark_delivered(
                    result.history_entry.identifier,
                    result.output_text,
                    receipt.history_outcome,
                    retain_audio=self.pending_audio_retention is AudioRetentionPolicy.ALWAYS,
                    delivery_ms=round(delivery_seconds * 1_000),
                )
            except Exception as error:
                history_warning = f"Local history could not record acceptance: {error}"
            if target is not None:
                self._remember_history_delivery_target(result.history_entry.identifier, target)
        self._clear_command_preview(clear_output=False)
        if self.history_page is not None:
            self.history_page.refresh()
        status = receipt.guidance if history_warning is None else f"{receipt.guidance} {history_warning}"
        self._set_status(status)

    def _discard_command_preview(self, _button: Gtk.Button) -> None:
        """Discard a proposed Command result without changing the captured target or clipboard."""
        result = self.pending_command_result
        if result is None:
            return
        self._record_session_diagnostic(
            result.session_identifier,
            result.mode,
            DiagnosticStage.DELIVERY,
            DiagnosticProvider.DESKTOP,
            DiagnosticOutcome.CANCELLED,
            0,
        )
        history_warning: str | None = None
        if result.history_entry is not None:
            try:
                self.history_store.mark_delivered(
                    result.history_entry.identifier,
                    result.output_text,
                    "discarded",
                    retain_audio=self.pending_audio_retention is AudioRetentionPolicy.ALWAYS,
                )
            except Exception as error:
                history_warning = f" Local history could not record the discard: {error}"
        self._clear_command_preview(clear_output=True)
        if self.history_page is not None:
            self.history_page.refresh()
        self._set_status(f"Command preview discarded; the target was not changed.{history_warning or ''}")

    def _clear_command_preview(self, clear_output: bool) -> None:
        """Release in-memory target context after one explicit preview decision."""
        self.pending_command_result = None
        self.pending_command_target = None
        if self.command_actions is not None:
            self.command_actions.set_visible(False)
        if self.command_source_label is not None:
            self.command_source_label.set_visible(False)
            self.command_source_label.set_label("")
        if self.output_view is not None:
            self.output_view.set_editable(True)
        if clear_output and self.output_buffer is not None:
            self.output_buffer.set_text("")
        self._reset_record_button()

    def _scratchpad_text_changed(self, buffer: Gtk.TextBuffer) -> None:
        """Persist every accepted editor change while an unresolved draft is active."""
        if not self.editing_scratchpad or self.scratchpad_store.draft is None:
            return
        start, end = buffer.get_bounds()
        text = buffer.get_text(start, end, include_hidden_chars=True)
        try:
            draft = replace(self.scratchpad_store.draft, text=text)
            self.scratchpad_store.save(draft, persist=not draft.incognito)
        except Exception as error:
            self._set_status(f"Scratchpad edit could not be persisted: {error}")

    def _copy_and_resolve_scratchpad(self, _button: Gtk.Button) -> None:
        """Copy the edited draft and erase recovery audio only after explicit acceptance."""
        draft = self.scratchpad_store.draft
        if draft is None or self.output_buffer is None:
            return
        start, end = self.output_buffer.get_bounds()
        text = self.output_buffer.get_text(start, end, include_hidden_chars=True).strip()
        if not text:
            self._set_status("The Scratchpad is empty; edit it or delete the draft.")
            return
        delivery_started_at = time.monotonic()
        try:
            receipt = deliver_text(text, auto_paste=False)
            delivery_seconds = time.monotonic() - delivery_started_at
            policy = AudioRetentionPolicy(draft.audio_retention_policy)
            retain_audio = not draft.incognito and policy is AudioRetentionPolicy.ALWAYS
            if draft.history_identifier is not None:
                self.history_store.mark_delivered(
                    draft.history_identifier,
                    text,
                    "copied",
                    retain_audio=retain_audio,
                    delivery_ms=round(delivery_seconds * 1_000),
                )
            self.scratchpad_store.clear(remove_audio=draft.history_identifier is None)
        except Exception as error:
            if draft.session_identifier is not None and not draft.incognito:
                self._record_session_diagnostic(
                    draft.session_identifier,
                    "scratchpad",
                    DiagnosticStage.DELIVERY,
                    DiagnosticProvider.DESKTOP,
                    DiagnosticOutcome.FAILED,
                    time.monotonic() - delivery_started_at,
                )
            self._set_status(f"Scratchpad acceptance did not fully resolve: {error}")
            return
        if draft.session_identifier is not None and not draft.incognito:
            self._record_session_diagnostic(
                draft.session_identifier,
                "scratchpad",
                DiagnosticStage.DELIVERY,
                DiagnosticProvider.DESKTOP,
                DiagnosticOutcome.COMPLETED,
                delivery_seconds,
            )
        try:
            self._prune_history()
        except Exception as error:
            receipt = replace(receipt, guidance=f"{receipt.guidance} History retention failed: {error}")
        self.editing_scratchpad = False
        if self.scratchpad_actions is not None:
            self.scratchpad_actions.set_visible(False)
        self._reset_record_button()
        if self.history_page is not None:
            self.history_page.refresh()
        self._history_changed()
        self._set_status(receipt.guidance)

    def _delete_current_scratchpad(self, _button: Gtk.Button) -> None:
        """Permanently delete the active draft, retained recording, and history row."""
        draft = self.scratchpad_store.draft
        if draft is None:
            return
        try:
            self._delete_scratchpad_draft(draft)
        except Exception as error:
            self._set_status(f"Scratchpad deletion failed: {error}")
            return
        if draft.session_identifier is not None and not draft.incognito:
            self._record_session_diagnostic(
                draft.session_identifier,
                "scratchpad",
                DiagnosticStage.DELIVERY,
                DiagnosticProvider.DESKTOP,
                DiagnosticOutcome.CANCELLED,
                0,
            )
        self.editing_scratchpad = False
        if self.output_buffer is not None:
            self.output_buffer.set_text("")
        if self.scratchpad_actions is not None:
            self.scratchpad_actions.set_visible(False)
        self._reset_record_button()
        if self.history_page is not None:
            self.history_page.refresh()
        self._history_changed()
        self._set_status("Scratchpad draft and recovery audio permanently deleted.")

    def _confirm_delete_current_scratchpad(self, _button: Gtk.Button) -> None:
        """Require an explicit destructive confirmation before erasing recoverable work."""
        if self.scratchpad_store.draft is None or self.window is None:
            return
        dialog = Adw.AlertDialog.new(
            "Delete this Notes draft?",
            "The draft, its history record, and retained recovery audio will be permanently deleted.",
        )
        dialog.add_response("cancel", "Cancel")
        dialog.add_response("delete", "Delete permanently")
        dialog.set_default_response("cancel")
        dialog.set_close_response("cancel")
        dialog.set_response_appearance("delete", Adw.ResponseAppearance.DESTRUCTIVE)
        dialog.choose(self.window, None, self._scratchpad_delete_chosen)

    def _scratchpad_delete_chosen(
        self,
        dialog: Adw.AlertDialog,
        result: Gio.AsyncResult,
        _user_data: object = None,
    ) -> None:
        """Apply the destructive Notes decision returned by the native alert."""
        if dialog.choose_finish(result) == "delete":
            self._delete_current_scratchpad(Gtk.Button())

    def _copy_text(self, text: str) -> None:
        """Copy history text and expose delivery failure without mutating the entry."""
        try:
            receipt = deliver_text(text, auto_paste=False)
        except Exception as error:
            self._show_toast(f"Copy failed: {error}")
            return
        self._show_toast(receipt.guidance)

    def _can_retry_history_delivery(self, entry: HistoryEntry) -> bool:
        """Return whether this process still owns the exact target needed for an explicit re-paste."""
        return (
            bool(entry.delivered_text)
            and entry.delivery_outcome != "draft"
            and entry.identifier in self.history_delivery_targets
            and self.config.auto_paste
        )

    def _history_changed(self) -> None:
        """Refresh correction-derived personalization after a local history mutation."""
        if self.personalization_page is not None:
            self.personalization_page.refresh()
        identifier = getattr(self, "overlay_review_identifier", None)
        if identifier is not None:
            self._publish_review(identifier, "rewriting" if self.rewrite_identifier == identifier else "ready")
        workspace = getattr(self, "conversation_workspace", None)
        if workspace is not None:
            workspace.refresh_history()
            if workspace.entry is not None:
                try:
                    entry = self.history_store.find(workspace.entry.identifier)
                except KeyError:
                    workspace.show_conversation(None, [])
                else:
                    workspace.show_conversation(entry, self.conversation_store.replies(entry.identifier))

    def _retry_history_delivery(self, entry: HistoryEntry) -> None:
        """Paste to an exact retained target when safe, otherwise copy for manual recovery."""
        if not entry.delivered_text:
            self._set_status("This history entry has no recognized text to paste or copy.")
            return
        delivery_started_at = time.monotonic()
        target = (
            self.history_delivery_targets[entry.identifier]
            if entry.identifier in self.history_delivery_targets
            else None
        )
        restored_target = False
        if target is not None and self.config.auto_paste:
            try:
                restored_target = target.restore()
            except Exception:
                restored_target = False
            if not restored_target:
                self.history_delivery_targets.pop(entry.identifier, None)
        try:
            receipt = (
                deliver_text(
                    entry.delivered_text,
                    auto_paste=True,
                    confirm_paste=lambda: target.confirm_insertion(entry.delivered_text),
                    insert_directly=target.insert_text,
                    authorize_keyboard_paste=target.restore,
                )
                if restored_target and target is not None
                else deliver_text(entry.delivered_text, auto_paste=False)
            )
        except Exception as error:
            self._set_status(f"Delivery retry failed: {error}")
            return
        if target is not None and self.config.auto_paste and not restored_target:
            receipt = replace(
                receipt,
                guidance=f"{receipt.guidance} The retained target was stale, so no paste was attempted.",
            )
        try:
            policy = AudioRetentionPolicy(entry.audio_retention_policy or AudioRetentionPolicy.FAILURES.value)
            self.history_store.mark_delivered(
                entry.identifier,
                entry.delivered_text,
                receipt.history_outcome,
                retain_audio=policy is AudioRetentionPolicy.ALWAYS,
                delivery_ms=round((time.monotonic() - delivery_started_at) * 1_000),
            )
        except Exception as error:
            if self.history_page is not None:
                self.history_page.refresh()
            self._set_status(f"{receipt.guidance} History receipt update failed: {error}")
            return
        if self.history_page is not None:
            self.history_page.refresh()
        self._set_status(receipt.guidance)

    def _reprocess_history_entry(self, entry: HistoryEntry) -> None:
        """Apply current deterministic rules locally and leave the result pending explicit delivery."""
        if entry.identifier == self.retry_identifier:
            self._set_status("Wait for the active transcription retry before reprocessing this entry.")
            return
        command_result = self.pending_command_result
        if (
            command_result is not None
            and command_result.history_entry is not None
            and command_result.history_entry.identifier == entry.identifier
        ):
            self._set_status("Apply or discard the active Command preview before reprocessing its history entry.")
            return
        draft = self.scratchpad_store.draft
        if draft is not None and draft.history_identifier == entry.identifier:
            self._set_status("Resolve or delete the active Notes draft before reprocessing its history entry.")
            return
        try:
            reprocessed = reprocess_history_entry(
                self.config,
                self.history_store,
                self.personalization_store,
                entry.identifier,
            )
        except Exception as error:
            self._set_status(f"History reprocessing failed: {error}")
            return
        if self.output_buffer is not None:
            self.output_buffer.set_text(reprocessed.delivered_text)
        if self.history_page is not None:
            self.history_page.refresh()
        self._history_changed()
        self._set_status("Raw transcript reprocessed locally. Review it, then paste or copy explicitly.")

    def _remember_history_delivery_target(
        self,
        identifier: str,
        target: TextTargetSnapshot,
        limit: int = MAX_IN_MEMORY_HISTORY_TARGETS,
    ) -> None:
        """Keep a bounded session-only target cache for explicit History paste actions."""
        self.history_delivery_targets.pop(identifier, None)
        self.history_delivery_targets[identifier] = target.without_selected_text()
        while len(self.history_delivery_targets) > limit:
            oldest_identifier = next(iter(self.history_delivery_targets))
            self.history_delivery_targets.pop(oldest_identifier)

    def _retry_history_recognition(self, entry: HistoryEntry) -> None:
        """Start one background recognition retry without authorizing delivery."""
        if self.workflow is None:
            self._set_status("Recognition retry is unavailable until capture services are configured.")
            return
        if self.meeting_processing or self.meeting_retry_in_progress or self._meeting_capture_active():
            self._set_status("Finish the explicit Meeting capture or retry before retrying dictation audio.")
            return
        if (
            self.capture_preparing
            or self.capture_processing
            or (self.recorder is not None and self.recorder.process is not None)
        ):
            self._set_status("Finish the active recording before retrying retained audio.")
            return
        if self.retry_in_progress:
            self._set_status("A transcription retry is already in progress.")
            return
        self.retry_in_progress = True
        self.retry_identifier = entry.identifier
        if self.record_button is not None:
            self.record_button.set_sensitive(False)
        if self.language is not None:
            self.language.set_sensitive(False)
        if self.microphone_device is not None:
            self.microphone_device.set_sensitive(False)
        if self.system_audio_device is not None:
            self.system_audio_device.set_sensitive(False)
        if self.refresh_audio_button is not None:
            self.refresh_audio_button.set_sensitive(False)
        self._set_status("Retrying transcription from retained audio with ElevenLabs Scribe v2…")
        threading.Thread(
            target=self._retry_history_recognition_worker,
            args=(entry.identifier,),
            name="history-recognition-retry",
            daemon=True,
        ).start()

    def _retry_history_recognition_worker(self, identifier: str) -> None:
        """Run retry network traffic away from GTK's thread."""
        if self.workflow is None:
            return
        try:
            entry = self.workflow.retry_recognition(identifier)
        except Exception as error:
            GLib.idle_add(self._retry_history_recognition_failed, str(error))
            return
        GLib.idle_add(self._retry_history_recognition_finished, entry)

    def _retry_history_recognition_finished(self, entry: HistoryEntry) -> bool:
        """Expose recovered text as a preview that still requires explicit copy."""
        self.retry_in_progress = False
        self.retry_identifier = None
        self._reset_record_button()
        if self.output_buffer is not None:
            self.output_buffer.set_text(entry.delivered_text)
        if self.history_page is not None:
            self.history_page.refresh()
        self._history_changed()
        self._set_status("Transcription recovered. Review it in History, then copy explicitly.")
        if entry.mode == "dictation":
            self._queue_conversation_title(entry)
        return GLib.SOURCE_REMOVE

    def _retry_history_recognition_failed(self, message: str) -> bool:
        """Leave retained audio retryable after another recognition failure."""
        self.retry_in_progress = False
        self.retry_identifier = None
        self._reset_record_button()
        if self.history_page is not None:
            self.history_page.refresh()
        self._set_status(f"Transcription retry failed: {message}. History and retained audio remain recoverable.")
        return GLib.SOURCE_REMOVE

    def _delete_history_entry(self, entry: HistoryEntry) -> bool:
        """Coordinate history deletion with a possibly active Scratchpad draft."""
        if entry.identifier == self.retry_identifier:
            self._set_status("Wait for the active transcription retry before deleting this entry.")
            return False
        command_result = self.pending_command_result
        if (
            command_result is not None
            and command_result.history_entry is not None
            and command_result.history_entry.identifier == entry.identifier
        ):
            self._set_status("Apply or discard the active Command preview before deleting its history entry.")
            return False
        try:
            draft = self.scratchpad_store.draft
            if draft is not None and draft.history_identifier == entry.identifier:
                self._delete_scratchpad_draft(draft)
                self.editing_scratchpad = False
                if self.output_buffer is not None:
                    self.output_buffer.set_text("")
                if self.scratchpad_actions is not None:
                    self.scratchpad_actions.set_visible(False)
                self._reset_record_button()
            else:
                self.history_store.delete(entry.identifier)
        except Exception as error:
            self._set_status(f"History deletion failed: {error}")
            return False
        self.history_delivery_targets.pop(entry.identifier, None)
        return True

    def _delete_scratchpad_draft(self, draft: ScratchpadDraft) -> None:
        """Erase coordinated history and draft state while keeping interrupted deletion retryable."""
        if draft.history_identifier is None:
            self.scratchpad_store.clear(remove_audio=True)
            return
        try:
            self.history_store.delete(draft.history_identifier)
        except KeyError:
            self.scratchpad_store.clear(remove_audio=True)
        else:
            self.scratchpad_store.clear(remove_audio=False)

    def _shortcut_toggled(self) -> bool:
        """Start or stop one auto-paste-eligible capture from the global key."""
        if self.recorder is None or self.record_button is None:
            return GLib.SOURCE_REMOVE
        if self.meeting_processing or self.meeting_retry_in_progress or self._meeting_capture_active():
            self._set_status(f"{self.config.global_recording_key} cannot start or stop explicit Meeting capture.")
            return GLib.SOURCE_REMOVE
        if self.capture_processing:
            self._set_status("The stopped recording is still being processed.")
            return GLib.SOURCE_REMOVE
        if self.capture_preparing:
            self._cancel_capture()
            return GLib.SOURCE_REMOVE
        if self.recorder.process is None:
            self.capture_allows_auto_paste = True
            self._start_capture()
        else:
            self._stop_capture()
        return GLib.SOURCE_REMOVE

    def _shortcut_cancelled(self) -> bool:
        """Cancel once when the compositor activates the approved cancel shortcut."""
        self._cancel_capture()
        return GLib.SOURCE_REMOVE

    def _rewrite_binding_changed(self, trigger: str | None) -> bool:
        """Show the compositor's assigned rewrite key without overstating the requested shortcut."""
        self.approved_rewrite_trigger = trigger
        if self.rewrite_shortcut_status_row is not None:
            self.rewrite_shortcut_status_row.set_subtitle(
                f"{trigger} opens the latest conversation"
                if trigger
                else "Shift+F9 is not approved. Open the latest conversation from the shell menu."
            )
        self._update_capture_status_rows()
        return GLib.SOURCE_REMOVE

    def _global_shortcut_binding_changed(
        self,
        function_key: str,
        trigger_description: str | None,
    ) -> bool:
        """Keep the visible trigger status aligned with the desktop portal."""
        preferred_trigger = self.config.global_recording_key
        if function_key != preferred_trigger:
            return GLib.SOURCE_REMOVE
        self.approved_recording_trigger = trigger_description
        if trigger_description is None and self.global_shortcut_status_row is not None:
            self.global_shortcut_status_row.set_subtitle(
                f"{preferred_trigger} is not currently approved; the on-screen copy-only button remains available"
            )
        elif (
            trigger_description is not None
            and trigger_description.casefold() == preferred_trigger.casefold()
            and self.global_shortcut_status_row is not None
        ):
            self.global_shortcut_status_row.set_subtitle(
                f"Ready — press {trigger_description} once to start and again to stop"
            )
        elif trigger_description is not None and self.global_shortcut_status_row is not None:
            self.global_shortcut_status_row.set_subtitle(
                f"Ready — the desktop assigned {trigger_description}; the saved preference is {preferred_trigger}"
            )
        self._update_capture_status_rows()
        return GLib.SOURCE_REMOVE

    def _update_capture_status(self) -> bool:
        """Expose live elapsed time, provider, and input during capture."""
        if self.capture_started_at is None or self.recorder is None or self.recorder.process is None:
            self.capture_status_timeout_id = None
            return GLib.SOURCE_REMOVE
        elapsed_seconds = int(time.monotonic() - self.capture_started_at)
        minutes, seconds = divmod(elapsed_seconds, 60)
        interaction = "Recording"
        microphone_name = self.pipewire_catalog.display_name(
            PipeWireDeviceKind.MICROPHONE,
            self.config.microphone_target,
        )
        realtime_session = self.realtime_session
        if realtime_session is None or not realtime_session.is_healthy:
            if self.pending_realtime_fallback_reason is None:
                self.pending_realtime_fallback_reason = (
                    RECOGNITION_FALLBACK_STREAM_FAILED
                    if realtime_session is not None
                    else RECOGNITION_FALLBACK_UNAVAILABLE
                )
            provider = f"{self.config.transcription_provider} · final transcription at Stop"
            preview_text = "Realtime preview unavailable; finalized local audio will use batch recognition."
        else:
            provider = (
                "Scribe Realtime"
                if self.config.transcription_provider == "elevenlabs"
                else f"{self.config.transcription_provider} · chunk preview"
            )
            snapshot = realtime_session.snapshot()
            preview_text = snapshot.display_text or "Waiting for speech…"
            self._maybe_live_rewrite(snapshot.display_text)
        self._present_recording_bar(
            self._recording_bar_state(
                kind=RECORDING_KIND_RECORDING,
                detail=f"{interaction} · {provider} · {microphone_name}",
                preview=preview_text,
                elapsed=f"{minutes:02d}:{seconds:02d}",
                level=self.recorder.audio_level,
            )
        )
        return GLib.SOURCE_CONTINUE

    def _clear_capture_status_timeout(self) -> None:
        """Stop live capture updates once recording finishes or is cancelled."""
        if self.capture_status_timeout_id is not None:
            GLib.source_remove(self.capture_status_timeout_id)
            self.capture_status_timeout_id = None
        self.capture_started_at = None

    def _command_key_pressed(self, _controller, key: int, _code: int, state: Gdk.ModifierType) -> bool:
        """Reach command search from native editors without taking over other child shortcuts."""
        if key not in (Gdk.KEY_p, Gdk.KEY_P) or not state & Gdk.ModifierType.CONTROL_MASK:
            return False
        if self.window is not None and self.window.get_visible_dialog() is not None:
            return False
        self._show_commands()
        return True

    def _key_pressed(
        self,
        _controller: Gtk.EventControllerKey,
        key_value: int,
        _key_code: int,
        _state: Gdk.ModifierType,
    ) -> bool:
        """Cancel capture only after child menus and dialogs have had their Escape handling."""
        if self.window is not None and self.window.get_visible_dialog() is not None:
            return False
        if key_value != Gdk.KEY_Escape:
            return False
        return self._cancel_capture()

    def _cancel_capture(self) -> bool:
        """Erase active audio and reset capture state without recognition or delivery."""
        self._cancel_live_rewrite()
        if self._meeting_capture_active() and not self.meeting_processing and self.meeting_recorder is not None:
            if self.meeting_capture_started_at is not None:
                self._record_meeting_diagnostic(
                    DiagnosticStage.CAPTURE,
                    DiagnosticProvider.PIPEWIRE,
                    DiagnosticOutcome.CANCELLED,
                    time.monotonic() - self.meeting_capture_started_at,
                )
            self.meeting_recorder.cancel()
            self._clear_meeting_state()
            self._set_meeting_status("Meeting cancelled. Microphone and system-audio files were erased without upload.")
            return True
        if self.capture_preparing:
            self.capture_preparing = False
            self.capture_stop_requested = False
            self.audio_path = None
            self.pending_command_target = None
            self.pending_session_identifier = None
            self.pending_application_identifier = None
            self.pending_style_identifier = None
            self.pending_style = None
            self.pending_use_saved_style = False
            self.pending_codex_model_identifier = None
            self.pending_transcript_preparation = None
            self.pending_realtime_fallback_reason = None
            self._clear_live_capture()
            self._reset_record_button()
            self._set_status("Recognition preparation cancelled. No microphone audio was recorded or sent.")
            return True
        if self.recorder is None or self.recorder.process is None or self.capture_processing:
            return False
        if self.capture_started_at is not None:
            self._record_app_diagnostic(
                DiagnosticStage.CAPTURE,
                DiagnosticOutcome.CANCELLED,
                time.monotonic() - self.capture_started_at,
            )
        self.recorder.cancel()
        realtime_session = self.realtime_session
        if realtime_session is not None:
            realtime_session.cancel()
        if self.segment_cleanup_session is not None:
            self.segment_cleanup_session.cancel()
        audio_was_streamed = realtime_session is not None and realtime_session.bytes_sent > 0
        self.realtime_session = None
        self.segment_cleanup_session = None
        self.audio_path = None
        self.pending_command_target = None
        self.pending_delivery_target = None
        self.pending_session_identifier = None
        self.pending_application_identifier = None
        self.pending_style_identifier = None
        self.pending_style = None
        self.pending_use_saved_style = False
        self.pending_codex_model_identifier = None
        self.pending_transcript_preparation = None
        self.pending_realtime_fallback_reason = None
        self._clear_live_capture()
        self._reset_record_button()
        self._set_status(
            "Recording cancelled. Local audio was erased; already processed audio cannot be recalled."
            if audio_was_streamed
            else "Recording cancelled. Local audio was erased."
        )
        return True

    def _on_shutdown(self, _application: Adw.Application) -> None:
        """Release child processes and portal registrations during application exit."""
        self.shutting_down = True
        self._cancel_live_rewrite()
        self._cancel_titles(wait=True)
        self.theme_controller.close()
        MluvaApplication._clear_overlay_timeout(self)
        if self.rewrite_client is not None:
            self.rewrite_client.cancel()
            self.rewrite_client = None
        if self.model_catalog_client is not None:
            self.model_catalog_client.cancel()
            self.model_catalog_client = None
        self._clear_meeting_capture_status_timeout()
        if self.recorder is not None and self.recorder.process is not None and self.capture_started_at is not None:
            self._record_app_diagnostic(
                DiagnosticStage.CAPTURE,
                DiagnosticOutcome.CANCELLED,
                time.monotonic() - self.capture_started_at,
            )
        self._clear_capture_status_timeout()
        self.pending_command_target = None
        self.pending_delivery_target = None
        self.history_delivery_targets.clear()
        self.pending_session_identifier = None
        self.pending_application_identifier = None
        self.pending_style_identifier = None
        self.pending_style = None
        self.pending_use_saved_style = False
        self.pending_codex_model_identifier = None
        self.pending_transcript_preparation = None
        self.pending_realtime_fallback_reason = None
        self.capture_preparing = False
        self.capture_stop_requested = False
        if self.recorder is not None and self.recorder.process is not None:
            self.recorder.cancel()
        if self.realtime_session is not None:
            self.realtime_session.cancel()
            self.realtime_session = None
        if self.segment_cleanup_session is not None:
            self.segment_cleanup_session.cancel()
            self.segment_cleanup_session = None
        self._clear_live_capture()
        self.recording_overlay_publisher = None
        if self.meeting_recorder is not None and self._meeting_capture_active():
            self.meeting_recorder.cancel()
        if self.shortcut_service is not None:
            self.shortcut_service.close()
        self.approved_recording_trigger = None
        if self.focus_tracker is not None:
            self.focus_tracker.close()
            self.focus_tracker = None
        if self.workflow is not None:
            self.workflow.codex.close()

    def _initialize_recording_overlay(self) -> None:
        """Attach the optional Shell projection to this application's owned session bus."""
        if self.recording_overlay_publisher is not None:
            return
        connection = self.get_dbus_connection()
        if connection is None:
            return
        publisher = RecordingOverlayPublisher(connection)
        if publisher.clear():
            self.recording_overlay_publisher = publisher

    def _set_initialization_error(self, message: str) -> None:
        """Expose a blocking startup failure with a bounded retry path."""
        self.capture_initialization_failed = True
        self._set_error(message)
        self._set_status("Set up the missing capture dependency, then retry from the callout.")
        if self.record_button is not None:
            self.record_button.set_sensitive(False)
        if self.meeting_page is not None:
            self.meeting_page.record_button.set_sensitive(False)
        self._update_capture_status_rows()

    def _set_error(self, message: str) -> None:
        """Expose one authoritative compact callout without echoing into page status."""
        if self.status_label is not None:
            self.status_label.remove_css_class("error")
        if self.setup_callout is not None:
            if self.setup_callout_title is not None:
                self.setup_callout_title.set_label(
                    "Setup required" if self.capture_initialization_failed else "Capture problem"
                )
            if self.setup_callout_body is not None:
                self.setup_callout_body.set_label(_callout_summary(message))
                self.setup_callout_body.set_tooltip_text(message)
            if self.setup_callout_button is not None:
                self.setup_callout_button.set_label(
                    "Retry initialization" if self.capture_initialization_failed else "Dismiss"
                )
            self.setup_callout.set_reveal_child(True)
        if self.window is not None:
            self.window.announce(message, Gtk.AccessibleAnnouncementPriority.HIGH)

    def _callout_action_clicked(self, _button: Gtk.Button) -> None:
        """Retry only startup failures; dismiss ordinary runtime errors."""
        if self.capture_initialization_failed:
            self._retry_capture_services()
        elif self.setup_callout is not None:
            self.setup_callout.set_reveal_child(False)

    def _retry_capture_services(self) -> None:
        """Rebuild capture boundaries after the user fixes a startup dependency."""
        if self.shortcut_service is not None:
            self.shortcut_service.close()
            self.shortcut_service = None
        self.approved_recording_trigger = None
        if self.workflow is not None:
            self.workflow.codex.close()
            self.workflow = None
        try:
            self._initialize_capture_services()
        except Exception as error:
            self._set_initialization_error(str(error))
            return
        self.capture_initialization_failed = False
        if self.setup_callout is not None:
            self.setup_callout.set_reveal_child(False)
        self._reset_record_button()
        if self.meeting_page is not None:
            self.meeting_page.set_capture_state(recording=False)
        self._set_status("Ready")
        self._show_toast("Capture services are ready.")


def main() -> int:
    """Run the GTK application under the distro Python selected by the launcher."""
    application = MluvaApplication()
    return application.run(sys.argv)


if __name__ == "__main__":
    raise SystemExit(main())
