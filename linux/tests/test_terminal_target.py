"""Protect terminal paste identity and fail-closed delivery without contacting a live compositor."""

import json
import subprocess

import pytest

from mluva_linux.delivery import deliver_text
from mluva_linux.terminal_target import capture_hyprland_terminal_target
from mluva_linux.text_target import FocusedTextTargetTracker


@pytest.mark.parametrize("change", [None, "address", "pid", "executable", "unmapped", "no-window", "disconnected"])
def test_terminal_delivery_requires_the_same_window_and_process_at_dispatch(
    monkeypatch: pytest.MonkeyPatch, change: str | None
) -> None:
    """Copy once and dispatch only to the exact terminal captured before recognition."""
    window = {"address": "0x1234", "pid": 42, "mapped": True}
    executable = "/usr/bin/foot"
    disconnected = False
    calls: list[tuple[list[str], str | None]] = []
    monkeypatch.setenv("HYPRLAND_INSTANCE_SIGNATURE", "isolated-test")
    monkeypatch.setenv("XDG_SESSION_TYPE", "wayland")
    monkeypatch.setattr(
        "mluva_linux.terminal_target.shutil.which",
        {name: f"/usr/bin/{name}" for name in ("hyprctl", "wl-copy", "wtype")}.get,
    )
    monkeypatch.setattr("mluva_linux.terminal_target.os.readlink", lambda _path: executable)
    monkeypatch.setattr("mluva_linux.delivery.time.sleep", lambda _delay: None)

    def run(command: list[str], **kwargs: object) -> subprocess.CompletedProcess[str]:
        """Expose a synthetic compositor peer and record only clipboard/input effects."""
        if command[0] == "/usr/bin/hyprctl":
            assert kwargs["env"]["HYPRLAND_INSTANCE_SIGNATURE"] == "isolated-test"
            if disconnected:
                raise subprocess.TimeoutExpired(command, 0.5)
            return subprocess.CompletedProcess(command, 0, json.dumps(window))
        calls.append((command, kwargs.get("input")))
        return subprocess.CompletedProcess(command, 0, "")

    target = None
    monkeypatch.setattr("mluva_linux.terminal_target.subprocess.run", run)
    target = capture_hyprland_terminal_target()
    assert target is not None
    assert target.restore()
    if change == "address":
        window["address"] = "0x5678"
    elif change == "pid":
        window["pid"] = 43
    elif change == "executable":
        executable = "/usr/bin/chromium"
    elif change == "unmapped":
        window["mapped"] = False
    elif change == "no-window":
        window.clear()
    elif change == "disconnected":
        disconnected = True

    receipt = deliver_text(
        "Příliš žluťoučký",
        auto_paste=True,
        insert_directly=target.insert_text,
        confirm_paste=lambda: target.confirm_insertion("Příliš žluťoučký"),
        authorize_keyboard_paste=target.restore,
        application_identifier=target.application_identifier,
    )

    assert calls[0] == (["/usr/bin/wl-copy"], "Příliš žluťoučký")
    if change is None:
        assert calls[1:] == [(["/usr/bin/wtype", "-M", "ctrl", "-M", "shift", "v", "-m", "shift", "-m", "ctrl"], None)]
        assert receipt.paste_dispatched and receipt.history_outcome == "paste-unconfirmed"
        assert not receipt.pasted and receipt.paste_confirmed is None
    else:
        assert len(calls) == 1
        assert not receipt.paste_dispatched and receipt.history_outcome == "copied"


@pytest.mark.parametrize("executable", ["/usr/bin/chromium", "/usr/bin/code", "/usr/bin/not-foot"])
def test_nonterminal_windows_never_authorize_paste_without_a_text_target(
    monkeypatch: pytest.MonkeyPatch, executable: str
) -> None:
    """Require accessibility for ordinary applications, regardless of their window title or class."""
    monkeypatch.setenv("HYPRLAND_INSTANCE_SIGNATURE", "isolated-test")
    monkeypatch.setattr("mluva_linux.terminal_target.shutil.which", lambda _name: "/usr/bin/hyprctl")
    monkeypatch.setattr("mluva_linux.terminal_target.os.readlink", lambda _path: executable)
    monkeypatch.setattr(
        "mluva_linux.terminal_target.subprocess.run",
        lambda *_args, **_kwargs: subprocess.CompletedProcess(
            [], 0, json.dumps({"address": "0x1234", "pid": 42, "mapped": True, "title": "foot", "class": "foot"})
        ),
    )

    assert capture_hyprland_terminal_target() is None


def test_terminal_capture_remains_available_when_accessibility_is_disabled(monkeypatch: pytest.MonkeyPatch) -> None:
    """Allow Omarchy terminal delivery without pretending that Command mode can read a selection."""
    monkeypatch.setattr("mluva_linux.text_target.system_accessibility_enabled", lambda: False)
    monkeypatch.setattr("mluva_linux.text_target.hyprland_terminal_tracking_available", lambda: True)
    terminal = object()
    monkeypatch.setattr("mluva_linux.text_target.capture_hyprland_terminal_target", lambda: terminal)
    tracker = FocusedTextTargetTracker()
    try:
        assert tracker.capture_delivery_target() is terminal
        assert tracker.capture_text_target() is None
    finally:
        tracker.close()
