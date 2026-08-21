"""Portal-backed function-key recording and cancellation shortcuts."""

from __future__ import annotations

import asyncio
import os
import threading
import uuid
from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path
from typing import Final

from dbus_next import BusType, Message, MessageType, Variant
from dbus_next.aio import MessageBus

from voice_scribe_linux.config import DEFAULT_GLOBAL_RECORDING_KEY, FUNCTION_KEY_OPTIONS

RECORDING_SHORTCUT_PREFIX = "toggle-recording-"
CANCEL_SHORTCUT_ID = "cancel-capture"

_PORTAL_BUS_NAME: Final = "org.freedesktop.portal.Desktop"
_PORTAL_PATH: Final = "/org/freedesktop/portal/desktop"
_GLOBAL_SHORTCUTS_INTERFACE: Final = "org.freedesktop.portal.GlobalShortcuts"
_HOST_REGISTRY_INTERFACE: Final = "org.freedesktop.host.portal.Registry"
_REQUEST_INTERFACE: Final = "org.freedesktop.portal.Request"
_SESSION_INTERFACE: Final = "org.freedesktop.portal.Session"
_DBUS_BUS_NAME: Final = "org.freedesktop.DBus"
_DBUS_PATH: Final = "/org/freedesktop/DBus"
_DBUS_INTERFACE: Final = "org.freedesktop.DBus"
_REQUEST_PATH_PREFIX: Final = "/org/freedesktop/portal/desktop/request"
_SESSION_PATH_PREFIX: Final = "/org/freedesktop/portal/desktop/session"


class PortalProtocolError(RuntimeError):
    """Report a controlled portal or D-Bus protocol failure."""


@dataclass(frozen=True, slots=True)
class Shortcut:
    """Describe one shortcut proposed to the desktop portal."""

    id: str
    description: str
    preferred_trigger: str


@dataclass(frozen=True, slots=True)
class BoundShortcut:
    """Preserve the trigger the desktop actually assigned."""

    id: str
    description: str
    trigger_description: str


def recording_shortcut_id(function_key: str) -> str:
    """Return a key-specific ID so the desktop replaces a changed trigger."""
    if function_key not in FUNCTION_KEY_OPTIONS:
        raise ValueError("function_key must be one of F1 through F24")
    return f"{RECORDING_SHORTCUT_PREFIX}{function_key.lower()}"


class _PortalShortcutCallback:
    """Translate only the recording toggle and cancellation portal bindings."""

    def __init__(
        self,
        recording_id: str,
        on_toggle_recording: Callable[[], None],
        on_cancel: Callable[[], None],
        on_binding_changed: Callable[[str | None], None],
        on_error: Callable[[str], None],
    ):
        """Retain the exact shortcut ID and application callbacks."""
        self.recording_id = recording_id
        self.toggle_recording = on_toggle_recording
        self.cancel_capture = on_cancel
        self.report_binding = on_binding_changed
        self.report_error = on_error
        self.active_shortcuts: set[str] = set()

    def on_activated(self, shortcut_id: str) -> None:
        """Toggle recording or cancel when the compositor activates a binding."""
        if shortcut_id not in {self.recording_id, CANCEL_SHORTCUT_ID}:
            return
        if shortcut_id in self.active_shortcuts:
            return
        self.active_shortcuts.add(shortcut_id)
        if shortcut_id == self.recording_id:
            self.toggle_recording()
        else:
            self.cancel_capture()

    def on_deactivated(self, shortcut_id: str) -> None:
        """Re-arm one-shot activation without applying release-to-stop behavior."""
        self.active_shortcuts.discard(shortcut_id)

    def on_shortcuts_changed(self, shortcuts: list[BoundShortcut]) -> None:
        """Keep the application status aligned with desktop-side edits."""
        self.report_binding(self._recording_trigger(shortcuts))

    def on_error(self, message: str) -> None:
        """Expose a controlled portal failure without terminating the application."""
        self.report_error(message)

    def _recording_trigger(self, shortcuts: list[BoundShortcut]) -> str | None:
        """Return the compositor's display trigger for this recording action."""
        for shortcut in shortcuts:
            if shortcut.id == self.recording_id:
                trigger = shortcut.trigger_description.strip()
                return trigger or None
        return None


def _new_session_bus() -> MessageBus:
    """Create the MIT-licensed D-Bus client used for the official portal protocol."""
    return MessageBus(bus_type=BusType.SESSION)


def _method_return(reply: Message | None, operation: str) -> Message:
    """Validate a D-Bus method reply without surfacing arbitrary remote text."""
    if reply is None:
        raise PortalProtocolError(f"{operation} returned no D-Bus reply.")
    if reply.message_type == MessageType.ERROR:
        error_name = reply.error_name or "unknown D-Bus error"
        raise PortalProtocolError(f"{operation} failed ({error_name}).")
    if reply.message_type != MessageType.METHOD_RETURN:
        raise PortalProtocolError(f"{operation} returned an unexpected D-Bus message.")
    return reply


def _sender_path_component(unique_name: str | None) -> str:
    """Convert the caller's unique bus name into the portal request path component."""
    if unique_name is None or not unique_name.startswith(":"):
        raise PortalProtocolError("The session bus did not assign a unique caller name.")
    return unique_name[1:].replace(".", "_")


def _variant_text(properties: dict[str, object], key: str) -> str:
    """Read one optional string property from a portal vardict."""
    value = properties.get(key)
    if value is None:
        return ""
    if not isinstance(value, Variant) or not isinstance(value.value, str):
        raise PortalProtocolError(f"The portal returned an invalid {key} property.")
    return value.value


def _parse_bound_shortcuts(value: object) -> list[BoundShortcut]:
    """Validate the bound-shortcut array returned by the desktop."""
    if not isinstance(value, list):
        raise PortalProtocolError("The portal returned an invalid shortcut list.")
    shortcuts: list[BoundShortcut] = []
    for entry in value:
        if not isinstance(entry, (list, tuple)) or len(entry) != 2:
            raise PortalProtocolError("The portal returned an invalid shortcut entry.")
        shortcut_id, properties = entry
        if not isinstance(shortcut_id, str) or not isinstance(properties, dict):
            raise PortalProtocolError("The portal returned an invalid shortcut entry.")
        shortcuts.append(
            BoundShortcut(
                id=shortcut_id,
                description=_variant_text(properties, "description"),
                trigger_description=_variant_text(properties, "trigger_description"),
            )
        )
    return shortcuts


class _PortalGlobalShortcutsSession:
    """Implement the official XDG GlobalShortcuts protocol directly over D-Bus."""

    def __init__(self, app_id: str, callback: _PortalShortcutCallback):
        """Retain the desktop identity and event callback for one immutable session."""
        self.app_id = app_id
        self.callback = callback
        self._bus: MessageBus | None = None
        self._session_handle: str | None = None
        self._pending_requests: dict[str, asyncio.Future[tuple[int, dict[str, object]]]] = {}
        self._early_responses: dict[str, tuple[int, dict[str, object]] | PortalProtocolError] = {}
        self._active_request_handles: set[str] = set()
        self._closing = False

    async def connect(self, shortcuts: list[Shortcut]) -> list[BoundShortcut]:
        """Register the host app, create one session, and bind its shortcuts once."""
        if self._bus is not None:
            raise PortalProtocolError("The global shortcut session is already connected.")
        self._closing = False
        bus = await _new_session_bus().connect()
        self._bus = bus
        bus.add_message_handler(self._handle_message)
        try:
            await self._register_host_application()
            await self._add_signal_matches()
            session_token = f"mluva_session_{uuid.uuid4().hex}"
            create_results = await self._portal_request(
                member="CreateSession",
                signature="a{sv}",
                positional_body=[],
                options={"session_handle_token": Variant("s", session_token)},
            )
            session_variant = create_results.get("session_handle")
            if not isinstance(session_variant, Variant) or not isinstance(session_variant.value, str):
                raise PortalProtocolError("The portal did not return a valid shortcut session handle.")
            sender = _sender_path_component(bus.unique_name)
            expected_session_handle = f"{_SESSION_PATH_PREFIX}/{sender}/{session_token}"
            if session_variant.value != expected_session_handle:
                raise PortalProtocolError("The portal returned an unexpected shortcut session handle.")
            self._session_handle = session_variant.value
            shortcut_payload = [
                [
                    shortcut.id,
                    {
                        "description": Variant("s", shortcut.description),
                        "preferred_trigger": Variant("s", shortcut.preferred_trigger),
                    },
                ]
                for shortcut in shortcuts
            ]
            bind_results = await self._portal_request(
                member="BindShortcuts",
                signature="oa(sa{sv})sa{sv}",
                positional_body=[self._session_handle, shortcut_payload, ""],
                options={},
            )
            shortcuts_variant = bind_results.get("shortcuts")
            if not isinstance(shortcuts_variant, Variant):
                raise PortalProtocolError("The portal did not return its approved shortcuts.")
            return _parse_bound_shortcuts(shortcuts_variant.value)
        except Exception:
            await self.close()
            raise

    async def close(self) -> None:
        """Close pending requests and the portal session, then disconnect the bus."""
        self._closing = True
        bus = self._bus
        if bus is None:
            return
        for request_handle in tuple(self._active_request_handles):
            await self._best_effort_call(
                Message(
                    destination=_PORTAL_BUS_NAME,
                    path=request_handle,
                    interface=_REQUEST_INTERFACE,
                    member="Close",
                )
            )
        session_handle = self._session_handle
        if session_handle is not None:
            await self._best_effort_call(
                Message(
                    destination=_PORTAL_BUS_NAME,
                    path=session_handle,
                    interface=_SESSION_INTERFACE,
                    member="Close",
                )
            )
        for future in tuple(self._pending_requests.values()):
            if not future.done():
                future.set_exception(PortalProtocolError("The global shortcut request was closed."))
        self._pending_requests.clear()
        self._early_responses.clear()
        self._active_request_handles.clear()
        self._session_handle = None
        try:
            bus.remove_message_handler(self._handle_message)
        except Exception:
            pass
        try:
            bus.disconnect()
        except Exception:
            pass
        self._bus = None

    async def _register_host_application(self) -> None:
        """Associate an unsandboxed process with its installed desktop identity when supported."""
        if os.environ.get("FLATPAK_ID") or Path("/.flatpak-info").exists():
            return
        bus = self._require_bus()
        try:
            reply = await bus.call(
                Message(
                    destination=_PORTAL_BUS_NAME,
                    path=_PORTAL_PATH,
                    interface=_HOST_REGISTRY_INTERFACE,
                    member="Register",
                    signature="sa{sv}",
                    body=[self.app_id, {}],
                )
            )
        except Exception:
            return
        if reply is not None and reply.message_type == MessageType.METHOD_RETURN:
            return
        # The host registry is optional and expected to disappear eventually; the portal can infer the app id.

    async def _add_signal_matches(self) -> None:
        """Subscribe before the first portal request so immediate responses cannot race registration."""
        await self._add_match(
            "type='signal',sender='org.freedesktop.portal.Desktop',"
            "interface='org.freedesktop.portal.Request',member='Response'"
        )
        await self._add_match(
            "type='signal',sender='org.freedesktop.portal.Desktop',"
            "path='/org/freedesktop/portal/desktop',"
            "interface='org.freedesktop.portal.GlobalShortcuts'"
        )
        await self._add_match(
            "type='signal',sender='org.freedesktop.portal.Desktop',"
            "interface='org.freedesktop.portal.Session',member='Closed'"
        )

    async def _add_match(self, rule: str) -> None:
        """Install one D-Bus daemon signal match for this private connection."""
        bus = self._require_bus()
        reply = await bus.call(
            Message(
                destination=_DBUS_BUS_NAME,
                path=_DBUS_PATH,
                interface=_DBUS_INTERFACE,
                member="AddMatch",
                signature="s",
                body=[rule],
            )
        )
        _method_return(reply, "D-Bus signal subscription")

    async def _portal_request(
        self,
        member: str,
        signature: str,
        positional_body: list[object],
        options: dict[str, Variant],
    ) -> dict[str, object]:
        """Run one race-free portal request and return its validated result vardict."""
        bus = self._require_bus()
        token = f"mluva_{uuid.uuid4().hex}"
        sender = _sender_path_component(bus.unique_name)
        expected_handle = f"{_REQUEST_PATH_PREFIX}/{sender}/{token}"
        request_options = {**options, "handle_token": Variant("s", token)}
        future: asyncio.Future[tuple[int, dict[str, object]]] = asyncio.get_running_loop().create_future()
        self._pending_requests[expected_handle] = future
        self._active_request_handles.add(expected_handle)
        returned_handle = expected_handle
        try:
            reply = await bus.call(
                Message(
                    destination=_PORTAL_BUS_NAME,
                    path=_PORTAL_PATH,
                    interface=_GLOBAL_SHORTCUTS_INTERFACE,
                    member=member,
                    signature=signature,
                    body=[*positional_body, request_options],
                )
            )
            reply = _method_return(reply, f"Global shortcut {member}")
            if len(reply.body) != 1 or not isinstance(reply.body[0], str):
                raise PortalProtocolError(f"Global shortcut {member} returned an invalid request handle.")
            returned_handle = reply.body[0]
            if not returned_handle.startswith(f"{_REQUEST_PATH_PREFIX}/{sender}/"):
                raise PortalProtocolError(f"Global shortcut {member} returned an unexpected request handle.")
            if returned_handle != expected_handle and not future.done():
                self._pending_requests.pop(expected_handle, None)
                self._active_request_handles.discard(expected_handle)
                self._pending_requests[returned_handle] = future
                self._active_request_handles.add(returned_handle)
                early_response = self._early_responses.pop(returned_handle, None)
                if isinstance(early_response, PortalProtocolError):
                    future.set_exception(early_response)
                elif early_response is not None:
                    future.set_result(early_response)
            response, results = await future
            if response == 1:
                raise PortalProtocolError("Global shortcut approval was cancelled.")
            if response != 0:
                raise PortalProtocolError("The desktop could not complete global shortcut approval.")
            return results
        finally:
            for path, pending in tuple(self._pending_requests.items()):
                if pending is future:
                    self._pending_requests.pop(path, None)
                    self._active_request_handles.discard(path)
            self._active_request_handles.discard(expected_handle)
            self._active_request_handles.discard(returned_handle)

    def _handle_message(self, message: Message) -> bool:
        """Dispatch only response, shortcut, and session signals owned by this connection."""
        if message.message_type != MessageType.SIGNAL:
            return False
        if message.interface == _REQUEST_INTERFACE and message.member == "Response":
            self._handle_request_response(message)
            return False
        if message.interface == _GLOBAL_SHORTCUTS_INTERFACE and message.path == _PORTAL_PATH:
            self._handle_shortcut_signal(message)
            return False
        if message.interface == _SESSION_INTERFACE and message.member == "Closed":
            if message.path == self._session_handle:
                self._session_handle = None
                if not self._closing:
                    self.callback.on_error("The desktop closed Mluva's global shortcut session.")
        return False

    def _handle_request_response(self, message: Message) -> None:
        """Complete the request future selected by the signal's object path."""
        path = message.path
        if path is None:
            return
        response_or_error: tuple[int, dict[str, object]] | PortalProtocolError
        if len(message.body) != 2 or not isinstance(message.body[0], int) or not isinstance(message.body[1], dict):
            response_or_error = PortalProtocolError("The portal returned an invalid request response.")
        else:
            response_or_error = (message.body[0], message.body[1])
        future = self._pending_requests.get(path)
        if future is not None and not future.done():
            if isinstance(response_or_error, PortalProtocolError):
                future.set_exception(response_or_error)
            else:
                future.set_result(response_or_error)
            return
        if len(self._early_responses) < 8:
            self._early_responses[path] = response_or_error

    def _handle_shortcut_signal(self, message: Message) -> None:
        """Filter every portal event to the exact active session before invoking application code."""
        try:
            if message.member in {"Activated", "Deactivated"}:
                if len(message.body) != 4:
                    raise PortalProtocolError("The portal returned an invalid shortcut activation.")
                session_handle, shortcut_id = message.body[:2]
                if session_handle != self._session_handle or not isinstance(shortcut_id, str):
                    return
                if message.member == "Activated":
                    self.callback.on_activated(shortcut_id)
                else:
                    self.callback.on_deactivated(shortcut_id)
                return
            if message.member == "ShortcutsChanged":
                if len(message.body) != 2 or message.body[0] != self._session_handle:
                    return
                self.callback.on_shortcuts_changed(_parse_bound_shortcuts(message.body[1]))
        except PortalProtocolError as error:
            self.callback.on_error(str(error))

    async def _best_effort_call(self, message: Message) -> None:
        """Attempt cleanup without replacing the original portal outcome."""
        bus = self._bus
        if bus is None:
            return
        try:
            await bus.call(message)
        except Exception:
            pass

    def _require_bus(self) -> MessageBus:
        """Return the connected private bus or fail with a controlled error."""
        if self._bus is None:
            raise PortalProtocolError("The global shortcut portal is not connected.")
        return self._bus


@dataclass(slots=True)
class GlobalShortcutService:
    """Own portal-approved recording-toggle and cancellation bindings."""

    on_toggle_recording: Callable[[], None]
    on_cancel: Callable[[], None]
    on_binding_changed: Callable[[str, str | None], None]
    on_error: Callable[[str], None]
    preferred_recording_trigger: str = DEFAULT_GLOBAL_RECORDING_KEY
    preferred_cancel_trigger: str = "CTRL+ALT+ESCAPE"
    _thread: threading.Thread | None = None
    _loop: asyncio.AbstractEventLoop | None = None
    _session: _PortalGlobalShortcutsSession | None = None
    _session_lock: asyncio.Lock | None = None
    _bound_recording_trigger: str | None = None
    _ready: threading.Event = field(default_factory=threading.Event)
    _closing: bool = False

    def __post_init__(self) -> None:
        """Reject unsupported recording keys before contacting the desktop."""
        recording_shortcut_id(self.preferred_recording_trigger)

    def start(self) -> None:
        """Connect and bind both shortcuts without blocking GTK startup."""
        if self._thread is not None:
            return
        self._closing = False
        self._ready.clear()
        self._thread = threading.Thread(target=self._run, name="global-shortcut-portal", daemon=True)
        self._thread.start()

    def set_recording_key(self, function_key: str) -> None:
        """Replace the portal session with one that prefers the selected key."""
        recording_shortcut_id(function_key)
        if function_key == self.preferred_recording_trigger:
            return
        self.preferred_recording_trigger = function_key
        loop = self._loop
        if loop is not None and loop.is_running():
            asyncio.run_coroutine_threadsafe(self._replace_session(function_key), loop)

    def close(self) -> None:
        """Release the portal session and stop its background event loop."""
        self._closing = True
        loop = self._loop
        if loop is not None and loop.is_running():
            future = asyncio.run_coroutine_threadsafe(self._close_async(), loop)
            try:
                future.result(timeout=5)
            except Exception:
                pass
            finally:
                loop.call_soon_threadsafe(loop.stop)
        if self._thread is not None:
            self._thread.join(timeout=5)
        self._thread = None
        self._loop = None

    def _run(self) -> None:
        """Create and run the asyncio loop used by the portal client."""
        self._loop = asyncio.new_event_loop()
        asyncio.set_event_loop(self._loop)
        self._session_lock = asyncio.Lock()
        try:
            self._loop.run_until_complete(self._replace_session(self.preferred_recording_trigger))
            self._ready.set()
            self._loop.run_forever()
        finally:
            self._ready.set()
            if self._loop is not None and not self._loop.is_closed():
                self._loop.run_until_complete(self._close_async())
                self._loop.close()
            self._session_lock = None

    async def _connect(self, function_key: str) -> None:
        """Create one session containing recording and cancellation actions."""
        recording_id = recording_shortcut_id(function_key)
        callback = _PortalShortcutCallback(
            recording_id=recording_id,
            on_toggle_recording=self.on_toggle_recording,
            on_cancel=self.on_cancel,
            on_binding_changed=lambda trigger: self.on_binding_changed(function_key, trigger),
            on_error=self.on_error,
        )
        session = _PortalGlobalShortcutsSession(
            app_id="com.voicescribe.Linux",
            callback=callback,
        )
        self._session = session
        try:
            bound_shortcuts = await session.connect(
                [
                    Shortcut(
                        id=recording_id,
                        description=f"Start or stop Mluva recording with {function_key}",
                        preferred_trigger=function_key,
                    ),
                    Shortcut(
                        id=CANCEL_SHORTCUT_ID,
                        description="Cancel active Mluva capture",
                        preferred_trigger=self.preferred_cancel_trigger,
                    ),
                ]
            )
        except Exception as error:
            self.on_error(str(error))
            await session.close()
            if self._session is session:
                self._session = None
            raise
        self._bound_recording_trigger = function_key
        if function_key == self.preferred_recording_trigger and not self._closing:
            callback.on_shortcuts_changed(bound_shortcuts)

    async def _replace_session(self, function_key: str) -> None:
        """Replace one immutable portal session after a settings change."""
        session_lock = self._session_lock
        if session_lock is None:
            return
        async with session_lock:
            if (
                function_key != self.preferred_recording_trigger
                or self._closing
                or function_key == self._bound_recording_trigger
            ):
                return
            try:
                await self._close_async()
                await self._connect(function_key)
            except Exception:
                if function_key == self.preferred_recording_trigger and not self._closing:
                    self.on_binding_changed(function_key, None)

    async def _close_async(self) -> None:
        """Release the registered portal session and private bus connection."""
        session = self._session
        self._session = None
        if session is not None:
            await session.close()
        self._bound_recording_trigger = None
