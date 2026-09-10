"""Coverage for portal-backed recording and cancellation shortcuts."""

import asyncio
import threading
from typing import Self

import pytest
from dbus_next import Message, MessageType, Variant

import mluva_linux.global_shortcuts as shortcut_module
from mluva_linux.global_shortcuts import (
    CANCEL_SHORTCUT_ID,
    REWRITE_SHORTCUT_ID,
    BoundShortcut,
    GlobalShortcutService,
    PortalProtocolError,
    Shortcut,
    _PortalGlobalShortcutsSession,
    _PortalShortcutCallback,
    recording_shortcut_id,
)


@pytest.mark.parametrize("function_key", ["F1", "F9", "F13", "F24"])
def test_recording_shortcut_identifier_includes_selected_function_key(function_key: str) -> None:
    """Force the desktop to treat each selected key as a replacement action."""
    assert recording_shortcut_id(function_key) == f"toggle-recording-{function_key.lower()}"


@pytest.mark.parametrize("function_key", ["F0", "F25", "f9", "F9+CTRL", "RightAlt"])
def test_recording_shortcut_identifier_rejects_unsupported_keys(function_key: str) -> None:
    """Keep the global picker and portal registration inside F1 through F24."""
    with pytest.raises(ValueError, match="F1 through F24"):
        recording_shortcut_id(function_key)


def test_portal_callback_toggles_and_cancels_only_exact_actions() -> None:
    """Ignore unrelated and repeated activations while release only re-arms."""
    calls: list[str] = []
    callback = _PortalShortcutCallback(
        recording_id="toggle-recording-f9",
        on_toggle_recording=lambda: calls.append("toggle"),
        on_cancel=lambda: calls.append("cancel"),
        on_binding_changed=lambda trigger: calls.append(f"binding:{trigger}"),
        on_error=lambda message: calls.append(f"error:{message}"),
        on_open_rewrite=lambda: calls.append("rewrite"),
    )

    callback.on_activated("unrelated")
    callback.on_activated("toggle-recording-f8")
    callback.on_activated("toggle-recording-f9")
    callback.on_activated("toggle-recording-f9")
    callback.on_deactivated("toggle-recording-f9")
    callback.on_activated("toggle-recording-f9")
    callback.on_deactivated("toggle-recording-f9")
    callback.on_activated(CANCEL_SHORTCUT_ID)
    callback.on_activated(CANCEL_SHORTCUT_ID)
    callback.on_deactivated(CANCEL_SHORTCUT_ID)

    callback.on_activated(REWRITE_SHORTCUT_ID)
    callback.on_activated(REWRITE_SHORTCUT_ID)
    callback.on_deactivated(REWRITE_SHORTCUT_ID)
    callback.on_activated(REWRITE_SHORTCUT_ID)
    assert calls == ["toggle", "toggle", "cancel", "rewrite", "rewrite"]


def test_portal_callback_reports_actual_recording_binding() -> None:
    """Expose desktop-side edits without claiming the preferred trigger won."""
    triggers: list[str | None] = []
    callback = _PortalShortcutCallback(
        recording_id="toggle-recording-f9",
        on_toggle_recording=lambda: None,
        on_cancel=lambda: None,
        on_binding_changed=triggers.append,
        on_error=lambda _message: None,
    )

    callback.on_shortcuts_changed(
        [
            BoundShortcut("toggle-recording-f9", "Record", "F10"),
            BoundShortcut(CANCEL_SHORTCUT_ID, "Cancel", "Ctrl+Alt+Escape"),
        ]
    )
    callback.on_shortcuts_changed([BoundShortcut("toggle-recording-f9", "Record", "")])
    callback.on_shortcuts_changed([BoundShortcut(CANCEL_SHORTCUT_ID, "Cancel", "Ctrl+Alt+Escape")])

    assert triggers == ["F10", None, None]


def test_running_service_serializes_function_key_replacement(monkeypatch: pytest.MonkeyPatch) -> None:
    """Queue a settings change behind startup instead of racing two portal sessions."""
    bindings: list[list[tuple[str, str]]] = []
    app_ids: list[str] = []
    closed_sessions: list[int] = []
    triggers: list[tuple[str, str | None]] = []
    rebound = threading.Event()

    class SessionStub:
        """Return the preferred key for every synthetic binding."""

        def __init__(self, app_id: str, callback: object):
            """Retain the public identity and each session's close boundary."""
            app_ids.append(app_id)
            self.instance = len(app_ids) - 1

        async def connect(self, shortcuts: list[Shortcut]) -> list[BoundShortcut]:
            """Record the ordered recording action IDs."""
            recording = shortcuts[0]
            bindings.append([(shortcut.id, shortcut.preferred_trigger) for shortcut in shortcuts])
            return [
                BoundShortcut(recording.id, recording.description, recording.preferred_trigger),
            ]

        async def close(self) -> None:
            """Record release of each session after replacement and shutdown."""
            closed_sessions.append(self.instance)

    def binding_changed(function_key: str, trigger: str | None) -> None:
        """Keep actual approval evidence and release the waiting test after replacement."""
        triggers.append((function_key, trigger))
        if function_key == "F24":
            rebound.set()

    monkeypatch.setattr(shortcut_module, "_PortalGlobalShortcutsSession", SessionStub)
    service = GlobalShortcutService(
        on_toggle_recording=lambda: None,
        on_cancel=lambda: None,
        on_binding_changed=binding_changed,
        on_error=lambda _message: None,
    )

    service.start()
    try:
        assert service._ready.wait(timeout=1)
        service.set_recording_key("F24")
        assert rebound.wait(timeout=1)
    finally:
        service.close()

    assert app_ids == ["com.mluva.Linux"] * 2
    assert bindings == [
        [("toggle-recording-f9", "F9"), (CANCEL_SHORTCUT_ID, "CTRL+ALT+ESCAPE"), (REWRITE_SHORTCUT_ID, "SHIFT+F9")],
        [("toggle-recording-f24", "F24"), (CANCEL_SHORTCUT_ID, "CTRL+ALT+ESCAPE"), (REWRITE_SHORTCUT_ID, "SHIFT+F9")],
    ]
    assert triggers == [("F9", "F9"), ("F24", "F24")]
    assert closed_sessions == [0, 1]


class _PortalBusStub:
    """Emulate the official request, session, and shortcut signal protocol."""

    unique_name = ":1.42"

    def __init__(self, bind_response: int = 0):
        """Configure the synthetic desktop's BindShortcuts result."""
        self.bind_response = bind_response
        self.messages: list[Message] = []
        self.handler: object | None = None
        self.disconnected = False
        self.session_handle = ""

    async def connect(self) -> Self:
        """Return the already isolated synthetic bus."""
        return self

    def add_message_handler(self, handler: object) -> None:
        """Retain the production signal dispatcher."""
        self.handler = handler

    def remove_message_handler(self, handler: object) -> None:
        """Release the production signal dispatcher."""
        if self.handler == handler:
            self.handler = None

    def disconnect(self) -> None:
        """Record connection teardown."""
        self.disconnected = True

    async def call(self, message: Message) -> Message:
        """Return protocol-valid replies and schedule race-free response signals."""
        message.serial = len(self.messages) + 1
        message._marshall()  # noqa: SLF001 - serialize the exact production D-Bus payload in this protocol test
        self.messages.append(message)
        if message.member == "CreateSession":
            options = message.body[-1]
            request_handle = self._request_handle(options)
            session_token = options["session_handle_token"].value
            self.session_handle = f"/org/freedesktop/portal/desktop/session/1_42/{session_token}"
            self._schedule_response(
                request_handle,
                0,
                {"session_handle": Variant("s", self.session_handle)},
            )
            return self._method_return("o", [request_handle])
        if message.member == "BindShortcuts":
            options = message.body[-1]
            request_handle = self._request_handle(options)
            approved = [
                [
                    shortcut_id,
                    {
                        "description": properties["description"],
                        "trigger_description": Variant("s", properties["preferred_trigger"].value),
                    },
                ]
                for shortcut_id, properties in message.body[1]
            ]
            self._schedule_response(
                request_handle,
                self.bind_response,
                {"shortcuts": Variant("a(sa{sv})", approved)},
            )
            return self._method_return("o", [request_handle])
        return self._method_return()

    def emit(self, message: Message) -> None:
        """Deliver one signal to the registered production dispatcher."""
        handler = self.handler
        if callable(handler):
            handler(message)

    def _schedule_response(self, path: str, response: int, results: dict[str, Variant]) -> None:
        """Deliver a request result after the method call yields to the event loop."""
        signal = Message.new_signal(
            path,
            "org.freedesktop.portal.Request",
            "Response",
            "ua{sv}",
            [response, results],
        )
        asyncio.get_running_loop().call_soon(self.emit, signal)

    def _request_handle(self, options: dict[str, Variant]) -> str:
        """Apply the portal's documented predictable request-path convention."""
        token = options["handle_token"].value
        return f"/org/freedesktop/portal/desktop/request/1_42/{token}"

    @staticmethod
    def _method_return(signature: str = "", body: list[object] | None = None) -> Message:
        """Build one synthetic D-Bus method return."""
        return Message(
            message_type=MessageType.METHOD_RETURN,
            reply_serial=1,
            signature=signature,
            body=body or [],
        )


def test_official_portal_client_registers_binds_and_filters_signals(monkeypatch: pytest.MonkeyPatch) -> None:
    """Exercise the independent XDG protocol and its exact serialized D-Bus messages."""
    calls: list[str] = []
    bindings: list[str | None] = []
    errors: list[str] = []
    bus = _PortalBusStub()
    monkeypatch.delenv("FLATPAK_ID", raising=False)
    monkeypatch.setattr(shortcut_module, "_new_session_bus", lambda: bus)
    callback = _PortalShortcutCallback(
        recording_id="toggle-recording-f9",
        on_toggle_recording=lambda: calls.append("toggle"),
        on_cancel=lambda: calls.append("cancel"),
        on_binding_changed=bindings.append,
        on_error=errors.append,
    )
    session = _PortalGlobalShortcutsSession("com.mluva.Linux", callback)

    async def exercise_session() -> list[BoundShortcut]:
        """Bind, receive active-session events, and close the private bus."""
        bound = await session.connect(
            [
                Shortcut("toggle-recording-f9", "Start or stop Mluva recording with F9", "F9"),
                Shortcut(CANCEL_SHORTCUT_ID, "Cancel active Mluva capture", "CTRL+ALT+ESCAPE"),
            ]
        )
        other_session = "/org/freedesktop/portal/desktop/session/1_42/other"
        bus.emit(
            Message.new_signal(
                "/org/freedesktop/portal/desktop",
                "org.freedesktop.portal.GlobalShortcuts",
                "Activated",
                "osta{sv}",
                [other_session, "toggle-recording-f9", 1, {}],
            )
        )
        for member in ("Activated", "Activated", "Deactivated", "Activated"):
            bus.emit(
                Message.new_signal(
                    "/org/freedesktop/portal/desktop",
                    "org.freedesktop.portal.GlobalShortcuts",
                    member,
                    "osta{sv}",
                    [bus.session_handle, "toggle-recording-f9", 1, {}],
                )
            )
        bus.emit(
            Message.new_signal(
                "/org/freedesktop/portal/desktop",
                "org.freedesktop.portal.GlobalShortcuts",
                "ShortcutsChanged",
                "oa(sa{sv})",
                [
                    bus.session_handle,
                    [
                        [
                            "toggle-recording-f9",
                            {
                                "description": Variant("s", "Record"),
                                "trigger_description": Variant("s", "F10"),
                            },
                        ]
                    ],
                ],
            )
        )
        await session.close()
        return bound

    bound_shortcuts = asyncio.run(exercise_session())

    members = [message.member for message in bus.messages]
    assert members.index("Register") < members.index("CreateSession")
    assert members.count("AddMatch") == 3
    create_message = next(message for message in bus.messages if message.member == "CreateSession")
    assert create_message.signature == "a{sv}"
    assert create_message.body[0]["session_handle_token"].value.startswith("mluva_session_")
    bind_message = next(message for message in bus.messages if message.member == "BindShortcuts")
    assert bind_message.signature == "oa(sa{sv})sa{sv}"
    assert [(identifier, properties["preferred_trigger"].value) for identifier, properties in bind_message.body[1]] == [
        ("toggle-recording-f9", "F9"),
        (CANCEL_SHORTCUT_ID, "CTRL+ALT+ESCAPE"),
    ]
    assert [(shortcut.id, shortcut.trigger_description) for shortcut in bound_shortcuts] == [
        ("toggle-recording-f9", "F9"),
        (CANCEL_SHORTCUT_ID, "CTRL+ALT+ESCAPE"),
    ]
    assert calls == ["toggle", "toggle"]
    assert bindings == ["F10"]
    assert errors == []
    assert bus.disconnected is True


def test_official_portal_client_closes_after_user_rejects_binding(monkeypatch: pytest.MonkeyPatch) -> None:
    """Turn a desktop rejection into a controlled status and release the private bus."""
    bus = _PortalBusStub(bind_response=1)
    monkeypatch.setattr(shortcut_module, "_new_session_bus", lambda: bus)
    callback = _PortalShortcutCallback(
        recording_id="toggle-recording-f9",
        on_toggle_recording=lambda: None,
        on_cancel=lambda: None,
        on_binding_changed=lambda _trigger: None,
        on_error=lambda _message: None,
    )
    session = _PortalGlobalShortcutsSession("com.mluva.Linux", callback)

    with pytest.raises(PortalProtocolError, match="approval was cancelled"):
        asyncio.run(session.connect([Shortcut("toggle-recording-f9", "Start or stop Mluva recording with F9", "F9")]))

    assert bus.disconnected is True
