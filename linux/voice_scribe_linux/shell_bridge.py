"""Desktop status bridge with an opt-in volatile preview and existing session-bus actions."""

import argparse
import json
import sys
from collections.abc import Callable

import gi

gi.require_version("Gio", "2.0")
from gi.repository import Gio, GLib  # noqa: E402

from voice_scribe_linux.overlay_state import (  # noqa: E402
    OVERLAY_INTERFACE,
    OVERLAY_OBJECT_PATH,
    OVERLAY_SIGNAL,
    OVERLAY_SIGNAL_SIGNATURE,
    VISIBLE_PHASES,
)

BUS_NAME = "com.voicescribe.Linux"
ACTION_PATH = "/com/voicescribe/Linux"
ACTIONS = ("record", "cancel", "latest", "status")


def project_state(parameters: GLib.Variant, overlay: bool = False) -> dict[str, object]:
    """Keep status content-free unless the floating preview is explicitly requested."""
    if parameters.get_type_string() != OVERLAY_SIGNAL_SIGNATURE:
        return {"phase": "unavailable", "elapsed": 0}
    visible, phase, _detail, elapsed, _mode, _route, level, preview, _delivery = parameters.unpack()
    if not visible or phase == "hidden":
        return {"phase": "idle", "elapsed": 0}
    state = {
        "phase": phase if phase in VISIBLE_PHASES else "unavailable",
        "elapsed": min(elapsed, 86_400),
    }
    if overlay and phase in VISIBLE_PHASES:
        state.update(level=max(0.0, min(level, 1.0)), preview=" ".join(preview.split())[-180:])
    return state


def activate(connection: Gio.DBusConnection, owner: str, action: str) -> None:
    """Send one bounded action to an existing unique owner, never D-Bus-activate Mluva."""
    if action not in ACTIONS:
        raise ValueError("Unsupported Mluva action")
    connection.call_sync(
        owner,
        ACTION_PATH,
        "org.gtk.Actions",
        "Activate",
        GLib.Variant("(sava{sv})", (action, [], {})),
        None,
        Gio.DBusCallFlags.NO_AUTO_START,
        1500,
        None,
    )


def current_owner(connection: Gio.DBusConnection) -> str:
    """Resolve an already running application without requesting service activation."""
    reply = connection.call_sync(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "GetNameOwner",
        GLib.Variant("(s)", (BUS_NAME,)),
        GLib.VariantType.new("(s)"),
        Gio.DBusCallFlags.NO_AUTO_START,
        1500,
        None,
    )
    return reply.unpack()[0]


class StatusWatch:
    """Track owner changes and replay status without reading files or starting capture."""

    def __init__(
        self, connection: Gio.DBusConnection, emit: Callable[[dict[str, object]], None], overlay: bool = False
    ) -> None:
        """Retain the session connection and injectable JSON sink."""
        self.connection = connection
        self.emit = emit
        self.overlay = overlay
        self.owner: str | None = None
        self.subscription = 0
        self.watch = 0

    def start(self) -> None:
        """Watch without auto-start and subscribe before asking for the last snapshot."""
        self.watch = Gio.bus_watch_name_on_connection(
            self.connection, BUS_NAME, Gio.BusNameWatcherFlags.NONE, self._appeared, self._vanished
        )

    def _appeared(self, _connection: Gio.DBusConnection, _name: str, owner: str) -> None:
        """Subscribe to the new process before asking it to replay its current state."""
        self._unsubscribe()
        self.owner = owner
        self.emit({"phase": "unavailable", "elapsed": 0})
        self.subscription = self.connection.signal_subscribe(
            owner,
            OVERLAY_INTERFACE,
            OVERLAY_SIGNAL,
            OVERLAY_OBJECT_PATH,
            None,
            Gio.DBusSignalFlags.NONE,
            self._changed,
        )
        try:
            activate(self.connection, owner, "status")
        except GLib.Error:
            self.emit({"phase": "unavailable", "elapsed": 0})

    def _changed(
        self,
        _connection: Gio.DBusConnection,
        sender: str,
        _path: str,
        _interface: str,
        _signal: str,
        parameters: GLib.Variant,
    ) -> None:
        """Ignore queued snapshots from former owners."""
        if sender == self.owner:
            self.emit(project_state(parameters, self.overlay))

    def _vanished(self, _connection: Gio.DBusConnection, _name: str) -> None:
        """Erase the previous process's state when it exits."""
        self._unsubscribe()
        self.emit({"phase": "stopped", "elapsed": 0})

    def _unsubscribe(self) -> None:
        """Release the old sender filter before replacing or closing the watch."""
        self.owner = None
        if self.subscription:
            self.connection.signal_unsubscribe(self.subscription)
            self.subscription = 0

    def close(self) -> None:
        """Release the name watch and discard the former owner's subscription."""
        if self.watch:
            Gio.bus_unwatch_name(self.watch)
            self.watch = 0
        self._unsubscribe()


def main() -> int:
    """Watch status or invoke one deliberate action without loading the GTK application."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("watch", *ACTIONS))
    parser.add_argument("--overlay", action="store_true", help="Include a volatile preview for the floating widget")
    args = parser.parse_args()
    try:
        connection = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        if args.action != "watch":
            activate(connection, current_owner(connection), args.action)
            return 0
        loop = GLib.MainLoop()

        def emit(state: dict[str, object]) -> None:
            """Write one snapshot to the shell and stop when it disconnects."""
            try:
                print(json.dumps(state), flush=True)
            except BrokenPipeError:
                loop.quit()

        watch = StatusWatch(connection, emit, args.overlay)
        watch.start()
        connection.connect("closed", lambda *_args: loop.quit())
        try:
            loop.run()
        finally:
            watch.close()
    except GLib.Error:
        print("Mluva unavailable. Start the configured Mluva application first.", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
