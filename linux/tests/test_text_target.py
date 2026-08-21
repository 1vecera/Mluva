"""Headless coverage for AT-SPI selection capture and restoration."""

from __future__ import annotations

from dataclasses import dataclass, field

import pytest

from voice_scribe_linux.text_target import (
    FocusedTextTargetTracker,
    TextSelectionTooLargeError,
    capture_focused_application_identifier,
    capture_focused_delivery_target,
    capture_focused_text_target,
    system_accessibility_enabled,
)

FOCUSED = "focused"
ACTIVE = "active"
EDITABLE = "editable"
PASSWORD = "password"
TEXT = "text"


@dataclass(frozen=True, slots=True)
class FakeRange:
    """Represent one AT-SPI selection range."""

    start_offset: int
    end_offset: int


@dataclass(slots=True)
class FakeStateSet:
    """Expose deterministic accessibility state membership."""

    states: set[str] = field(default_factory=set)

    def contains(self, state: object) -> bool:
        """Return whether the fake node owns this state."""
        return state in self.states


@dataclass(slots=True)
class FakeComponent:
    """Record focus restoration without touching the real desktop."""

    focus_succeeds: bool = True
    calls: list[str] = field(default_factory=list)

    def grab_focus(self) -> bool:
        """Record and return the configured focus result."""
        self.calls.append("focus")
        return self.focus_succeeds


@dataclass(slots=True)
class FakeText:
    """Implement the bounded AT-SPI text operations used by target capture."""

    value: str
    selection: FakeRange | None = None
    caret_offset: int = 0
    calls: list[tuple[object, ...]] = field(default_factory=list)

    def get_n_selections(self) -> int:
        """Expose zero or one fake selection."""
        return int(self.selection is not None)

    def get_selection(self, selection_number: int) -> FakeRange:
        """Return the only configured selection."""
        assert selection_number == 0
        assert self.selection is not None
        return self.selection

    def get_text(self, start_offset: int, end_offset: int) -> str:
        """Return the exact selected substring."""
        return self.value[start_offset:end_offset]

    def get_caret_offset(self) -> int:
        """Return the configured caret position."""
        return self.caret_offset

    def set_caret_offset(self, new_offset: int) -> bool:
        """Record caret restoration."""
        self.calls.append(("caret", new_offset))
        self.caret_offset = new_offset
        return True

    def add_selection(self, start_offset: int, end_offset: int) -> bool:
        """Record creation of a missing selection."""
        self.calls.append(("add", start_offset, end_offset))
        return True

    def set_selection(self, selection_number: int, start_offset: int, end_offset: int) -> bool:
        """Record restoration over an existing selection."""
        self.calls.append(("set", selection_number, start_offset, end_offset))
        return True

    def delete_text(self, start_position: int, end_position: int) -> bool:
        """Delete one fake character range and move the caret to its start."""
        self.calls.append(("delete", start_position, end_position))
        self.value = self.value[:start_position] + self.value[end_position:]
        self.caret_offset = start_position
        self.selection = None
        return True

    def insert_text(self, position: int, text: str, byte_length: int) -> bool:
        """Insert fake UTF-8 text after checking the libatspi byte-length contract."""
        self.calls.append(("insert", position, text, byte_length))
        assert byte_length == len(text.encode("utf-8"))
        self.value = self.value[:position] + text + self.value[position:]
        self.caret_offset = position + len(text)
        return True


@dataclass(slots=True)
class FakeNode:
    """Build an isolated accessibility tree for one focused text target."""

    process_id: int
    role: str = TEXT
    active: bool = False
    focused: bool = False
    editable: bool = True
    text: FakeText | None = None
    component: FakeComponent | None = None
    children: list[FakeNode] = field(default_factory=list)

    def get_state_set(self) -> FakeStateSet:
        """Expose focus state."""
        states = set()
        if self.focused:
            states.add(FOCUSED)
        if self.active:
            states.add(ACTIVE)
        if self.editable:
            states.add(EDITABLE)
        return FakeStateSet(states)

    def get_child_count(self) -> int:
        """Return the fake child count."""
        return len(self.children)

    def get_child_at_index(self, child_index: int) -> FakeNode:
        """Return one fake child."""
        return self.children[child_index]

    def get_process_id(self) -> int:
        """Return the fake owning process."""
        return self.process_id

    def get_role(self) -> object:
        """Return the fake semantic role."""
        return self.role

    def get_text_iface(self) -> FakeText | None:
        """Return the fake text interface."""
        return self.text

    def get_editable_text_iface(self) -> FakeText | None:
        """Return the fake editable interface only when configured."""
        return self.text if self.editable else None

    def get_component_iface(self) -> FakeComponent | None:
        """Return the fake component interface."""
        return self.component


@dataclass(slots=True)
class FakeEvent:
    """Carry one synthetic AT-SPI focus-state transition."""

    source: FakeNode
    detail1: int


@dataclass(slots=True)
class FakeEventListener:
    """Record event registration and deliver synthetic focus events."""

    callback: object = None
    registrations: list[str] = field(default_factory=list)
    deregistrations: list[str] = field(default_factory=list)

    def register(self, event_type: str) -> bool:
        """Record successful event registration."""
        self.registrations.append(event_type)
        return True

    def deregister(self, event_type: str) -> bool:
        """Record successful event deregistration."""
        self.deregistrations.append(event_type)
        return True

    def emit(self, source: FakeNode, focused: bool = True) -> None:
        """Deliver one event through the registered callback."""
        assert callable(self.callback)
        self.callback(FakeEvent(source, int(focused)), None)


@dataclass(frozen=True, slots=True)
class FakeRuntime:
    """Avoid opening the real accessibility bus during tests."""

    root: FakeNode
    active_state: object = ACTIVE
    editable_state: object = EDITABLE
    focused_state: object = FOCUSED
    password_text_role: object = PASSWORD
    listener: FakeEventListener = field(default_factory=FakeEventListener)

    def desktop(self) -> FakeNode:
        """Return the isolated accessibility root."""
        return self.root

    def application_identifier(self, node: FakeNode) -> str | None:
        """Derive a deterministic fake executable identity without target text."""
        return f"/usr/bin/process-{node.process_id}"

    def text_interface(self, node: FakeNode) -> FakeText | None:
        """Return the fake Text interface without method-name ambiguity."""
        return node.get_text_iface()

    def editable_text_interface(self, node: FakeNode) -> FakeText | None:
        """Return the fake EditableText interface when configured."""
        return node.get_editable_text_iface()

    def create_event_listener(self, callback: object) -> FakeEventListener:
        """Bind the tracker callback to one fake listener."""
        self.listener.callback = callback
        return self.listener


def test_system_accessibility_reads_the_live_atspi_status(monkeypatch: pytest.MonkeyPatch) -> None:
    """Use the session status property rather than assuming AT-SPI init means enabled."""
    calls: list[tuple[object, ...]] = []

    class Response:
        def unpack(self) -> tuple[bool]:
            return (True,)

    class Connection:
        def call_sync(self, *arguments: object) -> Response:
            calls.append(arguments)
            return Response()

    monkeypatch.setattr("voice_scribe_linux.text_target.Gio.bus_get_sync", lambda *_args: Connection())

    assert system_accessibility_enabled()
    assert calls[0][0:4] == (
        "org.a11y.Bus",
        "/org/a11y/bus",
        "org.freedesktop.DBus.Properties",
        "Get",
    )


def test_focus_tracker_rejects_a_disabled_system_accessibility_bus(monkeypatch: pytest.MonkeyPatch) -> None:
    """Keep the application copy-only when GNOME has not enabled AT-SPI clients."""
    monkeypatch.setattr("voice_scribe_linux.text_target.system_accessibility_enabled", lambda: False)

    with pytest.raises(RuntimeError, match="accessibility is disabled"):
        FocusedTextTargetTracker()


def test_capture_and_restore_explicit_selection() -> None:
    """Freeze selected text and later restore the same target range before replacement."""
    target_text = FakeText("prefix selected suffix", FakeRange(7, 15), caret_offset=15)
    component = FakeComponent()
    target = FakeNode(20, focused=True, text=target_text, component=component)
    own_focused = FakeNode(10, focused=True, text=FakeText("Mluva"), component=FakeComponent())
    root = FakeNode(0, children=[own_focused, FakeNode(20, children=[target])])

    snapshot = capture_focused_text_target(FakeRuntime(root), own_process_id=10)

    assert snapshot is not None
    assert snapshot.selected_text == "selected"
    assert snapshot.application_identifier == "/usr/bin/process-20"
    assert snapshot.has_selection
    target.focused = False
    assert snapshot.restore()
    assert component.calls == ["focus"]
    assert target_text.calls == [("set", 0, 7, 15)]


def test_delivery_only_target_discards_selected_content_but_preserves_exact_offsets() -> None:
    """Keep later explicit paste possible without retaining Command source text in memory."""
    target_text = FakeText("prefix selected suffix", FakeRange(7, 15), caret_offset=15)
    target = FakeNode(20, focused=True, text=target_text, component=FakeComponent())
    snapshot = capture_focused_text_target(FakeRuntime(FakeNode(0, children=[target])), own_process_id=10)

    assert snapshot is not None
    delivery_target = snapshot.without_selected_text()

    assert delivery_target.selected_text is None
    assert delivery_target.selection_start == 7
    assert delivery_target.selection_end == 15
    assert delivery_target.caret_offset == snapshot.caret_offset
    assert delivery_target.application_identifier == snapshot.application_identifier
    assert delivery_target.accessible is snapshot.accessible


def test_capture_without_selection_restores_original_caret() -> None:
    """Treat a command without selected text as an insertion at the captured caret."""
    target_text = FakeText("draft here", caret_offset=5)
    target = FakeNode(20, focused=True, text=target_text, component=FakeComponent())
    snapshot = capture_focused_text_target(FakeRuntime(FakeNode(0, children=[target])), own_process_id=10)

    assert snapshot is not None
    assert snapshot.selected_text is None
    assert not snapshot.has_selection
    assert snapshot.restore()
    assert target_text.calls == [("caret", 5)]


def test_delivery_target_captures_selection_offsets_without_reading_text() -> None:
    """Restore ordinary dictation focus while keeping selected content undisclosed."""

    class NoReadText(FakeText):
        """Fail if delivery-only target capture attempts to read content."""

        def get_text(self, _start_offset: int, _end_offset: int) -> str:
            """Reject content disclosure outside Command mode."""
            raise AssertionError("delivery target must not read selected text")

    target_text = NoReadText("private selection", FakeRange(0, 7), caret_offset=7)
    target = FakeNode(20, focused=True, text=target_text, component=FakeComponent())

    snapshot = capture_focused_delivery_target(
        FakeRuntime(FakeNode(0, children=[target])),
        own_process_id=10,
    )

    assert snapshot is not None
    assert snapshot.selected_text is None
    assert snapshot.has_selection
    assert snapshot.restore()
    assert target_text.calls == [("set", 0, 0, 7)]


def test_delivery_confirmation_uses_only_expected_caret_position() -> None:
    """Confirm one insertion without reading the inserted or surrounding target text."""
    target_text = FakeText("content must remain unread", caret_offset=4)
    target = FakeNode(20, focused=True, text=target_text, component=FakeComponent())
    snapshot = capture_focused_delivery_target(
        FakeRuntime(FakeNode(0, children=[target])),
        own_process_id=10,
    )
    assert snapshot is not None

    target_text.caret_offset = 4 + len("inserted")

    assert snapshot.confirm_insertion("inserted") is True
    assert target_text.calls == []


def test_native_insertion_replaces_selection_and_uses_utf8_byte_length() -> None:
    """Mutate one editable range directly and leave the caret after its UTF-8 text."""
    target_text = FakeText("prefix selected suffix", FakeRange(7, 15), caret_offset=15)
    target = FakeNode(20, focused=True, text=target_text, component=FakeComponent())
    snapshot = capture_focused_delivery_target(
        FakeRuntime(FakeNode(0, children=[target])),
        own_process_id=10,
    )
    assert snapshot is not None

    assert snapshot.restore()
    assert snapshot.insert_text("Příliš") is True
    assert target_text.value == "prefix Příliš suffix"
    assert target_text.calls == [
        ("set", 0, 7, 15),
        ("delete", 7, 15),
        ("insert", 7, "Příliš", len("Příliš".encode())),
        ("caret", 13),
    ]


def test_native_insertion_does_not_require_the_toolkit_to_advance_its_caret() -> None:
    """Trust libatspi's edit result and move a GTK-style stationary caret separately."""

    class StationaryCaretText(FakeText):
        """Insert text while preserving the pre-edit caret, as GTK does over AT-SPI."""

        def insert_text(self, position: int, text: str, byte_length: int) -> bool:
            """Report a successful mutation without advancing the exposed caret."""
            self.calls.append(("insert", position, text, byte_length))
            assert byte_length == len(text.encode("utf-8"))
            self.value = self.value[:position] + text + self.value[position:]
            return True

    target_text = StationaryCaretText("Mluva: ", caret_offset=7)
    target = FakeNode(20, focused=True, text=target_text, component=FakeComponent())
    snapshot = capture_focused_delivery_target(
        FakeRuntime(FakeNode(0, children=[target])),
        own_process_id=10,
    )
    assert snapshot is not None

    assert snapshot.insert_text("Příliš") is True
    assert target_text.value == "Mluva: Příliš"
    assert target_text.caret_offset == 13
    assert target_text.calls == [
        ("insert", 7, "Příliš", len("Příliš".encode())),
        ("caret", 13),
    ]


def test_native_insertion_declines_noneditable_target_without_mutation() -> None:
    """Authorize the keyboard fallback only before any native mutation was attempted."""
    target_text = FakeText("read only", caret_offset=4)
    target = FakeNode(20, focused=True, editable=False, text=target_text, component=FakeComponent())
    snapshot = capture_focused_delivery_target(
        FakeRuntime(FakeNode(0, children=[target])),
        own_process_id=10,
    )
    assert snapshot is not None

    assert snapshot.insert_text("candidate") is None
    assert target_text.calls == []


def test_focused_text_without_selection_or_valid_caret_is_not_a_delivery_target() -> None:
    """Reject links and document proxies that expose Text but no insertion position."""
    target = FakeNode(20, focused=True, editable=False, text=FakeText("link", caret_offset=-1))

    assert (
        capture_focused_delivery_target(
            FakeRuntime(FakeNode(0, children=[target])),
            own_process_id=10,
        )
        is None
    )


def test_active_window_text_wins_over_stale_focused_text() -> None:
    """Ignore toolkit focus residue from inactive applications on GNOME."""
    stale = FakeNode(20, focused=True, text=FakeText("stale", caret_offset=5))
    stale_application = FakeNode(20, children=[FakeNode(20, children=[stale])])
    current = FakeNode(30, focused=True, text=FakeText("current", caret_offset=7))
    current_application = FakeNode(30, children=[FakeNode(30, active=True, children=[current])])

    snapshot = capture_focused_delivery_target(
        FakeRuntime(FakeNode(0, children=[stale_application, current_application])),
        own_process_id=10,
    )

    assert snapshot is not None
    assert snapshot.accessible is current
    assert snapshot.application_identifier == "/usr/bin/process-30"


def test_active_password_target_blocks_stale_nonpassword_focus() -> None:
    """Fail closed when the active text target is a password even if an inactive editor remains focused."""
    stale = FakeNode(20, focused=True, text=FakeText("stale", caret_offset=5))
    stale_application = FakeNode(20, children=[FakeNode(20, children=[stale])])
    password = FakeNode(30, role=PASSWORD, focused=True, text=FakeText("synthetic", caret_offset=9))
    current_application = FakeNode(30, children=[FakeNode(30, active=True, children=[password])])
    root = FakeNode(0, children=[stale_application, current_application])

    assert capture_focused_delivery_target(FakeRuntime(root), own_process_id=10) is None
    assert capture_focused_application_identifier(FakeRuntime(root), own_process_id=10) is None


def test_ambiguous_per_application_focus_states_fail_closed() -> None:
    """Never guess between two applications that both retain active and focused state."""
    first = FakeNode(20, focused=True, text=FakeText("first", caret_offset=5))
    second = FakeNode(30, focused=True, text=FakeText("second", caret_offset=6))
    root = FakeNode(
        0,
        children=[
            FakeNode(20, children=[FakeNode(20, active=True, children=[first])]),
            FakeNode(30, children=[FakeNode(30, active=True, children=[second])]),
        ],
    )

    assert capture_focused_delivery_target(FakeRuntime(root), own_process_id=10) is None


def test_focus_tracker_uses_latest_event_instead_of_ambiguous_tree_state() -> None:
    """Capture the latest global focus event and clear it on nontext, self, or focus loss."""
    first = FakeNode(20, focused=True, text=FakeText("first", caret_offset=5))
    second = FakeNode(30, focused=True, text=FakeText("second", caret_offset=6))
    root = FakeNode(
        0,
        children=[
            FakeNode(20, children=[FakeNode(20, active=True, children=[first])]),
            FakeNode(30, children=[FakeNode(30, active=True, children=[second])]),
        ],
    )
    runtime = FakeRuntime(root)
    tracker = FocusedTextTargetTracker(runtime, own_process_id=10)
    assert runtime.listener.registrations == ["object:state-changed:focused"]

    runtime.listener.emit(second)
    snapshot = tracker.capture_delivery_target()
    assert snapshot is not None
    assert snapshot.accessible is second
    assert tracker.capture_application_identifier() == "/usr/bin/process-30"

    runtime.listener.emit(FakeNode(30, focused=True))
    assert tracker.capture_delivery_target() is None
    assert tracker.capture_application_identifier() == "/usr/bin/process-30"

    runtime.listener.emit(first)
    runtime.listener.emit(first, focused=False)
    assert tracker.capture_delivery_target() is None

    runtime.listener.emit(FakeNode(10, focused=True, text=FakeText("self", caret_offset=4)))
    assert tracker.capture_delivery_target() is None
    assert tracker.capture_application_identifier() is None

    tracker.close()
    assert runtime.listener.deregistrations == ["object:state-changed:focused"]


def test_focus_tracker_clears_focus_loss_from_an_equivalent_proxy() -> None:
    """Clear retained focus when AT-SPI re-wraps the same accessible for the loss event."""
    original = FakeNode(20, focused=True, text=FakeText("synthetic", caret_offset=3))
    equivalent_proxy = FakeNode(20, focused=True, text=original.text)
    runtime = FakeRuntime(FakeNode(0))
    tracker = FocusedTextTargetTracker(runtime=runtime, own_process_id=10)

    runtime.listener.emit(original)
    runtime.listener.emit(equivalent_proxy, focused=False)

    assert tracker.capture_delivery_target() is None


def test_focus_tracker_fails_closed_when_focus_loss_source_is_inaccessible() -> None:
    """Discard a retained target when a focus-loss event cannot identify its source."""

    class InaccessibleFocusLoss:
        """Raise while resolving the event source."""

        detail1 = 0

        @property
        def source(self) -> FakeNode:
            """Represent a defunct AT-SPI proxy."""
            raise RuntimeError("defunct synthetic proxy")

    runtime = FakeRuntime(FakeNode(0))
    tracker = FocusedTextTargetTracker(runtime=runtime, own_process_id=10)
    runtime.listener.emit(FakeNode(20, focused=True, text=FakeText("synthetic", caret_offset=3)))

    tracker._focus_changed(InaccessibleFocusLoss())

    assert tracker.capture_delivery_target() is None


def test_focus_tracker_excludes_password_event_source() -> None:
    """Apply the same password boundary to authoritative event-tracked focus."""
    runtime = FakeRuntime(FakeNode(0))
    tracker = FocusedTextTargetTracker(runtime, own_process_id=10)
    password = FakeNode(20, role=PASSWORD, focused=True, text=FakeText("synthetic", caret_offset=9))

    runtime.listener.emit(password)

    assert tracker.capture_delivery_target() is None
    assert tracker.capture_text_target() is None
    assert tracker.capture_application_identifier() is None


def test_noneditable_tracker_target_requires_same_latest_focus_before_keyboard_fallback() -> None:
    """Refuse a terminal-style fallback after focus moves to another accessible object."""
    target_text = FakeText("synthetic", caret_offset=3)
    target = FakeNode(20, focused=True, editable=False, text=target_text, component=FakeComponent())
    runtime = FakeRuntime(FakeNode(0))
    tracker = FocusedTextTargetTracker(runtime=runtime, own_process_id=10)
    runtime.listener.emit(target)
    snapshot = tracker.capture_delivery_target()

    assert snapshot is not None
    assert snapshot.editable_text is None
    assert snapshot.restore()
    assert target_text.calls == [("caret", 3)]

    runtime.listener.emit(FakeNode(30, focused=True, editable=False, text=FakeText("other", caret_offset=2)))

    assert not snapshot.restore()
    assert target_text.calls == [("caret", 3)]


def test_password_target_is_never_captured() -> None:
    """Exclude password text even when the accessibility tree exposes an interface."""
    password = FakeNode(
        20,
        role=PASSWORD,
        focused=True,
        text=FakeText("should-not-leave-field", FakeRange(0, 22)),
        component=FakeComponent(),
    )
    assert capture_focused_text_target(FakeRuntime(FakeNode(0, children=[password])), own_process_id=10) is None
    assert (
        capture_focused_application_identifier(
            FakeRuntime(FakeNode(0, children=[password])),
            own_process_id=10,
        )
        is None
    )


def test_application_identity_does_not_read_focused_text() -> None:
    """Resolve per-application settings without disclosing selected or nearby text."""
    target_text = FakeText("private selected content", FakeRange(0, 24))
    target = FakeNode(20, focused=True, text=target_text, component=FakeComponent())

    identifier = capture_focused_application_identifier(
        FakeRuntime(FakeNode(0, children=[target])),
        own_process_id=10,
    )

    assert identifier == "/usr/bin/process-20"
    assert target_text.calls == []


def test_oversized_selection_fails_closed() -> None:
    """Do not truncate context and then replace a larger selection with a partial edit."""
    target = FakeNode(
        20,
        focused=True,
        text=FakeText("abcdef", FakeRange(0, 6)),
        component=FakeComponent(),
    )
    with pytest.raises(TextSelectionTooLargeError, match="6 characters"):
        capture_focused_text_target(
            FakeRuntime(FakeNode(0, children=[target])),
            own_process_id=10,
            maximum_selected_characters=5,
        )


def test_restore_failure_never_mutates_selection() -> None:
    """Require successful focus restoration before changing the live selection."""
    target_text = FakeText("selected", FakeRange(0, 8))
    target = FakeNode(
        20,
        focused=True,
        text=target_text,
        component=FakeComponent(focus_succeeds=False),
    )
    snapshot = capture_focused_text_target(FakeRuntime(FakeNode(0, children=[target])), own_process_id=10)

    assert snapshot is not None
    target.focused = False
    assert not snapshot.restore()
    assert target_text.calls == []
