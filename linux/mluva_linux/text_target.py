"""Capture and restore one explicit Linux text selection through AT-SPI."""

from __future__ import annotations

import os
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path
from threading import Lock
from typing import Protocol

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi, Gio, GLib  # noqa: E402

MAX_SELECTED_TEXT_CHARACTERS = 2_000
MAX_ACCESSIBLE_NODES = 20_000


def system_accessibility_enabled() -> bool:
    """Read GNOME's live AT-SPI status without starting a disabled service."""
    try:
        connection = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        response = connection.call_sync(
            "org.a11y.Bus",
            "/org/a11y/bus",
            "org.freedesktop.DBus.Properties",
            "Get",
            GLib.Variant("(ss)", ("org.a11y.Status", "IsEnabled")),
            GLib.VariantType.new("(v)"),
            Gio.DBusCallFlags.NO_AUTO_START,
            1_000,
            None,
        )
        enabled = response.unpack()[0]
        return bool(enabled.unpack() if isinstance(enabled, GLib.Variant) else enabled)
    except Exception:
        return False


class TextSelectionTooLargeError(RuntimeError):
    """Reject a selection that cannot be disclosed to the command provider safely."""


class TextRange(Protocol):
    """Describe the selection offsets returned by the AT-SPI text interface."""

    start_offset: int
    end_offset: int


class AccessibleStateSet(Protocol):
    """Describe the state lookup needed while finding the focused object."""

    def contains(self, state: object) -> bool:
        """Return whether this accessible object exposes the requested state."""
        ...


class AccessibleText(Protocol):
    """Describe the bounded text and selection operations used by Mluva."""

    def get_n_selections(self) -> int:
        """Return the number of active text selections."""
        ...

    def get_selection(self, selection_number: int) -> TextRange:
        """Return one active selection range."""
        ...

    def get_text(self, start_offset: int, end_offset: int) -> str:
        """Return text within the explicit character range."""
        ...

    def get_caret_offset(self) -> int:
        """Return the current insertion position when no text is selected."""
        ...

    def set_caret_offset(self, new_offset: int) -> bool:
        """Restore the captured insertion position."""
        ...

    def add_selection(self, start_offset: int, end_offset: int) -> bool:
        """Create the captured selection when the application no longer exposes one."""
        ...

    def set_selection(self, selection_number: int, start_offset: int, end_offset: int) -> bool:
        """Restore the captured selection over an existing selection slot."""
        ...


class AccessibleEditableText(Protocol):
    """Describe the mutation operations exposed by an editable AT-SPI text target."""

    def delete_text(self, start_position: int, end_position: int) -> bool:
        """Delete one explicit selected range before replacement."""
        ...

    def insert_text(self, position: int, text: str, byte_length: int) -> bool:
        """Insert UTF-8 text at one character offset."""
        ...


class AccessibleComponent(Protocol):
    """Describe the AT-SPI focus operation needed before replacement."""

    def grab_focus(self) -> bool:
        """Restore keyboard focus to the captured accessible element."""
        ...


class AccessibilityEvent(Protocol):
    """Describe the AT-SPI focus event fields consumed by the tracker."""

    source: AccessibleNode
    detail1: int


class AccessibilityEventListener(Protocol):
    """Describe registration lifecycle for one AT-SPI event callback."""

    def register(self, event_type: str) -> bool:
        """Register one event family."""
        ...

    def deregister(self, event_type: str) -> bool:
        """Deregister one event family."""
        ...


class AccessibleNode(Protocol):
    """Describe the small AT-SPI accessible surface used by target capture."""

    def get_state_set(self) -> AccessibleStateSet:
        """Return the node's accessibility state set."""
        ...

    def get_child_count(self) -> int:
        """Return the number of accessible children."""
        ...

    def get_child_at_index(self, child_index: int) -> AccessibleNode | None:
        """Return one accessible child."""
        ...

    def get_process_id(self) -> int:
        """Return the process that owns this accessibility node."""
        ...

    def get_role(self) -> object:
        """Return the semantic role used to exclude password fields."""
        ...

    def get_text_iface(self) -> AccessibleText | None:
        """Return the selectable text interface when implemented."""
        ...

    def get_editable_text_iface(self) -> AccessibleEditableText | None:
        """Return the mutation interface only when the target is editable."""
        ...

    def get_component_iface(self) -> AccessibleComponent | None:
        """Return the focusable component interface when implemented."""
        ...


class AtspiRuntime(Protocol):
    """Supply desktop and enum values without coupling tests to a live desktop."""

    active_state: object
    editable_state: object
    focused_state: object
    password_text_role: object

    def desktop(self) -> AccessibleNode:
        """Return the root of the current AT-SPI desktop."""
        ...

    def application_identifier(self, node: AccessibleNode) -> str | None:
        """Return one stable local identifier without reading target text."""
        ...

    def text_interface(self, node: AccessibleNode) -> AccessibleText | None:
        """Return an unambiguous text-interface adapter for one node."""
        ...

    def editable_text_interface(self, node: AccessibleNode) -> AccessibleEditableText | None:
        """Return an unambiguous editable-text adapter when the node supports mutation."""
        ...

    def create_event_listener(
        self,
        callback: Callable[[AccessibilityEvent, object | None], None],
    ) -> AccessibilityEventListener:
        """Create an AT-SPI listener for authoritative cross-application focus events."""
        ...


@dataclass(frozen=True, slots=True)
class SystemAccessibleText:
    """Disambiguate AT-SPI Text calls from same-named Accessible selection methods."""

    accessible: object
    atspi: object

    def get_n_selections(self) -> int:
        """Return the number of text ranges, not selected accessible children."""
        return self.atspi.Text.get_n_selections(self.accessible)

    def get_selection(self, selection_number: int) -> TextRange:
        """Return one text selection through the Text interface explicitly."""
        return self.atspi.Text.get_selection(self.accessible, selection_number)

    def get_text(self, start_offset: int, end_offset: int) -> str:
        """Read only the explicitly requested text range."""
        return self.atspi.Text.get_text(self.accessible, start_offset, end_offset)

    def get_caret_offset(self) -> int:
        """Return the Text-interface caret offset."""
        return self.atspi.Text.get_caret_offset(self.accessible)

    def set_caret_offset(self, new_offset: int) -> bool:
        """Set the Text-interface caret offset."""
        return self.atspi.Text.set_caret_offset(self.accessible, new_offset)

    def add_selection(self, start_offset: int, end_offset: int) -> bool:
        """Add one Text-interface range."""
        return self.atspi.Text.add_selection(self.accessible, start_offset, end_offset)

    def set_selection(self, selection_number: int, start_offset: int, end_offset: int) -> bool:
        """Replace one Text-interface selection range."""
        return self.atspi.Text.set_selection(self.accessible, selection_number, start_offset, end_offset)


@dataclass(frozen=True, slots=True)
class SystemAccessibleEditableText:
    """Dispatch mutations through AT-SPI EditableText without PyGObject method ambiguity."""

    accessible: object
    atspi: object

    def delete_text(self, start_position: int, end_position: int) -> bool:
        """Delete one explicit character range."""
        return self.atspi.EditableText.delete_text(self.accessible, start_position, end_position)

    def insert_text(self, position: int, text: str, byte_length: int) -> bool:
        """Insert UTF-8 bytes at one character position."""
        return self.atspi.EditableText.insert_text(self.accessible, position, text, byte_length)


@dataclass(frozen=True, slots=True)
class SystemAtspiRuntime:
    """Use the distribution-provided libatspi GObject binding."""

    atspi: object
    active_state: object
    editable_state: object
    focused_state: object
    password_text_role: object

    @classmethod
    def load(cls) -> SystemAtspiRuntime:
        """Construct the runtime from the already validated system typelib."""
        Atspi.init()
        return cls(
            atspi=Atspi,
            active_state=Atspi.StateType.ACTIVE,
            editable_state=Atspi.StateType.EDITABLE,
            focused_state=Atspi.StateType.FOCUSED,
            password_text_role=Atspi.Role.PASSWORD_TEXT,
        )

    def desktop(self) -> AccessibleNode:
        """Return the first desktop exposed by the active accessibility bus."""
        return self.atspi.get_desktop(0)

    def application_identifier(self, node: AccessibleNode) -> str | None:
        """Resolve a focused process to a stable, local-only executable identity."""
        process_id = node.get_process_id()
        if process_id <= 0:
            return None
        process_directory = Path("/proc") / str(process_id)
        try:
            executable = os.readlink(process_directory / "exe")
        except OSError:
            try:
                process_name = (process_directory / "comm").read_text(encoding="utf-8").strip()
            except OSError:
                return None
            return f"process:{process_name}" if process_name else None
        deleted_suffix = " (deleted)"
        if executable.endswith(deleted_suffix):
            executable = executable[: -len(deleted_suffix)]
        return str(Path(executable)) if executable else None

    def text_interface(self, node: AccessibleNode) -> AccessibleText | None:
        """Wrap a supported Text interface so overloaded methods remain unambiguous."""
        return SystemAccessibleText(node, self.atspi) if node.get_text_iface() is not None else None

    def editable_text_interface(self, node: AccessibleNode) -> AccessibleEditableText | None:
        """Wrap a supported EditableText interface so mutation calls use the intended API."""
        if not node.get_state_set().contains(self.editable_state) or node.get_editable_text_iface() is None:
            return None
        return SystemAccessibleEditableText(node, self.atspi)

    def create_event_listener(
        self,
        callback: Callable[[AccessibilityEvent, object | None], None],
    ) -> AccessibilityEventListener:
        """Create a listener integrated with the application's GLib main context."""
        return self.atspi.EventListener.new(callback)


class FocusedTextTargetTracker:
    """Track the latest global focus event because AT-SPI states are only per application."""

    event_type = "object:state-changed:focused"

    def __init__(
        self,
        runtime: AtspiRuntime | None = None,
        own_process_id: int | None = None,
    ) -> None:
        """Register one fail-closed focus listener for the lifetime of the application."""
        if runtime is None:
            if not system_accessibility_enabled():
                raise RuntimeError("Desktop accessibility is disabled")
            runtime = SystemAtspiRuntime.load()
        self.runtime = runtime
        self.own_process_id = os.getpid() if own_process_id is None else own_process_id
        self._lock = Lock()
        self._focused: AccessibleNode | None = None
        self._listener: AccessibilityEventListener | None = self.runtime.create_event_listener(self._focus_changed)
        if not self._listener.register(self.event_type):
            self._listener = None
            raise RuntimeError("AT-SPI rejected global focus-event registration")

    def _focus_changed(self, event: AccessibilityEvent, _user_data: object | None = None) -> None:
        """Retain only the newest focused external object and clear on focus loss or self-focus."""
        try:
            source = event.source
            source_process_id = source.get_process_id()
        except Exception:
            source = None
            source_process_id = self.own_process_id
        with self._lock:
            if not event.detail1:
                if source is None or source == self._focused:
                    self._focused = None
                return
            self._focused = source if source_process_id != self.own_process_id else None

    def capture_delivery_target(self) -> TextTargetSnapshot | None:
        """Capture content-free delivery metadata from the latest authoritative focus event."""
        return self._capture(include_selected_text=False, maximum_selected_characters=0)

    def capture_text_target(
        self,
        maximum_selected_characters: int = MAX_SELECTED_TEXT_CHARACTERS,
    ) -> TextTargetSnapshot | None:
        """Capture optional selected text for explicit Command mode only."""
        return self._capture(
            include_selected_text=True,
            maximum_selected_characters=maximum_selected_characters,
        )

    def capture_application_identifier(self) -> str | None:
        """Return the current external process identity without reading its text."""
        focused = self._current()
        if focused is None:
            return None
        try:
            if focused.get_role() == self.runtime.password_text_role:
                return None
            return self.runtime.application_identifier(focused)
        except Exception:
            return None

    def _capture(
        self,
        include_selected_text: bool,
        maximum_selected_characters: int,
    ) -> TextTargetSnapshot | None:
        """Freeze the current event source through the shared disclosure boundary."""
        return _capture_focused_target(
            runtime=self.runtime,
            own_process_id=self.own_process_id,
            include_selected_text=include_selected_text,
            maximum_selected_characters=maximum_selected_characters,
            focused_node=self._current(),
            discover_focused=False,
            is_current_target=self._is_current,
        )

    def _current(self) -> AccessibleNode | None:
        """Read the latest event source atomically."""
        with self._lock:
            return self._focused

    def _is_current(self, target: AccessibleNode) -> bool:
        """Confirm that one keyboard fallback still points at the latest global focus event."""
        with self._lock:
            focused = self._focused
        try:
            return focused is not None and focused == target
        except Exception:
            return False

    def close(self) -> None:
        """Deregister the listener and release its retained accessible proxy."""
        listener = self._listener
        self._listener = None
        if listener is not None:
            try:
                listener.deregister(self.event_type)
            except Exception:
                pass
        with self._lock:
            self._focused = None


@dataclass(frozen=True, slots=True)
class TextTargetSnapshot:
    """Keep an in-memory target, offsets, and optional explicitly disclosed selection."""

    accessible: AccessibleNode
    text: AccessibleText
    editable_text: AccessibleEditableText | None
    focused_state: object
    selected_text: str | None
    selection_start: int | None
    selection_end: int | None
    caret_offset: int
    application_identifier: str | None
    is_current_target: Callable[[AccessibleNode], bool] | None = None

    @property
    def has_selection(self) -> bool:
        """Return whether acceptance should replace an explicit selection."""
        return self.selection_start is not None and self.selection_end is not None

    def without_selected_text(self) -> TextTargetSnapshot:
        """Retain only focus and offset metadata needed by later explicit delivery."""
        return TextTargetSnapshot(
            accessible=self.accessible,
            text=self.text,
            editable_text=self.editable_text,
            focused_state=self.focused_state,
            selected_text=None,
            selection_start=self.selection_start,
            selection_end=self.selection_end,
            caret_offset=self.caret_offset,
            application_identifier=self.application_identifier,
            is_current_target=self.is_current_target,
        )

    def restore(self) -> bool:
        """Restore focus plus the exact captured selection or caret without changing text."""
        try:
            if (
                self.editable_text is None
                and self.is_current_target is not None
                and not self.is_current_target(self.accessible)
            ):
                return False
            already_focused = self.accessible.get_state_set().contains(self.focused_state)
            if not already_focused:
                component = self.accessible.get_component_iface()
                if component is None or not component.grab_focus():
                    return False
            if self.selection_start is None or self.selection_end is None:
                return self.text.set_caret_offset(self.caret_offset)
            if self.text.get_n_selections() > 0:
                return self.text.set_selection(0, self.selection_start, self.selection_end)
            return self.text.add_selection(self.selection_start, self.selection_end)
        except Exception:
            return False

    def confirm_insertion(self, inserted_text: str) -> bool | None:
        """Confirm the expected post-paste caret without reading any target text."""
        try:
            insertion_start = self.selection_start if self.selection_start is not None else self.caret_offset
            return self.text.get_caret_offset() == insertion_start + len(inserted_text)
        except Exception:
            return None

    def insert_text(self, inserted_text: str) -> bool | None:
        """Replace the captured range through AT-SPI and leave the caret after the inserted text."""
        if self.editable_text is None:
            return None
        insertion_start = self.selection_start if self.selection_start is not None else self.caret_offset
        try:
            if self.selection_start is not None and self.selection_end is not None:
                if not self.editable_text.delete_text(self.selection_start, self.selection_end):
                    return False
            if not self.editable_text.insert_text(
                insertion_start,
                inserted_text,
                len(inserted_text.encode("utf-8")),
            ):
                return False
            try:
                self.text.set_caret_offset(insertion_start + len(inserted_text))
            except Exception:
                pass
            return True
        except Exception:
            return False


def capture_focused_text_target(
    runtime: AtspiRuntime | None = None,
    own_process_id: int | None = None,
    maximum_selected_characters: int = MAX_SELECTED_TEXT_CHARACTERS,
) -> TextTargetSnapshot | None:
    """Capture only the focused non-password text object and its explicit selection."""
    return _capture_focused_target(
        runtime=runtime,
        own_process_id=own_process_id,
        include_selected_text=True,
        maximum_selected_characters=maximum_selected_characters,
    )


def capture_focused_delivery_target(
    runtime: AtspiRuntime | None = None,
    own_process_id: int | None = None,
) -> TextTargetSnapshot | None:
    """Capture focus and offsets for delivery without reading selected or nearby text."""
    return _capture_focused_target(
        runtime=runtime,
        own_process_id=own_process_id,
        include_selected_text=False,
        maximum_selected_characters=0,
    )


def _capture_focused_target(
    runtime: AtspiRuntime | None,
    own_process_id: int | None,
    include_selected_text: bool,
    maximum_selected_characters: int,
    focused_node: AccessibleNode | None = None,
    discover_focused: bool = True,
    is_current_target: Callable[[AccessibleNode], bool] | None = None,
) -> TextTargetSnapshot | None:
    """Capture one focused text target under the requested disclosure boundary."""
    runtime = runtime or SystemAtspiRuntime.load()
    own_process_id = os.getpid() if own_process_id is None else own_process_id
    try:
        focused = (
            _find_focused_text_node(runtime, runtime.desktop(), own_process_id) if discover_focused else focused_node
        )
        if focused is not None and focused.get_process_id() == own_process_id:
            return None
        if focused is None or focused.get_role() == runtime.password_text_role:
            return None
        text = runtime.text_interface(focused)
        if text is None:
            return None
        editable_text = runtime.editable_text_interface(focused)
        caret_offset = text.get_caret_offset()
        if text.get_n_selections() <= 0:
            if caret_offset < 0:
                return None
            return TextTargetSnapshot(
                accessible=focused,
                text=text,
                editable_text=editable_text,
                focused_state=runtime.focused_state,
                selected_text=None,
                selection_start=None,
                selection_end=None,
                caret_offset=caret_offset,
                application_identifier=runtime.application_identifier(focused),
                is_current_target=is_current_target,
            )
        selection = text.get_selection(0)
        if selection.start_offset < 0 or selection.end_offset < selection.start_offset:
            return None
        selected_text = None
        if include_selected_text:
            selected_text = text.get_text(selection.start_offset, selection.end_offset)
            if len(selected_text) > maximum_selected_characters:
                raise TextSelectionTooLargeError(
                    f"The selected text is {len(selected_text):,} characters; Command mode allows at most "
                    f"{maximum_selected_characters:,}. Shorten the selection before recording."
                )
        return TextTargetSnapshot(
            accessible=focused,
            text=text,
            editable_text=editable_text,
            focused_state=runtime.focused_state,
            selected_text=selected_text,
            selection_start=selection.start_offset,
            selection_end=selection.end_offset,
            caret_offset=caret_offset,
            application_identifier=runtime.application_identifier(focused),
            is_current_target=is_current_target,
        )
    except TextSelectionTooLargeError:
        raise
    except Exception:
        return None


def capture_focused_application_identifier(
    runtime: AtspiRuntime | None = None,
    own_process_id: int | None = None,
) -> str | None:
    """Capture only a focused non-password application's local process identity."""
    runtime = runtime or SystemAtspiRuntime.load()
    own_process_id = os.getpid() if own_process_id is None else own_process_id
    try:
        focused = _find_focused_text_node(runtime, runtime.desktop(), own_process_id)
        if focused is None or focused.get_role() == runtime.password_text_role:
            return None
        return runtime.application_identifier(focused)
    except Exception:
        return None


def _find_focused_text_node(
    runtime: AtspiRuntime,
    root: AccessibleNode,
    own_process_id: int,
) -> AccessibleNode | None:
    """Prefer focused text inside an active top-level window and ignore stale focus flags."""
    active_roots: list[AccessibleNode] = []
    try:
        application_count = root.get_child_count()
    except Exception:
        application_count = 0
    for application_index in range(application_count):
        try:
            application = root.get_child_at_index(application_index)
            if application is None or application.get_process_id() == own_process_id:
                continue
            top_level_count = application.get_child_count()
        except Exception:
            continue
        for top_level_index in range(top_level_count):
            try:
                top_level = application.get_child_at_index(top_level_index)
                if top_level is not None and top_level.get_state_set().contains(runtime.active_state):
                    active_roots.append(top_level)
            except Exception:
                continue
    if active_roots:
        return _find_focused_text_in_roots(runtime, active_roots, own_process_id)
    return _find_focused_text_in_roots(runtime, [root], own_process_id)


def _find_focused_text_in_roots(
    runtime: AtspiRuntime,
    roots: list[AccessibleNode],
    own_process_id: int,
) -> AccessibleNode | None:
    """Return a focused text node only when the candidate is globally unambiguous."""
    stack = list(reversed(roots))
    visited = 0
    candidates: list[AccessibleNode] = []
    while stack and visited < MAX_ACCESSIBLE_NODES:
        node = stack.pop()
        visited += 1
        try:
            process_id = node.get_process_id()
        except Exception:
            process_id = -1
        if process_id == own_process_id:
            continue
        try:
            if node.get_state_set().contains(runtime.focused_state) and runtime.text_interface(node) is not None:
                candidates.append(node)
        except Exception:
            pass
        try:
            child_count = node.get_child_count()
        except Exception:
            child_count = 0
        for index in range(child_count - 1, -1, -1):
            try:
                child = node.get_child_at_index(index)
            except Exception:
                continue
            if child is not None:
                stack.append(child)
    return candidates[0] if len(candidates) == 1 else None
