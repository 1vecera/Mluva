"""Content and owner-boundary tests that never access the live session bus."""

import pytest
from gi.repository import Gio, GLib

from mluva_linux.overlay_state import OVERLAY_SIGNAL_SIGNATURE, SHELL_SIGNAL_SIGNATURE, RecordingOverlayState
from mluva_linux.shell_bridge import StatusWatch, activate, project_state


@pytest.mark.parametrize("phase", ["preparing", "recording", "processing", "copied", "error"])
def test_projection_keeps_only_phase_and_bounded_elapsed(phase: str) -> None:
    """Exclude raw recognition and arbitrary detail from the shell's stdout."""
    state = RecordingOverlayState(phase=phase, elapsed_seconds=12, preview="synthetic private text", detail="secret")
    assert project_state(GLib.Variant(OVERLAY_SIGNAL_SIGNATURE, state.as_signal_values())) == {
        "phase": phase,
        "elapsed": 12,
    }


def test_hidden_and_invalid_signatures_do_not_preserve_stale_recording() -> None:
    """Show idle only for an authoritative hidden snapshot."""
    hidden = GLib.Variant(OVERLAY_SIGNAL_SIGNATURE, RecordingOverlayState.hidden().as_signal_values())
    assert project_state(hidden) == {"phase": "idle", "elapsed": 0}
    assert project_state(GLib.Variant("(s)", ("recording",))) == {"phase": "unavailable", "elapsed": 0}


def test_overlay_preview_is_opt_in_bounded_and_erased_when_hidden() -> None:
    """Expose only the latest visible words to the floating preview, with no idle residue."""
    text = "Earlier words " * 1000 + "Newest words: Žluťoučký kůň"
    parameters = GLib.Variant(
        SHELL_SIGNAL_SIGNATURE, (RecordingOverlayState(phase="recording", level=0.4, preview=text).as_shell_values(),)
    )
    projected = project_state(parameters, overlay=True)
    assert projected["phase"] == "recording" and projected["elapsed"] == 0 and projected["level"] == 0.4
    start = projected["preview_start"]
    assert start > 0 and text[start - 1] == " "
    assert projected["preview"] == text[start:] and len(projected["preview"]) <= 4096
    assert projected["preview"].endswith("Newest words: Žluťoučký kůň")
    assert "preview" not in project_state(parameters)
    hidden = GLib.Variant(SHELL_SIGNAL_SIGNATURE, (RecordingOverlayState.hidden().as_shell_values(),))
    assert project_state(hidden, overlay=True) == {"phase": "idle", "elapsed": 0}


def test_preview_offset_counts_unicode_characters_and_handles_unbroken_tokens() -> None:
    """Preserve a usable origin without losing the newest words or splitting surrogate pairs."""
    for text in ("🙂 café " * 1000 + "newest", "a" * 6000):
        values = RecordingOverlayState(phase="recording", preview=text).as_shell_values()
        projected = project_state(GLib.Variant(SHELL_SIGNAL_SIGNATURE, (values,)), overlay=True)
        assert projected["preview"] == text[projected["preview_start"] :]
        assert 0 < len(projected["preview"]) <= 4096


class FakeConnection:
    """Record calls and subscriptions without a D-Bus connection."""

    def __init__(self) -> None:
        """Initialize fake protocol bookkeeping."""
        self.calls = []
        self.unsubscribed = []

    def call_sync(self, *args: object) -> None:
        """Retain method calls in order."""
        self.calls.append(args)

    def signal_subscribe(self, *args: object) -> int:
        """Retain the owner filter before status replay."""
        self.calls.append(args)
        return 7

    def signal_unsubscribe(self, identifier: int) -> None:
        """Record release of the previous owner."""
        self.unsubscribed.append(identifier)


def test_owner_loss_clears_state_and_ignores_queued_old_signals() -> None:
    """Do not leave a red microphone after the application exits or changes owners."""
    connection = FakeConnection()
    states = []
    watch = StatusWatch(connection, states.append)
    watch._appeared(connection, "com.mluva.Linux", ":1.5")
    assert connection.calls[0][0] == ":1.5"
    assert connection.calls[1][4].unpack()[0] == "status"
    recording = GLib.Variant(OVERLAY_SIGNAL_SIGNATURE, RecordingOverlayState(phase="recording").as_signal_values())
    watch._changed(connection, ":1.99", "", "", "", recording)
    assert states[-1]["phase"] == "unavailable"
    watch._changed(connection, ":1.5", "", "", "", recording)
    assert states[-1]["phase"] == "recording"
    watch._vanished(connection, "com.mluva.Linux")
    watch._changed(connection, ":1.5", "", "", "", recording)
    assert states[-1] == {"phase": "stopped", "elapsed": 0}
    assert connection.unsubscribed == [7]
    watch._appeared(connection, "com.mluva.Linux", ":1.6")
    assert states[-1]["phase"] == "unavailable"
    assert connection.calls[-1][0] == ":1.6"
    watch.close()


def test_control_is_allowlisted_and_never_autostarts() -> None:
    """Keep shell controls deliberate and directed to the resolved unique process."""
    connection = FakeConnection()
    activate(connection, ":1.5", "cancel")
    call = connection.calls[-1]
    assert call[0] == ":1.5"
    assert call[4].unpack() == ("cancel", [], {})
    assert call[6] == Gio.DBusCallFlags.NO_AUTO_START
    assert call[7] == 1500
    with pytest.raises(ValueError):
        activate(connection, ":1.5", "arbitrary-command")
    activate(connection, ":1.5", "review", ("rewrite", "note-id", "polish"))
    assert connection.calls[-1][4].unpack() == ("review", [("rewrite", "note-id", "polish")], {})
    with pytest.raises(ValueError):
        activate(connection, ":1.5", "review", ("arbitrary-command", "note-id", "polish"))


def test_review_projection_carries_option_ids_and_erases_all_review_content() -> None:
    """Offer bounded labels without publishing the saved model instructions to the shell."""
    state = RecordingOverlayState(
        phase="ready",
        preview="Synthetic result",
        review_identifier="n" * 80,
        review_options=tuple(("s" * 80, "Label " * 40) for _ in range(200)),
        message="m" * 200,
    )
    parameters = GLib.Variant(SHELL_SIGNAL_SIGNATURE, (state.as_shell_values(),))
    projected = project_state(parameters, overlay=True)
    assert projected["identifier"] == "n" * 36
    assert len(projected["options"]) == 128
    assert projected["options"][0] == {"value": "s" * 36, "label": ("Label " * 40)[:64]}
    assert projected["message"] == "m" * 96
    assert "preview" not in project_state(parameters)
    hidden = GLib.Variant(SHELL_SIGNAL_SIGNATURE, (RecordingOverlayState.hidden().as_shell_values(),))
    assert project_state(hidden, overlay=True) == {"phase": "idle", "elapsed": 0}
