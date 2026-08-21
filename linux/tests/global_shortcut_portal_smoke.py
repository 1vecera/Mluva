"""Exercise Mluva's GlobalShortcuts client against a synthetic peer on a private D-Bus."""

from __future__ import annotations

import asyncio
from typing import Final

from dbus_next import Message, MessageType, RequestNameReply, Variant
from dbus_next.aio import MessageBus

from voice_scribe_linux.global_shortcuts import (
    CANCEL_SHORTCUT_ID,
    BoundShortcut,
    Shortcut,
    _PortalGlobalShortcutsSession,
    _PortalShortcutCallback,
)

PORTAL_BUS_NAME: Final = "org.freedesktop.portal.Desktop"
PORTAL_PATH: Final = "/org/freedesktop/portal/desktop"
GLOBAL_SHORTCUTS_INTERFACE: Final = "org.freedesktop.portal.GlobalShortcuts"
REQUEST_INTERFACE: Final = "org.freedesktop.portal.Request"
SESSION_INTERFACE: Final = "org.freedesktop.portal.Session"
REGISTRY_INTERFACE: Final = "org.freedesktop.host.portal.Registry"


class SyntheticPortal:
    """Implement the exact successful request flow needed by one shortcut session."""

    def __init__(self, bus: MessageBus):
        """Retain the private service connection and observable protocol state."""
        self.bus = bus
        self.registered_app_ids: list[str] = []
        self.bound_shortcuts: list[list[object]] = []
        self.session_handle: str | None = None
        self.errors: list[str] = []

    def handle_message(self, message: Message) -> Message | bool:
        """Reply to portal method calls and schedule their documented response signals."""
        if message.message_type != MessageType.METHOD_CALL:
            return False
        try:
            if message.interface == REGISTRY_INTERFACE and message.member == "Register":
                app_id = message.body[0]
                if not isinstance(app_id, str):
                    raise ValueError("Register app id was not a string")
                self.registered_app_ids.append(app_id)
                return Message.new_method_return(message)
            if message.interface == GLOBAL_SHORTCUTS_INTERFACE and message.member == "CreateSession":
                return self._create_session(message)
            if message.interface == GLOBAL_SHORTCUTS_INTERFACE and message.member == "BindShortcuts":
                return self._bind_shortcuts(message)
            if message.interface in {REQUEST_INTERFACE, SESSION_INTERFACE} and message.member == "Close":
                return Message.new_method_return(message)
            return False
        except (KeyError, TypeError, ValueError) as error:
            self.errors.append(str(error))
            return Message.new_error(
                message, "org.freedesktop.portal.Error.Failed", "Synthetic portal rejected request"
            )

    def emit_shortcut(self, member: str, shortcut_id: str) -> None:
        """Emit one activation or deactivation for the active synthetic session."""
        session_handle = self._require_session()
        self.bus.send(
            Message.new_signal(
                PORTAL_PATH,
                GLOBAL_SHORTCUTS_INTERFACE,
                member,
                "osta{sv}",
                [session_handle, shortcut_id, 1, {}],
            )
        )

    def emit_shortcuts_changed(self, trigger: str) -> None:
        """Report a compositor-side trigger change for the recording action."""
        session_handle = self._require_session()
        self.bus.send(
            Message.new_signal(
                PORTAL_PATH,
                GLOBAL_SHORTCUTS_INTERFACE,
                "ShortcutsChanged",
                "oa(sa{sv})",
                [
                    session_handle,
                    [
                        [
                            "toggle-recording-f9",
                            {
                                "description": Variant("s", "Record"),
                                "trigger_description": Variant("s", trigger),
                            },
                        ]
                    ],
                ],
            )
        )

    def _create_session(self, message: Message) -> Message:
        """Create the predictable session path and publish it through Request.Response."""
        options = message.body[0]
        sender_component = self._sender_component(message)
        request_handle = self._request_handle(sender_component, options)
        session_token = self._variant_string(options, "session_handle_token")
        self.session_handle = f"/org/freedesktop/portal/desktop/session/{sender_component}/{session_token}"
        self._schedule_response(
            request_handle,
            {"session_handle": Variant("s", self.session_handle)},
        )
        return Message.new_method_return(message, "o", [request_handle])

    def _bind_shortcuts(self, message: Message) -> Message:
        """Approve the proposed triggers and return their display descriptions."""
        if message.body[0] != self._require_session():
            raise ValueError("BindShortcuts used the wrong session")
        shortcuts = message.body[1]
        if not isinstance(shortcuts, list):
            raise ValueError("BindShortcuts payload was not an array")
        self.bound_shortcuts = shortcuts
        sender_component = self._sender_component(message)
        request_handle = self._request_handle(sender_component, message.body[-1])
        approved = [
            [
                shortcut_id,
                {
                    "description": properties["description"],
                    "trigger_description": Variant("s", properties["preferred_trigger"].value),
                },
            ]
            for shortcut_id, properties in shortcuts
        ]
        self._schedule_response(request_handle, {"shortcuts": Variant("a(sa{sv})", approved)})
        return Message.new_method_return(message, "o", [request_handle])

    def _schedule_response(self, path: str, results: dict[str, Variant]) -> None:
        """Send the result after the method return so the request follows real portal ordering."""
        signal = Message.new_signal(path, REQUEST_INTERFACE, "Response", "ua{sv}", [0, results])
        asyncio.get_running_loop().call_soon(self.bus.send, signal)

    @staticmethod
    def _sender_component(message: Message) -> str:
        """Convert the caller's unique bus name into a portal path component."""
        sender = message.sender
        if sender is None or not sender.startswith(":"):
            raise ValueError("Portal request had no unique sender")
        return sender[1:].replace(".", "_")

    def _request_handle(self, sender_component: str, options: object) -> str:
        """Return the request path selected by the caller's opaque token."""
        if not isinstance(options, dict):
            raise ValueError("Portal request options were not a vardict")
        token = self._variant_string(options, "handle_token")
        return f"/org/freedesktop/portal/desktop/request/{sender_component}/{token}"

    @staticmethod
    def _variant_string(options: dict[str, object], key: str) -> str:
        """Validate one required string variant."""
        value = options[key]
        if not isinstance(value, Variant) or not isinstance(value.value, str):
            raise ValueError(f"Portal option {key} was not a string variant")
        return value.value

    def _require_session(self) -> str:
        """Return the created session handle."""
        if self.session_handle is None:
            raise ValueError("Shortcut session was not created")
        return self.session_handle


async def exercise_private_portal() -> None:
    """Run the production client against a second connection on the same private bus."""
    service_bus = await MessageBus().connect()
    name_reply = await service_bus.request_name(PORTAL_BUS_NAME)
    if name_reply not in {RequestNameReply.PRIMARY_OWNER, RequestNameReply.ALREADY_OWNER}:
        raise RuntimeError("Synthetic portal could not own its private bus name")
    portal = SyntheticPortal(service_bus)
    service_bus.add_message_handler(portal.handle_message)
    activations: list[str] = []
    bindings: list[str | None] = []
    errors: list[str] = []
    callback = _PortalShortcutCallback(
        recording_id="toggle-recording-f9",
        on_toggle_recording=lambda: activations.append("toggle"),
        on_cancel=lambda: activations.append("cancel"),
        on_binding_changed=bindings.append,
        on_error=errors.append,
    )
    session = _PortalGlobalShortcutsSession("com.voicescribe.Linux", callback)
    try:
        bound = await session.connect(
            [
                Shortcut("toggle-recording-f9", "Start or stop Mluva recording with F9", "F9"),
                Shortcut(CANCEL_SHORTCUT_ID, "Cancel active Mluva capture", "CTRL+ALT+ESCAPE"),
            ]
        )
        portal.emit_shortcut("Activated", "toggle-recording-f9")
        portal.emit_shortcut("Activated", "toggle-recording-f9")
        portal.emit_shortcut("Deactivated", "toggle-recording-f9")
        portal.emit_shortcut("Activated", "toggle-recording-f9")
        portal.emit_shortcut("Activated", CANCEL_SHORTCUT_ID)
        portal.emit_shortcut("Deactivated", CANCEL_SHORTCUT_ID)
        portal.emit_shortcuts_changed("F10")
        await asyncio.sleep(0.05)

        expected_bound = [
            BoundShortcut("toggle-recording-f9", "Start or stop Mluva recording with F9", "F9"),
            BoundShortcut(CANCEL_SHORTCUT_ID, "Cancel active Mluva capture", "CTRL+ALT+ESCAPE"),
        ]
        if bound != expected_bound:
            raise RuntimeError(f"Unexpected approved shortcuts: {bound!r}")
        if portal.registered_app_ids != ["com.voicescribe.Linux"]:
            raise RuntimeError(f"Unexpected registered app ids: {portal.registered_app_ids!r}")
        bound_identifiers = [shortcut_id for shortcut_id, _properties in portal.bound_shortcuts]
        if bound_identifiers != ["toggle-recording-f9", CANCEL_SHORTCUT_ID]:
            raise RuntimeError(f"Unexpected bound identifiers: {bound_identifiers!r}")
        if activations != ["toggle", "toggle", "cancel"]:
            raise RuntimeError(f"Shortcut activation filtering failed: {activations!r}")
        if bindings != ["F10"]:
            raise RuntimeError(f"Shortcut change propagation failed: {bindings!r}")
        if errors or portal.errors:
            raise RuntimeError(f"Portal smoke reported errors: client={errors!r}, service={portal.errors!r}")
    finally:
        await session.close()
        service_bus.remove_message_handler(portal.handle_message)
        service_bus.disconnect()

    print("private_dbus_portal_registration=passed")
    print("private_dbus_portal_binding=passed")
    print("private_dbus_shortcut_lifecycle=passed")


def main() -> None:
    """Run the isolated portal contract smoke."""
    asyncio.run(exercise_private_portal())


if __name__ == "__main__":
    main()
