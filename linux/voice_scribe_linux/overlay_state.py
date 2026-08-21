"""Bounded display-only recording state for the GNOME Shell projection."""

import math
from dataclasses import dataclass
from typing import Protocol

import gi

gi.require_version("Gio", "2.0")
from gi.repository import GLib  # noqa: E402

OVERLAY_OBJECT_PATH = "/com/voicescribe/Linux/RecordingStatus"
OVERLAY_INTERFACE = "com.voicescribe.Linux.RecordingStatus"
OVERLAY_SIGNAL = "StateChanged"
OVERLAY_SIGNAL_SIGNATURE = "(bssussdss)"
VISIBLE_PHASES = frozenset({"preparing", "recording"})


class SignalConnection(Protocol):
    """Small Gio.DBusConnection boundary used by the state publisher."""

    def emit_signal(
        self,
        destination_bus_name: str | None,
        object_path: str,
        interface_name: str,
        signal_name: str,
        parameters: GLib.Variant,
    ) -> None:
        """Emit one D-Bus signal."""


@dataclass(frozen=True, slots=True)
class RecordingOverlayState:
    """One immutable, content-bounded snapshot of the transient recording bar."""

    phase: str
    detail: str = ""
    elapsed_seconds: int = 0
    mode: str = ""
    route: str = ""
    level: float = 0.0
    preview: str = ""
    delivery: str = ""

    @classmethod
    def hidden(cls) -> "RecordingOverlayState":
        """Return the fully erased terminal projection."""
        return cls(phase="hidden")

    def as_signal_values(self) -> tuple[bool, str, str, int, str, str, float, str, str]:
        """Clamp untrusted display strings and numerics to the public signal contract."""
        phase = self.phase.casefold() if self.phase.casefold() in VISIBLE_PHASES else "hidden"
        visible = phase in VISIBLE_PHASES
        if not visible:
            return (False, "hidden", "", 0, "", "", 0.0, "", "")
        level = self.level if math.isfinite(self.level) else 0.0
        return (
            True,
            phase,
            _one_line(self.detail, 80),
            max(0, min(int(self.elapsed_seconds), 86_400)),
            _one_line(self.mode, 32),
            _one_line(self.route, 48),
            max(0.0, min(level, 1.0)),
            _one_line(self.preview, 180),
            _one_line(self.delivery, 48),
        )


class RecordingOverlayPublisher:
    """Publish bounded state on the application's existing session-bus connection."""

    def __init__(self, connection: SignalConnection) -> None:
        """Retain the application-owned connection without owning another bus name."""
        self.connection = connection

    def publish(self, state: RecordingOverlayState) -> bool:
        """Broadcast one optional display snapshot without risking the capture path."""
        try:
            self.connection.emit_signal(
                None,
                OVERLAY_OBJECT_PATH,
                OVERLAY_INTERFACE,
                OVERLAY_SIGNAL,
                GLib.Variant(OVERLAY_SIGNAL_SIGNATURE, state.as_signal_values()),
            )
        except GLib.Error:
            return False
        return True

    def clear(self) -> bool:
        """Erase the Shell projection immediately at every terminal capture state."""
        return self.publish(RecordingOverlayState.hidden())


def _one_line(value: str, limit: int) -> str:
    """Return bounded display text without retaining multiline transcript structure."""
    return " ".join(str(value).split())[:limit]
