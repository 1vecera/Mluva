"""Contract tests for the display-only GNOME recording projection."""

import math
from pathlib import Path

from gi.repository import GLib

from voice_scribe_linux.overlay_state import (
    OVERLAY_INTERFACE,
    OVERLAY_OBJECT_PATH,
    OVERLAY_SIGNAL,
    RecordingOverlayPublisher,
    RecordingOverlayState,
)

LINUX_ROOT = Path(__file__).resolve().parents[1]
EXTENSION_ROOT = LINUX_ROOT / "gnome-extension" / "recording-status@voicescribe.local"


class FakeConnection:
    """Collect public signal arguments without opening the user's session bus."""

    def __init__(self) -> None:
        """Start with no emissions."""
        self.calls: list[tuple[object, ...]] = []

    def emit_signal(self, *arguments: object) -> None:
        """Retain one emission for assertions."""
        self.calls.append(arguments)


class FailingConnection:
    """Model a session bus that closes while capture remains healthy."""

    def emit_signal(self, *_arguments: object) -> None:
        """Raise the same error family as a closed Gio connection."""
        raise GLib.Error("session bus closed")


def test_overlay_state_is_bounded_one_line_and_numeric_safe() -> None:
    """Keep volatile display data small even when a provider produces malformed values."""
    state = RecordingOverlayState(
        phase="RECORDING",
        detail="Listening\nnow " * 20,
        elapsed_seconds=999_999,
        mode="Dictation " * 20,
        route="ElevenLabs Realtime " * 20,
        level=math.inf,
        preview="private\npreview " * 100,
        delivery="Automatic paste ready " * 20,
    )

    visible, phase, detail, elapsed, mode, route, level, preview, delivery = state.as_signal_values()

    assert visible
    assert phase == "recording"
    assert elapsed == 86_400
    assert level == 0
    assert "\n" not in detail
    assert len(detail) <= 80
    assert len(mode) <= 32
    assert len(route) <= 48
    assert len(preview) <= 180
    assert len(delivery) <= 48


def test_hidden_state_erases_every_display_field() -> None:
    """Never leave transcript or route residue after a terminal state."""
    assert RecordingOverlayState(
        phase="finished",
        detail="must disappear",
        elapsed_seconds=10,
        preview="must disappear",
    ).as_signal_values() == (False, "hidden", "", 0, "", "", 0.0, "", "")


def test_optional_publisher_failure_does_not_escape_into_capture() -> None:
    """A dead Shell/session-bus projection must never break recording."""
    publisher = RecordingOverlayPublisher(FailingConnection())

    assert not publisher.publish(RecordingOverlayState(phase="recording"))
    assert not publisher.clear()


def test_publisher_emits_only_the_display_signal_and_clear() -> None:
    """Expose no inbound D-Bus method or command surface."""
    connection = FakeConnection()
    publisher = RecordingOverlayPublisher(connection)

    assert publisher.publish(RecordingOverlayState(phase="preparing", detail="Opening microphone"))
    assert publisher.clear()

    assert len(connection.calls) == 2
    for destination, path, interface, signal, _parameters in connection.calls:
        assert destination is None
        assert path == OVERLAY_OBJECT_PATH
        assert interface == OVERLAY_INTERFACE
        assert signal == OVERLAY_SIGNAL
    assert connection.calls[0][-1].unpack()[0:3] == (True, "preparing", "Opening microphone")
    assert connection.calls[1][-1].unpack() == (False, "hidden", "", 0, "", "", 0.0, "", "")


def test_extension_is_noninteractive_and_clears_on_owner_loss() -> None:
    """Keep the Shell package display-only and lifecycle-complete."""
    source = (EXTENSION_ROOT / "recordingOverlay.js").read_text()

    assert "reactive: false" in source
    assert "can_focus: false" in source
    assert "affectsStruts: false" in source
    assert "Gio.bus_watch_name" in source
    assert "this._hide.bind(this)" in source
    assert "signal_subscribe" in source
    assert "signal_unsubscribe" in source
    assert "spawn" not in source.casefold()
    assert "method" not in source.casefold()
