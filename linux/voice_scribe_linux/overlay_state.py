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
VISIBLE_PHASES = frozenset({"preparing", "recording", "processing", "copied", "error"})
SHELL_SIGNAL = "ShellStateChanged"
SHELL_SIGNAL_SIGNATURE = "(a{sv})"
REVIEW_PHASES = frozenset({"ready", "rewriting", "review-error"})
SHELL_PHASES = frozenset({"preparing", "recording", "processing", "error"}) | REVIEW_PHASES
SHELL_PREVIEW_CHARACTERS = 4096
MAX_REVIEW_OPTIONS = 128


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
    review_identifier: str = ""
    review_options: tuple[tuple[str, str], ...] = ()
    message: str = ""
    review_timeout_seconds: int = 4
    show_copy_action: bool = True
    smooth_scrolling: bool = True
    scroll_duration_ms: int = 800
    scroll_lookahead_lines: int = 2

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
            " ".join(str(self.preview).split())[-180:],
            _one_line(self.delivery, 48),
        )

    def as_shell_values(self) -> dict[str, GLib.Variant]:
        """Expose bounded text and option IDs to the opt-in, interactive Omarchy widget."""
        phase = self.phase if self.phase in SHELL_PHASES else "idle"
        values = {"phase": GLib.Variant("s", phase), "elapsed": GLib.Variant("u", 0)}
        if phase == "idle":
            return values
        preview = " ".join(self.preview.split())
        preview_start = max(0, len(preview) - SHELL_PREVIEW_CHARACTERS)
        if preview_start:
            boundary = preview.find(" ", preview_start - 1)
            if boundary >= 0:
                preview_start = boundary + 1
        values.update(
            review_timeout=GLib.Variant("u", max(1, min(60, self.review_timeout_seconds))),
            show_copy=GLib.Variant("b", self.show_copy_action),
            smooth_scrolling=GLib.Variant("b", self.smooth_scrolling),
            scroll_duration=GLib.Variant("u", max(0, min(2000, self.scroll_duration_ms))),
            scroll_lookahead=GLib.Variant("u", max(0, min(6, self.scroll_lookahead_lines))),
            elapsed=GLib.Variant("u", max(0, min(int(self.elapsed_seconds), 86_400))),
            level=GLib.Variant("d", max(0.0, min(self.level, 1.0)) if math.isfinite(self.level) else 0.0),
            preview=GLib.Variant("s", preview[preview_start:]),
            preview_start=GLib.Variant("u", preview_start),
        )
        if phase in REVIEW_PHASES:
            values.update(
                identifier=GLib.Variant("s", self.review_identifier[:36]),
                options=GLib.Variant(
                    "a(ss)",
                    [
                        (identifier[:36], _one_line(label, 64))
                        for identifier, label in self.review_options[:MAX_REVIEW_OPTIONS]
                    ],
                ),
                message=GLib.Variant("s", _one_line(self.message, 96)),
            )
        return values


class RecordingOverlayPublisher:
    """Publish bounded state on the application's existing session-bus connection."""

    def __init__(self, connection: SignalConnection) -> None:
        """Retain the application-owned connection without owning another bus name."""
        self.connection = connection
        self._parameters = GLib.Variant(OVERLAY_SIGNAL_SIGNATURE, RecordingOverlayState.hidden().as_signal_values())
        self._shell_parameters = GLib.Variant(
            SHELL_SIGNAL_SIGNATURE, (RecordingOverlayState.hidden().as_shell_values(),)
        )

    def publish(self, state: RecordingOverlayState) -> bool:
        """Broadcast one optional display snapshot without risking the capture path."""
        self._parameters = GLib.Variant(OVERLAY_SIGNAL_SIGNATURE, state.as_signal_values())
        self._shell_parameters = GLib.Variant(SHELL_SIGNAL_SIGNATURE, (state.as_shell_values(),))
        return self.replay()

    def replay(self) -> bool:
        """Synchronize a newly attached shell using only the last bounded display snapshot."""
        try:
            self.connection.emit_signal(
                None,
                OVERLAY_OBJECT_PATH,
                OVERLAY_INTERFACE,
                OVERLAY_SIGNAL,
                self._parameters,
            )
            self.connection.emit_signal(
                None, OVERLAY_OBJECT_PATH, OVERLAY_INTERFACE, SHELL_SIGNAL, self._shell_parameters
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
