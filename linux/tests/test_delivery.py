"""Headless subprocess-boundary coverage for clipboard and exact-once paste receipts."""

import os
import socket
import subprocess
from pathlib import Path

import pytest

from mluva_linux.delivery import (
    DeliveryError,
    _paste_command,
    _ydotool_socket_ready,
    deliver_text,
    keyboard_paste_available,
)


def install_fake_desktop(
    monkeypatch: pytest.MonkeyPatch,
    executables: dict[str, str | None],
) -> list[tuple[list[str], str | None]]:
    """Replace executable resolution, process dispatch, and delay without touching the desktop."""
    calls: list[tuple[list[str], str | None]] = []
    monkeypatch.setattr("mluva_linux.delivery.shutil.which", executables.get)
    monkeypatch.setattr(
        "mluva_linux.delivery._ydotool_socket_ready",
        lambda _environment: executables.get("ydotool") is not None,
    )

    def fake_run(
        command: list[str],
        input: str | None = None,
        text: bool = False,
        check: bool = False,
    ) -> subprocess.CompletedProcess[str]:
        assert check
        calls.append((command, input))
        return subprocess.CompletedProcess(command, 0, "", "")

    monkeypatch.setattr("mluva_linux.delivery.subprocess.run", fake_run)
    monkeypatch.setattr("mluva_linux.delivery.time.sleep", lambda _seconds: None)
    return calls


def test_copy_only_uses_wayland_clipboard_without_input_injection(monkeypatch: pytest.MonkeyPatch) -> None:
    """Keep button capture and explicit copy actions free of synthetic key events."""
    calls = install_fake_desktop(monkeypatch, {"wl-copy": "/usr/bin/wl-copy"})

    receipt = deliver_text("Hello Linux", auto_paste=False)

    assert calls == [(["/usr/bin/wl-copy"], "Hello Linux")]
    assert receipt.copied
    assert not receipt.pasted
    assert not receipt.paste_dispatched
    assert receipt.paste_confirmed is None
    assert receipt.history_outcome == "copied"


def test_confirmed_native_insertion_precedes_keyboard_fallback(monkeypatch: pytest.MonkeyPatch) -> None:
    """Use the restored target's editable interface after installing clipboard recovery."""
    calls = install_fake_desktop(
        monkeypatch,
        {
            "wl-copy": "/usr/bin/wl-copy",
            "wtype": "/usr/bin/wtype",
        },
    )
    insertions: list[str] = []

    receipt = deliver_text(
        "Příliš žluťoučký",
        auto_paste=True,
        insert_directly=lambda text: insertions.append(text) is None,
    )

    assert calls == [(["/usr/bin/wl-copy"], "Příliš žluťoučký")]
    assert insertions == ["Příliš žluťoučký"]
    assert receipt.pasted
    assert receipt.paste_dispatched
    assert receipt.paste_confirmed is True
    assert receipt.history_outcome == "pasted"
    assert "accessibility editing interface" in receipt.guidance


@pytest.mark.parametrize(
    ("application_identifier", "chord"),
    [
        ("/usr/bin/foot", ["-M", "shift", "-k", "Insert", "-m", "shift"]),
        ("/usr/bin/alacritty", ["-M", "shift", "-k", "Insert", "-m", "shift"]),
        ("/opt/ghostty/bin/ghostty", ["-M", "shift", "-k", "Insert", "-m", "shift"]),
        ("process:foot", ["-M", "shift", "-k", "Insert", "-m", "shift"]),
        ("/usr/bin/code", ["-M", "ctrl", "v", "-m", "ctrl"]),
        ("Terminal · foot", ["-M", "ctrl", "v", "-m", "ctrl"]),
        ("/usr/bin/not-foot", ["-M", "ctrl", "v", "-m", "ctrl"]),
        (None, ["-M", "ctrl", "v", "-m", "ctrl"]),
    ],
)
def test_unsupported_native_insertion_uses_captured_process_paste_chord(
    monkeypatch: pytest.MonkeyPatch,
    application_identifier: str | None,
    chord: list[str],
) -> None:
    """Paste once with terminal semantics only for a known captured executable, never a title substring."""
    calls = install_fake_desktop(
        monkeypatch,
        {
            "wl-copy": "/usr/bin/wl-copy",
            "wtype": "/usr/bin/wtype",
        },
    )

    receipt = deliver_text(
        "Keyboard fallback",
        auto_paste=True,
        insert_directly=lambda _text: None,
        confirm_paste=lambda: True,
        application_identifier=application_identifier,
    )

    assert calls == [
        (["/usr/bin/wl-copy"], "Keyboard fallback"),
        (["/usr/bin/wtype", *chord], None),
    ]
    assert receipt.pasted
    assert receipt.paste_confirmed is True


def test_changed_target_prevents_keyboard_fallback_after_clipboard_recovery(monkeypatch: pytest.MonkeyPatch) -> None:
    """Revalidate focus immediately before input injection and remain copy-only when it changed."""
    calls = install_fake_desktop(
        monkeypatch,
        {
            "wl-copy": "/usr/bin/wl-copy",
            "wtype": "/usr/bin/wtype",
        },
    )

    receipt = deliver_text(
        "Do not redirect",
        auto_paste=True,
        insert_directly=lambda _text: None,
        authorize_keyboard_paste=lambda: False,
        application_identifier="/usr/bin/foot",
    )

    assert calls == [(["/usr/bin/wl-copy"], "Do not redirect")]
    assert receipt.copied
    assert not receipt.pasted
    assert not receipt.paste_dispatched
    assert receipt.paste_confirmed is None
    assert receipt.history_outcome == "copied"
    assert "revalidated" in receipt.guidance


def test_uncertain_native_insertion_never_dispatches_keyboard_fallback(monkeypatch: pytest.MonkeyPatch) -> None:
    """Prevent duplicates after an editable target accepted an attempted mutation path."""
    calls = install_fake_desktop(
        monkeypatch,
        {
            "wl-copy": "/usr/bin/wl-copy",
            "ydotool": "/usr/bin/ydotool",
        },
    )

    receipt = deliver_text(
        "Inspect once",
        auto_paste=True,
        insert_directly=lambda _text: False,
        confirm_paste=lambda: True,
    )

    assert calls == [(["/usr/bin/wl-copy"], "Inspect once")]
    assert not receipt.pasted
    assert receipt.paste_dispatched
    assert receipt.paste_confirmed is False
    assert receipt.history_outcome == "paste-unconfirmed"
    assert "before any manual retry" in receipt.guidance


def test_confirmed_paste_dispatches_exactly_once(monkeypatch: pytest.MonkeyPatch) -> None:
    """Poll the captured target but never repeat the synthetic paste command."""
    calls = install_fake_desktop(
        monkeypatch,
        {
            "wl-copy": "/usr/bin/wl-copy",
            "wtype": "/usr/bin/wtype",
        },
    )
    confirmations = iter((False, True))

    receipt = deliver_text("Confirmed", auto_paste=True, confirm_paste=lambda: next(confirmations))

    assert calls == [
        (["/usr/bin/wl-copy"], "Confirmed"),
        (["/usr/bin/wtype", "-M", "ctrl", "v", "-m", "ctrl"], None),
    ]
    assert receipt.pasted
    assert receipt.paste_dispatched
    assert receipt.paste_confirmed is True
    assert receipt.history_outcome == "pasted"


def test_unconfirmed_dispatch_remains_recoverable_without_automatic_retry(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Record uncertainty after one key dispatch while leaving complete text on the clipboard."""
    calls = install_fake_desktop(
        monkeypatch,
        {
            "wl-copy": "/usr/bin/wl-copy",
            "ydotool": "/usr/bin/ydotool",
        },
    )

    receipt = deliver_text(
        "Unconfirmed",
        auto_paste=True,
        confirm_paste=lambda: False,
        confirmation_timeout_seconds=0,
        application_identifier="/usr/bin/foot",
    )

    assert calls == [
        (["/usr/bin/wl-copy"], "Unconfirmed"),
        (["/usr/bin/ydotool", "key", "42:1", "110:1", "110:0", "42:0"], None),
    ]
    assert not receipt.pasted
    assert receipt.paste_dispatched
    assert receipt.paste_confirmed is False
    assert receipt.history_outcome == "paste-unconfirmed"
    assert "sent once" in receipt.guidance
    assert "before any manual retry" in receipt.guidance


def test_confirmation_failure_does_not_turn_dispatched_paste_into_workflow_error(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Treat inaccessible post-paste state as uncertainty rather than authorizing a duplicate."""
    monkeypatch.setenv("XDG_SESSION_TYPE", "x11")
    calls = install_fake_desktop(
        monkeypatch,
        {
            "xclip": "/usr/bin/xclip",
            "xdotool": "/usr/bin/xdotool",
        },
    )

    def fail_confirmation() -> bool:
        raise RuntimeError("synthetic inaccessible target")

    receipt = deliver_text("Fallback", auto_paste=True, confirm_paste=fail_confirmation)

    assert len(calls) == 2
    assert calls[0] == (["/usr/bin/xclip", "-selection", "clipboard"], "Fallback")
    assert receipt.paste_confirmed is None
    assert receipt.history_outcome == "paste-unconfirmed"


def test_input_injector_failure_is_an_uncertain_single_dispatch_not_a_lost_copy(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Preserve the clipboard recovery path when a compositor rejects the injector."""
    calls = install_fake_desktop(
        monkeypatch,
        {
            "wl-copy": "/usr/bin/wl-copy",
            "wtype": "/usr/bin/wtype",
        },
    )

    def fail_after_dispatch(
        command: list[str],
        input: str | None = None,
        text: bool = False,
        check: bool = False,
    ) -> subprocess.CompletedProcess[str]:
        assert check
        calls.append((command, input))
        if command[0].endswith("wtype"):
            raise subprocess.CalledProcessError(1, command)
        return subprocess.CompletedProcess(command, 0, "", "")

    monkeypatch.setattr("mluva_linux.delivery.subprocess.run", fail_after_dispatch)

    receipt = deliver_text("Still recoverable", auto_paste=True, confirm_paste=lambda: True)

    assert calls == [
        (["/usr/bin/wl-copy"], "Still recoverable"),
        (["/usr/bin/wtype", "-M", "ctrl", "v", "-m", "ctrl"], None),
    ]
    assert receipt.copied
    assert not receipt.pasted
    assert receipt.paste_dispatched
    assert receipt.paste_confirmed is None
    assert receipt.history_outcome == "paste-unconfirmed"
    assert "attempted once" in receipt.guidance


def test_missing_clipboard_tool_fails_before_any_delivery(monkeypatch: pytest.MonkeyPatch) -> None:
    """Fail closed when neither supported clipboard boundary is installed."""
    calls = install_fake_desktop(monkeypatch, {})

    with pytest.raises(DeliveryError, match="wl-clipboard"):
        deliver_text("No clipboard", auto_paste=True)

    assert calls == []


def test_missing_input_injector_stays_copy_only(monkeypatch: pytest.MonkeyPatch) -> None:
    """Do not claim insertion when the desktop exposes only a clipboard tool."""
    calls = install_fake_desktop(monkeypatch, {"wl-copy": "/usr/bin/wl-copy"})

    receipt = deliver_text("Manual paste", auto_paste=True)

    assert calls == [(["/usr/bin/wl-copy"], "Manual paste")]
    assert receipt.history_outcome == "copied"
    assert "paste manually" in receipt.guidance


def test_installed_ydotool_without_a_live_daemon_stays_copy_only(monkeypatch: pytest.MonkeyPatch) -> None:
    """Do not select the mandatory-daemon client from executable presence alone."""
    calls = install_fake_desktop(
        monkeypatch,
        {
            "wl-copy": "/usr/bin/wl-copy",
            "ydotool": "/usr/bin/ydotool",
        },
    )
    monkeypatch.setattr("mluva_linux.delivery._ydotool_socket_ready", lambda _environment: False)

    receipt = deliver_text("Manual recovery", auto_paste=True, application_identifier="/usr/bin/foot")

    assert calls == [(["/usr/bin/wl-copy"], "Manual recovery")]
    assert receipt.history_outcome == "copied"
    assert "helper is ready" in receipt.guidance


def test_ydotool_only_keeps_ordinary_targets_copy_only(monkeypatch: pytest.MonkeyPatch) -> None:
    """Keep ordinary targets copy-only when the only injector exposes physical keycodes."""
    calls = install_fake_desktop(
        monkeypatch,
        {"wl-copy": "/usr/bin/wl-copy", "ydotool": "/usr/bin/ydotool"},
    )

    assert not keyboard_paste_available(application_identifier="/usr/bin/code")
    assert keyboard_paste_available(application_identifier="/usr/bin/foot")
    receipt = deliver_text("Keep the complete text", auto_paste=True, application_identifier="/usr/bin/code")

    assert calls == [(["/usr/bin/wl-copy"], "Keep the complete text")]
    assert receipt.history_outcome == "copied"
    assert not receipt.paste_dispatched
    assert "layout-safe" in receipt.guidance


@pytest.mark.parametrize(
    ("application_identifier", "chord"), [("/usr/bin/code", "ctrl+v"), ("/usr/bin/foot", "shift+Insert")]
)
def test_x11_prefers_symbolic_paste_over_ydotool_physical_keys(
    monkeypatch: pytest.MonkeyPatch, application_identifier: str, chord: str
) -> None:
    """Resolve paste through X11 keysyms even when a numeric injector is also ready."""
    monkeypatch.setenv("XDG_SESSION_TYPE", "x11")
    calls = install_fake_desktop(
        monkeypatch,
        {"xclip": "/usr/bin/xclip", "xdotool": "/usr/bin/xdotool", "ydotool": "/usr/bin/ydotool"},
    )

    receipt = deliver_text("One paste", auto_paste=True, application_identifier=application_identifier)

    assert calls == [
        (["/usr/bin/xclip", "-selection", "clipboard"], "One paste"),
        (["/usr/bin/xdotool", "key", "--clearmodifiers", chord], None),
    ]
    assert receipt.paste_dispatched


def test_ydotool_requires_a_live_owner_only_socket(tmp_path: Path) -> None:
    """Reject permissive and stale sockets without sending a keyboard event."""
    socket_path = tmp_path / "ydotool.sock"
    listener = socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM)
    listener.bind(str(socket_path))
    os.chmod(socket_path, 0o600)

    assert _ydotool_socket_ready({"YDOTOOL_SOCKET": str(socket_path)})

    os.chmod(socket_path, 0o660)
    assert not _ydotool_socket_ready({"YDOTOOL_SOCKET": str(socket_path)})

    os.chmod(socket_path, 0o600)
    listener.close()
    assert not _ydotool_socket_ready({"YDOTOOL_SOCKET": str(socket_path)})


def test_xdotool_is_selected_only_for_an_x11_session(monkeypatch: pytest.MonkeyPatch) -> None:
    """Do not mistake an XWayland DISPLAY for a working Wayland-wide paste backend."""
    monkeypatch.setattr(
        "mluva_linux.delivery.shutil.which",
        {"xdotool": "/usr/bin/xdotool"}.get,
    )

    assert _paste_command({"XDG_SESSION_TYPE": "wayland", "DISPLAY": ":0"}) is None
    assert _paste_command({"XDG_SESSION_TYPE": "x11", "DISPLAY": ":0"}) == [
        "/usr/bin/xdotool",
        "key",
        "--clearmodifiers",
        "ctrl+v",
    ]
    assert _paste_command({"DISPLAY": ":0", "WAYLAND_DISPLAY": "wayland-0"}) is None
    assert _paste_command({"DISPLAY": ":0"}) == [
        "/usr/bin/xdotool",
        "key",
        "--clearmodifiers",
        "ctrl+v",
    ]


def test_empty_text_and_negative_timeout_are_rejected_before_subprocess(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Reject programmer errors without changing clipboard ownership."""
    calls = install_fake_desktop(monkeypatch, {"wl-copy": "/usr/bin/wl-copy"})

    with pytest.raises(DeliveryError, match="empty"):
        deliver_text("", auto_paste=False)
    with pytest.raises(ValueError, match="cannot be negative"):
        deliver_text("text", auto_paste=False, confirmation_timeout_seconds=-1)

    assert calls == []
