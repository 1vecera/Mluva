"""Headless coverage for the transient recording bar projection."""

from types import SimpleNamespace

from voice_scribe_linux.app import MluvaApplication, _elapsed_seconds, _overlay_detail_and_route
from voice_scribe_linux.config import AppConfig
from voice_scribe_linux.overlay_state import RecordingOverlayPublisher, RecordingOverlayState
from voice_scribe_linux.ui import (
    RECORDING_KIND_PREPARING,
    RECORDING_KIND_RECORDING,
    RECORDING_KIND_TERMINAL,
    RecordingBarState,
    is_compact_layout,
)


class _ProductionPaths(SimpleNamespace):
    """Bind the real terminal-path methods onto otherwise stubbed state."""

    _clear_capture_status_timeout = MluvaApplication._clear_capture_status_timeout
    _clear_live_capture = MluvaApplication._clear_live_capture
    _hide_recording_bar = MluvaApplication._hide_recording_bar
    _set_status = MluvaApplication._set_status
    # The stop path spawns the real worker; with workflow=None it returns at once.
    _finish_recording = MluvaApplication._finish_recording


def _application_stub(**overrides: object) -> _ProductionPaths:
    """Assemble the minimal state surface the bar projection reads."""

    def noop(*_args: object) -> None:
        """Accept any terminal-path refresh without GTK."""

    application = _ProductionPaths(
        recorder=None,
        mode=None,
        pending_mode="dictation",
        config=AppConfig(),
        focus_tracker=None,
        capture_allows_auto_paste=False,
        recording_bar=None,
        recording_bar_slot=None,
        recording_overlay_publisher=None,
        capture_summary_box=None,
        pending_delivery_target=None,
        pending_command_target=None,
        capture_status_timeout_id=None,
        capture_started_at=None,
        status_label=None,
        _update_capture_status_rows=noop,
    )
    for name, value in overrides.items():
        setattr(application, name, value)
    return application


def test_compact_layout_boundary_matches_navigation_breakpoint() -> None:
    """The bar collapses metadata exactly when navigation collapses."""
    assert is_compact_layout(420)
    assert is_compact_layout(736)
    assert not is_compact_layout(737)
    assert not is_compact_layout(1280)


def test_overlay_projection_parses_elapsed_and_splits_route() -> None:
    """Keep the Shell signal numeric while preserving provider/device context."""
    assert _elapsed_seconds("01:23") == 83
    assert _elapsed_seconds("bad") == 0
    assert _elapsed_seconds("01:99") == 0
    assert _overlay_detail_and_route("Recording · ElevenLabs Realtime · USB Microphone") == (
        "Recording",
        "ElevenLabs Realtime · USB Microphone",
    )


def test_application_initializes_overlay_on_its_existing_bus_connection() -> None:
    """Use the Gtk application's owned bus name instead of creating an inbound service."""
    calls: list[tuple[object, ...]] = []
    connection = SimpleNamespace(emit_signal=lambda *arguments: calls.append(arguments))
    application = SimpleNamespace(
        recording_overlay_publisher=None,
        get_dbus_connection=lambda: connection,
    )

    MluvaApplication._initialize_recording_overlay(application)

    assert isinstance(application.recording_overlay_publisher, RecordingOverlayPublisher)
    assert calls[0][-1].unpack() == (False, "hidden", "", 0, "", "", 0.0, "", "")


def test_preparing_projection_uses_pending_mode_and_copy_only() -> None:
    """Preparing shows the frozen pending decision without claiming live listening."""
    application = _application_stub(pending_mode="command")
    state = MluvaApplication._recording_bar_state(application, kind=RECORDING_KIND_PREPARING, detail="Preparing…")
    assert state.kind == RECORDING_KIND_PREPARING
    assert state.mode == "Command"
    assert state.delivery == "Copy only"
    assert state.preview == "Preparing recognition…"
    assert state.quiet is False


def test_recording_projection_reports_armed_paste_and_speech() -> None:
    """Recording derives mode from the visible selector and honors paste arming."""
    application = _application_stub(
        recorder=SimpleNamespace(process=object(), audio_level=0.4),
        mode=SimpleNamespace(get_selected=lambda: 2),
        config=AppConfig(auto_paste=True),
        focus_tracker=object(),
        capture_allows_auto_paste=True,
        pending_delivery_target=SimpleNamespace(editable_text=object()),
    )
    state = MluvaApplication._recording_bar_state(
        application,
        kind=RECORDING_KIND_RECORDING,
        detail="Recording · Realtime",
        preview="the quarterly review is on …",
        elapsed="01:23",
        level=0.4,
    )
    assert state.kind == RECORDING_KIND_RECORDING
    assert state.mode == "Notes"
    assert state.delivery == "Paste armed"
    assert state.quiet is False


def test_recording_projection_drops_to_copy_only_when_target_not_armed() -> None:
    """An unarmed target never claims paste even with auto-paste enabled."""
    application = _application_stub(
        recorder=SimpleNamespace(process=object(), audio_level=0.1),
        mode=SimpleNamespace(get_selected=lambda: 0),
        config=AppConfig(auto_paste=True),
        focus_tracker=object(),
        capture_allows_auto_paste=False,
    )
    state = MluvaApplication._recording_bar_state(
        application, kind=RECORDING_KIND_RECORDING, preview="Waiting for speech…"
    )
    assert state.delivery == "Copy only"
    assert state.quiet is True


class RecordingBarSpy(SimpleNamespace):
    """Record bar projection calls without constructing GTK widgets."""

    def __init__(self) -> None:
        """Start with no projected states or terminal clears."""
        super().__init__(presented=[], cleared=0, reveal=None)

    def clear(self) -> None:
        """Record one terminal erase."""
        self.cleared += 1


class RecordingSlotSpy(SimpleNamespace):
    """Record slot reveal requests without constructing GTK widgets."""

    def __init__(self) -> None:
        """Start with no reveal decisions."""
        super().__init__(revealed=[])

    def set_reveal_child(self, revealed: bool) -> None:
        """Record one reveal decision."""
        self.revealed.append(revealed)


class RecordingOverlayPublisherSpy(SimpleNamespace):
    """Record optional Shell publications without opening a session bus."""

    def __init__(self) -> None:
        """Start with no snapshots or terminal clears."""
        super().__init__(published=[], cleared=0)

    def publish(self, state: RecordingOverlayState) -> bool:
        """Retain one bounded snapshot and report a healthy optional route."""
        self.published.append(state)
        return True

    def clear(self) -> bool:
        """Record immediate removal of the desktop-wide projection."""
        self.cleared += 1
        return True


def test_terminal_projection_hides_bar_and_slot_immediately() -> None:
    """The terminal path blanks the bar and un-reveals the slot in one step."""
    bar = RecordingBarSpy()
    slot = RecordingSlotSpy()
    overlay = RecordingOverlayPublisherSpy()
    application = _application_stub(
        recording_bar=bar,
        recording_bar_slot=slot,
        recording_overlay_publisher=overlay,
    )
    MluvaApplication._hide_recording_bar(application)
    assert slot.revealed == [False]
    assert bar.cleared == 1
    assert overlay.cleared == 1


def test_present_projection_follows_widget_visibility_contract() -> None:
    """The slot reveal follows the bar's own visibility decision."""
    bar = RecordingBarSpy()
    bar.present = lambda state: True  # type: ignore[method-assign]
    slot = RecordingSlotSpy()
    overlay = RecordingOverlayPublisherSpy()
    application = _application_stub(
        recording_bar=bar,
        recording_bar_slot=slot,
        recording_overlay_publisher=overlay,
    )
    state = RecordingBarState(
        kind=RECORDING_KIND_RECORDING,
        detail="Recording",
        elapsed="00:04",
        mode="Dictate",
        delivery="Copy only",
        level=0.2,
        preview="hello",
        quiet=False,
    )
    MluvaApplication._present_recording_bar(application, state)
    assert slot.revealed == [True]
    assert overlay.published == [
        RecordingOverlayState(
            phase=RECORDING_KIND_RECORDING,
            detail="Recording",
            elapsed_seconds=4,
            mode="Dictate",
            level=0.2,
            preview="hello",
            delivery="Copy only",
        )
    ]


def _button_stub() -> SimpleNamespace:
    """Minimal record-button surface touched by the stop path."""
    return SimpleNamespace(set_sensitive=lambda sensitive: None)


def test_stop_capture_erases_bar_immediately_without_finalizing_surface() -> None:
    """Stop hides the bar and slot at once; recognition continues in the status card."""
    bar = RecordingBarSpy()
    bar.present = lambda state: bar.__setattr__("presented", bar.presented + [state])  # type: ignore[method-assign]
    slot = RecordingSlotSpy()
    application = _application_stub(
        recorder=SimpleNamespace(process=object(), audio_level=0.2, cancel=lambda: None),
        workflow=None,
        record_button=_button_stub(),
        capture_started_at=None,
        capture_status_timeout_id=None,
        realtime_session=None,
        capture_processing=False,
        recording_bar=bar,
        recording_bar_slot=slot,
        status_label=None,
        scratchpad_store=SimpleNamespace(draft=None),
        pending_command_result=None,
    )
    application.capture_allows_auto_paste = True
    MluvaApplication._stop_capture(application)
    assert application.capture_processing is True
    assert slot.revealed[-1] is False
    assert bar.cleared == 1
    for state in bar.presented:
        assert "Finalizing" not in state.detail


def test_clear_live_capture_is_the_shared_terminal_erase_for_all_paths() -> None:
    """Cancel, failure, and shutdown all route through the same immediate erase."""
    bar = RecordingBarSpy()
    slot = RecordingSlotSpy()
    summary = SimpleNamespace(set_visible=lambda visible: None)
    application = _application_stub(
        recording_bar=bar,
        recording_bar_slot=slot,
        capture_summary_box=summary,
    )
    MluvaApplication._clear_live_capture(application)
    assert bar.cleared == 1
    assert slot.revealed == [False]


def test_terminal_state_constant_is_recognized() -> None:
    """The terminal kind stays a distinct explicit state."""
    assert RECORDING_KIND_TERMINAL == "terminal"
    assert RECORDING_KIND_PREPARING == "preparing"
    assert RECORDING_KIND_RECORDING == "recording"
