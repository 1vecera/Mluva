"""Headless readiness coverage for the GTK application's capture handoff."""

import time
from pathlib import Path
from types import SimpleNamespace

import pytest

import voice_scribe_linux.app as app_module
from voice_scribe_linux.app import (
    CAPTURE_MODE_FEATURE_IDS,
    CAPTURE_MODE_IDS,
    CAPTURE_MODE_LABELS,
    CAPTURE_MODE_TOOLTIP,
    GLOBAL_RECORDING_KEY_GUIDANCE,
    GLOBAL_RECORDING_KEY_TOOLTIP,
    MluvaApplication,
    capture_mode_description,
)
from voice_scribe_linux.codex_client import CodexAppServerClient
from voice_scribe_linux.config import FUNCTION_KEY_OPTIONS, AppConfig, load_config
from voice_scribe_linux.delivery import DeliveryReceipt
from voice_scribe_linux.history import HistoryStore
from voice_scribe_linux.realtime import RealtimeCommittedSegment
from voice_scribe_linux.segment_cleanup import SegmentCleanupSession, SegmentCleanupState
from voice_scribe_linux.workflow import TranscriptPreparationSnapshot


class ImmediateTransformer:
    """Return the bounded dictated payload through one fake isolated Codex child."""

    def __init__(self, events: list[str]) -> None:
        """Share the capture-order event ledger with the parent client."""
        self.events = events

    def transform(self, prompt: str, cwd: Path, model: str | None = None) -> str:
        """Validate the frozen model and return the prompt's dictated segment."""
        self.events.append("transform")
        assert cwd.name == "codex-workspace"
        assert model == "gpt-5.4-test"
        return prompt.rsplit("DICTATION:\n", maxsplit=1)[1]

    def cancel(self) -> None:
        """Record release of the isolated fake child."""
        self.events.append("close-child")


class ReadinessCodexClient(CodexAppServerClient):
    """Resolve one model and spawn fake children without launching a subprocess."""

    def __init__(self, events: list[str], failure: Exception | None = None) -> None:
        """Configure deterministic model resolution success or failure."""
        super().__init__(command=("must-not-start",))
        self.events = events
        self.failure = failure

    def resolve_model(self, requested_model: str | None) -> str:
        """Record that model identity is resolved before realtime readiness."""
        self.events.append("resolve-model")
        assert requested_model is None
        if self.failure is not None:
            raise self.failure
        return "gpt-5.4-test"

    def spawn(self) -> ImmediateTransformer:
        """Return an isolated fake transformer for one segment attempt."""
        self.events.append("spawn-child")
        return ImmediateTransformer(self.events)


class CapturingRealtimeClient:
    """Capture the committed-segment callback without opening a WebSocket."""

    def __init__(self, events: list[str]) -> None:
        """Create callback storage beside the order ledger."""
        self.events = events
        self.on_committed_segment = None
        self.session = SimpleNamespace(cancel=lambda: None)

    def start(self, language_code: str, on_committed_segment: object = None) -> object:
        """Record provider readiness and retain the callback for a synthetic commit."""
        self.events.append("realtime-ready")
        assert language_code == "eng"
        self.on_committed_segment = on_committed_segment
        return self.session


class RestorableHistoryTarget:
    """Expose an exact session-only target for a headless explicit History paste."""

    def __init__(self, restores: bool) -> None:
        """Configure whether the retained accessibility object is still usable."""
        self.restores = restores
        self.restore_calls = 0
        self.confirmed_text: list[str] = []
        self.inserted_text: list[str] = []

    def restore(self) -> bool:
        """Record the required restoration before any synthetic paste boundary."""
        self.restore_calls += 1
        return self.restores

    def confirm_insertion(self, inserted_text: str) -> bool:
        """Confirm only by the already-known delivered text supplied to the target object."""
        self.confirmed_text.append(inserted_text)
        return True

    def insert_text(self, inserted_text: str) -> bool:
        """Represent a confirmed native insertion into the retained target."""
        self.inserted_text.append(inserted_text)
        return True


class ToggleSpy:
    """Expose the small GTK toggle contract used by headless state tests."""

    def __init__(self, active: bool) -> None:
        """Record active, sensitive, and explanatory state without GTK."""
        self.active = active
        self.sensitive = True
        self.subtitle = ""

    def get_active(self) -> bool:
        """Return the synthetic toggle state."""
        return self.active

    def set_active(self, active: bool) -> None:
        """Apply one synthetic toggle state."""
        self.active = active

    def set_sensitive(self, sensitive: bool) -> None:
        """Record whether the control can currently be changed."""
        self.sensitive = sensitive

    def set_subtitle(self, subtitle: str) -> None:
        """Record the current consequence copy."""
        self.subtitle = subtitle


class LabelSpy:
    """Capture label and visibility projections without constructing GTK widgets."""

    def __init__(self) -> None:
        """Start with no projected text and a visible surface."""
        self.label = ""
        self.visible = True

    def set_label(self, label: str) -> None:
        """Record the projected label."""
        self.label = label

    def set_visible(self, visible: bool) -> None:
        """Record the projected visibility."""
        self.visible = visible


class SummaryRowSpy:
    """Capture the title and subtitle projected into one compact summary row."""

    def __init__(self) -> None:
        """Start with no projected delivery state."""
        self.title = ""
        self.subtitle = ""

    def set_title(self, title: str) -> None:
        """Record the projected title."""
        self.title = title

    def set_subtitle(self, subtitle: str) -> None:
        """Record the projected explanation."""
        self.subtitle = subtitle


def _preparation() -> TranscriptPreparationSnapshot:
    """Return one immutable no-personalization segment processor."""
    return TranscriptPreparationSnapshot(
        mode="dictation",
        spoken_commands_enabled=False,
        dictionary_replacements=(),
        snippets=(),
        variables=(),
        protected_vocabulary=(),
    )


def _wait_until_cleaned(session: SegmentCleanupSession, timeout: float = 2) -> bool:
    """Wait for one synthetic committed segment to reach ordered publication."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if tuple(item.state for item in session.projection()) == (SegmentCleanupState.CLEANED,):
            return True
        time.sleep(0.001)
    return False


def test_prepare_capture_resolves_model_before_realtime_and_wires_segment_cleanup(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Build the headless cleanup route before the GTK callback can start PipeWire."""
    events: list[str] = []
    scheduled: list[tuple[object, tuple[object, ...]]] = []
    realtime = CapturingRealtimeClient(events)
    prepared_callback = object()
    failed_callback = object()
    application = SimpleNamespace(
        workflow=SimpleNamespace(codex=ReadinessCodexClient(events)),
        shutting_down=False,
        pending_session_identifier="capture-session",
        realtime_client=realtime,
        codex_workspace=tmp_path / "codex-workspace",
        _capture_prepared=prepared_callback,
        _capture_preparation_failed=failed_callback,
    )
    application.codex_workspace.mkdir()

    def capture_idle_add(callback: object, *args: object) -> int:
        """Capture the main-loop handoff without executing GTK or PipeWire work."""
        scheduled.append((callback, args))
        return 1

    monkeypatch.setattr(app_module.GLib, "idle_add", capture_idle_add)
    MluvaApplication._prepare_capture(
        application,
        "capture-session",
        tmp_path / "capture.wav",
        "eng",
        True,
        None,
        "dictation",
        True,
        _preparation(),
    )

    assert events == ["resolve-model", "realtime-ready"]
    assert len(scheduled) == 1
    callback, args = scheduled[0]
    assert callback is prepared_callback
    assert args[4] == "gpt-5.4-test"
    cleanup = args[3]
    assert isinstance(cleanup, SegmentCleanupSession)
    assert realtime.on_committed_segment is not None
    realtime.on_committed_segment(RealtimeCommittedSegment("segment-0", 0, "Raw segment"))
    assert _wait_until_cleaned(cleanup)
    terminal = cleanup.stop_and_drain()
    assert terminal.raw_text == "Raw segment"
    assert terminal.selected_text == "Raw segment"
    assert events[:3] == ["resolve-model", "realtime-ready", "spawn-child"]


def test_prepare_capture_model_failure_never_opens_realtime_route(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Schedule a controlled pre-microphone failure when model resolution fails."""
    events: list[str] = []
    scheduled: list[tuple[object, tuple[object, ...]]] = []
    failed_callback = object()
    application = SimpleNamespace(
        workflow=SimpleNamespace(
            codex=ReadinessCodexClient(events, failure=RuntimeError("unavailable model")),
        ),
        shutting_down=False,
        pending_session_identifier="capture-session",
        realtime_client=CapturingRealtimeClient(events),
        codex_workspace=tmp_path / "codex-workspace",
        _capture_prepared=object(),
        _capture_preparation_failed=failed_callback,
    )

    def capture_idle_add(callback: object, *args: object) -> int:
        """Capture only the controlled readiness failure callback."""
        scheduled.append((callback, args))
        return 1

    monkeypatch.setattr(app_module.GLib, "idle_add", capture_idle_add)
    MluvaApplication._prepare_capture(
        application,
        "capture-session",
        tmp_path / "capture.wav",
        "eng",
        True,
        None,
        "dictation",
        True,
        _preparation(),
    )

    assert events == ["resolve-model"]
    assert len(scheduled) == 1
    callback, args = scheduled[0]
    assert callback is failed_callback
    assert args[0] == "capture-session"
    assert args[2] == "unavailable model"


def test_capture_mode_contract_keeps_full_labels_and_pre_recording_explanations() -> None:
    """Expose Dictate, Command, Notes, and Meeting intent before microphone capture."""
    assert CAPTURE_MODE_IDS == ("dictation", "command", "scratchpad")
    assert CAPTURE_MODE_LABELS == ("Dictate", "Command", "Notes")
    assert CAPTURE_MODE_FEATURE_IDS == ("dictation", "command_mode", "notes_mode")
    assert "configured global function key" in capture_mode_description(0)
    assert capture_mode_description(1).startswith("Experimental —")
    assert "explicitly selected text" in capture_mode_description(1)
    assert capture_mode_description(2).startswith("Experimental —")
    assert "editable draft" in capture_mode_description(2)
    assert all(label in CAPTURE_MODE_TOOLTIP for label in ("Dictate", "Command", "Notes", "Meeting"))
    assert CAPTURE_MODE_TOOLTIP.count("Experimental") == 3


def test_function_key_guidance_names_default_and_low_conflict_range() -> None:
    """Explain the practical F9 default without hiding lower-conflict extended keys."""
    assert "F9 is the practical default" in GLOBAL_RECORDING_KEY_GUIDANCE
    assert "F13–F24" in GLOBAL_RECORDING_KEY_GUIDANCE
    assert "fewest conflicts" in GLOBAL_RECORDING_KEY_GUIDANCE
    assert "F1–F12" in GLOBAL_RECORDING_KEY_TOOLTIP
    assert "absent from most standard keyboards" in GLOBAL_RECORDING_KEY_TOOLTIP


def test_capture_summary_yields_to_live_and_pending_review_states() -> None:
    """Keep the active state and required decision above the fold on a small window."""
    title = LabelSpy()
    summary = LabelSpy()
    application = SimpleNamespace(
        recorder=SimpleNamespace(process=None),
        capture_preparing=False,
        capture_processing=False,
        pending_command_result=object(),
        scratchpad_store=SimpleNamespace(draft=None),
        capture_initialization_failed=False,
        capture_status_title=title,
        capture_summary_box=summary,
        capture_mode_status_row=None,
        capture_delivery_status_row=None,
        capture_privacy_status_row=None,
        capture_action_hint=None,
        mode=None,
        output_style=None,
        config=AppConfig(),
        focus_tracker=None,
        shortcut_service=None,
        approved_recording_trigger=None,
        capture_allows_auto_paste=False,
    )

    MluvaApplication._update_capture_status_rows(application)

    assert title.label == "Review required"
    assert not summary.visible

    application.pending_command_result = None
    application.capture_preparing = True
    MluvaApplication._update_capture_status_rows(application)

    assert title.label == "Preparing capture"
    assert not summary.visible

    application.capture_preparing = False
    application.recorder.process = object()
    MluvaApplication._update_capture_status_rows(application)

    assert title.label == "Recording"
    assert not summary.visible

    application.recorder.process = None
    MluvaApplication._update_capture_status_rows(application)

    assert title.label == "Ready to capture"
    assert summary.visible


def test_capture_delivery_requires_portal_approval_and_reports_the_actual_trigger() -> None:
    """Claim automatic paste only after the portal reports its approved trigger."""
    delivery = SummaryRowSpy()
    action_hint = LabelSpy()
    application = SimpleNamespace(
        recorder=SimpleNamespace(process=None),
        capture_preparing=False,
        capture_processing=False,
        pending_command_result=None,
        scratchpad_store=SimpleNamespace(draft=None),
        capture_initialization_failed=False,
        capture_status_title=None,
        capture_summary_box=None,
        capture_mode_status_row=None,
        capture_delivery_status_row=delivery,
        capture_privacy_status_row=None,
        capture_action_hint=action_hint,
        mode=None,
        output_style=None,
        config=AppConfig(global_recording_key="F9", auto_paste=True),
        focus_tracker=object(),
        shortcut_service=None,
        approved_recording_trigger=None,
        capture_allows_auto_paste=False,
    )

    MluvaApplication._update_capture_status_rows(application)

    assert delivery.title == "Copy-only delivery"
    assert delivery.subtitle == "The global shortcut service is disabled or unavailable"
    assert action_hint.label == "Use the on-screen button for copy-only capture"

    application.shortcut_service = object()
    MluvaApplication._update_capture_status_rows(application)

    assert delivery.subtitle == "F9 is waiting for desktop shortcut approval"
    assert action_hint.label == "F9 awaits desktop approval · this button always copies"

    application.approved_recording_trigger = "F10"
    MluvaApplication._update_capture_status_rows(application)

    assert delivery.title == "Global paste · button copy"
    assert delivery.subtitle == (
        "F10 inserts when an accessible text target is captured; the on-screen button always copies"
    )
    assert action_hint.label == (
        "F10 toggles global capture and can insert into a captured text field · this button always copies"
    )

    application.recorder.process = object()
    application.capture_allows_auto_paste = True
    MluvaApplication._update_capture_status_rows(application)

    assert delivery.title == "Copy-only capture"
    assert delivery.subtitle == "No accessible focused text field was captured"

    application.pending_delivery_target = SimpleNamespace(editable_text=object())
    MluvaApplication._update_capture_status_rows(application)

    assert delivery.title == "Automatic paste armed"


def test_incognito_temporarily_suspends_and_then_restores_cleanup() -> None:
    """Do not erase the user's cleanup preference when Incognito requires it off."""
    incognito = ToggleSpy(active=True)
    cleanup = ToggleSpy(active=True)
    output_style = ToggleSpy(active=False)
    application = SimpleNamespace(
        incognito_switch=incognito,
        cleanup_switch=cleanup,
        cleanup_before_incognito=None,
        recorder=None,
        capture_preparing=False,
        capture_processing=False,
        _meeting_capture_active=lambda: False,
        meeting_processing=False,
        pending_command_result=None,
        scratchpad_store=SimpleNamespace(draft=None),
        output_style=output_style,
    )

    MluvaApplication._apply_incognito_controls(application)

    assert application.cleanup_before_incognito is True
    assert not cleanup.active
    assert not cleanup.sensitive
    assert "Unavailable in Incognito" in cleanup.subtitle
    assert not output_style.sensitive

    incognito.active = False
    MluvaApplication._apply_incognito_controls(application)

    assert application.cleanup_before_incognito is None
    assert cleanup.active
    assert cleanup.sensitive
    assert "local Codex app-server" in cleanup.subtitle
    assert output_style.sensitive


def test_function_key_selection_persists_and_rebinds_portal_service(tmp_path: Path) -> None:
    """Apply any selected F1–F24 key to config and the live portal service."""
    selected_keys: list[str] = []
    statuses: list[str] = []
    binding_subtitles: list[str] = []
    capture_status_updates: list[str] = []
    workflow = SimpleNamespace(config=None)
    meeting_workflow = SimpleNamespace(config=None)
    application = SimpleNamespace(
        global_recording_key=SimpleNamespace(
            get_selected=lambda: FUNCTION_KEY_OPTIONS.index("F24"),
            set_selected=lambda _index: None,
        ),
        config=AppConfig(),
        config_path=tmp_path / "config.json",
        recorder=None,
        capture_preparing=False,
        capture_processing=False,
        meeting_processing=False,
        meeting_retry_in_progress=False,
        _meeting_capture_active=lambda: False,
        workflow=workflow,
        meeting_workflow=meeting_workflow,
        global_shortcut_status_row=SimpleNamespace(set_subtitle=binding_subtitles.append),
        approved_recording_trigger="F9",
        shortcut_service=SimpleNamespace(set_recording_key=selected_keys.append),
        _update_capture_status_rows=lambda: capture_status_updates.append("updated"),
        _set_status=statuses.append,
    )

    MluvaApplication._global_recording_key_changed(application)

    assert application.config.global_recording_key == "F24"
    assert load_config(application.config_path).global_recording_key == "F24"
    assert workflow.config is application.config
    assert meeting_workflow.config is application.config
    assert selected_keys == ["F24"]
    assert application.approved_recording_trigger is None
    assert binding_subtitles == ["Requesting F24 through the desktop portal; approve the replacement if prompted…"]
    assert capture_status_updates == ["updated"]
    assert statuses == ["F24 saved; the desktop portal is replacing the global binding."]


def test_global_function_key_starts_an_auto_paste_eligible_capture() -> None:
    """Make a portal activation the sole automatic-paste recording origin."""
    events: list[str] = []
    application = SimpleNamespace(
        recorder=SimpleNamespace(process=None),
        record_button=object(),
        meeting_processing=False,
        meeting_retry_in_progress=False,
        _meeting_capture_active=lambda: False,
        capture_processing=False,
        capture_preparing=False,
        config=AppConfig(auto_paste=True),
        capture_allows_auto_paste=False,
        _start_capture=lambda: events.append("start"),
        _stop_capture=lambda: events.append("stop"),
        _cancel_capture=lambda: events.append("cancel"),
        _set_status=events.append,
    )

    MluvaApplication._shortcut_toggled(application)

    assert application.capture_allows_auto_paste
    assert events == ["start"]


def test_global_function_key_stops_an_active_capture() -> None:
    """Use the next F-key activation as a one-shot stop action."""
    events: list[str] = []
    application = SimpleNamespace(
        recorder=SimpleNamespace(process=object()),
        record_button=object(),
        meeting_processing=False,
        meeting_retry_in_progress=False,
        _meeting_capture_active=lambda: False,
        capture_processing=False,
        capture_preparing=False,
        config=AppConfig(),
        capture_allows_auto_paste=True,
        _start_capture=lambda: events.append("start"),
        _stop_capture=lambda: events.append("stop"),
        _cancel_capture=lambda: events.append("cancel"),
        _set_status=events.append,
    )

    MluvaApplication._shortcut_toggled(application)

    assert events == ["stop"]


def test_global_shortcut_status_reports_actual_binding_and_ignores_stale_session() -> None:
    """Never present an old portal session or unapproved preference as ready."""
    subtitles: list[str] = []
    capture_statuses: list[str | None] = []
    status_row = SimpleNamespace(set_subtitle=subtitles.append)
    application = SimpleNamespace(
        config=AppConfig(global_recording_key="F9"),
        global_shortcut_status_row=status_row,
        approved_recording_trigger=None,
        _update_capture_status_rows=lambda: capture_statuses.append(application.approved_recording_trigger),
    )

    MluvaApplication._global_shortcut_binding_changed(application, "F8", "F8")
    MluvaApplication._global_shortcut_binding_changed(application, "F9", "F10")
    MluvaApplication._global_shortcut_binding_changed(application, "F9", None)
    MluvaApplication._global_shortcut_binding_changed(application, "F9", "F9")

    assert subtitles == [
        "Ready — the desktop assigned F10; the saved preference is F9",
        "F9 is not currently approved; the on-screen copy-only button remains available",
        "Ready — press F9 once to start and again to stop",
    ]
    assert capture_statuses == ["F10", None, "F9"]
    assert application.approved_recording_trigger == "F9"


def test_global_shortcut_binding_updates_capture_state_without_settings_row() -> None:
    """Keep the primary capture card truthful when no settings status row exists."""
    capture_statuses: list[str | None] = []
    application = SimpleNamespace(
        config=AppConfig(global_recording_key="F9"),
        global_shortcut_status_row=None,
        approved_recording_trigger=None,
        _update_capture_status_rows=lambda: capture_statuses.append(application.approved_recording_trigger),
    )

    MluvaApplication._global_shortcut_binding_changed(application, "F9", "F10")

    assert application.approved_recording_trigger == "F10"
    assert capture_statuses == ["F10"]


def test_history_retry_pastes_once_only_after_exact_cached_target_restoration(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Reuse a session-only target for explicit paste and persist its confirmed receipt."""
    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    entry = history.add("raw", "Saved result.", "dictation", "eng", None, "copied")
    target = RestorableHistoryTarget(restores=True)
    calls: list[tuple[str, bool]] = []

    def fake_delivery(
        text: str,
        auto_paste: bool,
        confirm_paste: object = None,
        insert_directly: object = None,
        authorize_keyboard_paste: object = None,
    ) -> DeliveryReceipt:
        """Require restoration to precede one content-free paste confirmation."""
        calls.append((text, auto_paste))
        assert callable(insert_directly)
        assert insert_directly(text)
        assert callable(authorize_keyboard_paste)
        assert callable(confirm_paste)
        assert confirm_paste()
        return DeliveryReceipt(
            copied=True,
            pasted=True,
            guidance="Pasted in test.",
            paste_dispatched=True,
            paste_confirmed=True,
        )

    monkeypatch.setattr(app_module, "deliver_text", fake_delivery)
    messages: list[str] = []
    application = SimpleNamespace(
        history_delivery_targets={entry.identifier: target},
        config=AppConfig(auto_paste=True),
        history_store=history,
        history_page=None,
        _set_status=messages.append,
    )

    MluvaApplication._retry_history_delivery(application, entry)

    assert target.restore_calls == 1
    assert target.inserted_text == ["Saved result."]
    assert target.confirmed_text == ["Saved result."]
    assert calls == [("Saved result.", True)]
    assert history.find(entry.identifier).delivery_outcome == "pasted"
    assert messages == ["Pasted in test."]


def test_history_retry_copies_without_dispatch_when_cached_target_is_stale(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Drop a stale target and leave the complete saved text on the clipboard for recovery."""
    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    entry = history.add("raw", "Saved result.", "dictation", "eng", None, "delivery-failed")
    target = RestorableHistoryTarget(restores=False)

    def fake_delivery(text: str, auto_paste: bool) -> DeliveryReceipt:
        """Represent a copy-only recovery without invoking an input injector."""
        assert text == "Saved result."
        assert not auto_paste
        return DeliveryReceipt(copied=True, pasted=False, guidance="Copied in test.")

    monkeypatch.setattr(app_module, "deliver_text", fake_delivery)
    messages: list[str] = []
    application = SimpleNamespace(
        history_delivery_targets={entry.identifier: target},
        config=AppConfig(auto_paste=True),
        history_store=history,
        history_page=None,
        _set_status=messages.append,
    )

    MluvaApplication._retry_history_delivery(application, entry)

    assert target.restore_calls == 1
    assert application.history_delivery_targets == {}
    assert history.find(entry.identifier).delivery_outcome == "copied"
    assert messages == ["Copied in test. The retained target was stale, so no paste was attempted."]
