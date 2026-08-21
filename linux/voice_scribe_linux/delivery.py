"""Safe clipboard-first text delivery for Linux desktops."""

import os
import shutil
import socket
import stat
import subprocess
import time
from collections.abc import Callable, Mapping
from dataclasses import dataclass
from pathlib import Path


class DeliveryError(RuntimeError):
    """Report that text could not be placed on the desktop clipboard."""


@dataclass(frozen=True, slots=True)
class DeliveryReceipt:
    """Separate clipboard installation, paste dispatch, and confirmed insertion."""

    copied: bool
    pasted: bool
    guidance: str
    paste_dispatched: bool = False
    paste_confirmed: bool | None = None

    @property
    def history_outcome(self) -> str:
        """Return one durable receipt without overstating an unconfirmed key dispatch."""
        if self.pasted:
            return "pasted"
        if self.paste_dispatched:
            return "paste-unconfirmed"
        return "copied"


def deliver_text(
    text: str,
    auto_paste: bool,
    confirm_paste: Callable[[], bool | None] | None = None,
    insert_directly: Callable[[str], bool | None] | None = None,
    authorize_keyboard_paste: Callable[[], bool] | None = None,
    confirmation_timeout_seconds: float = 0.75,
) -> DeliveryReceipt:
    """Copy once, attempt one native insertion or paste, and distinguish dispatch from confirmation."""
    if not text:
        raise DeliveryError("Cannot deliver empty text.")
    if confirmation_timeout_seconds < 0:
        raise ValueError("Paste confirmation timeout cannot be negative.")
    clipboard = shutil.which("wl-copy") or shutil.which("xclip")
    if clipboard is None:
        raise DeliveryError("Install wl-clipboard on Wayland or xclip on X11.")
    command = [clipboard] if clipboard.endswith("wl-copy") else [clipboard, "-selection", "clipboard"]
    subprocess.run(command, input=text, text=True, check=True)
    if not auto_paste:
        return DeliveryReceipt(copied=True, pasted=False, guidance="Copied. Paste in the target application.")
    if insert_directly is not None:
        try:
            inserted_directly = insert_directly(text)
        except Exception:
            inserted_directly = False
        if inserted_directly is True:
            return DeliveryReceipt(
                copied=True,
                pasted=True,
                guidance="Inserted once into the restored target through its accessibility editing interface.",
                paste_dispatched=True,
                paste_confirmed=True,
            )
        if inserted_directly is False:
            return DeliveryReceipt(
                copied=True,
                pasted=False,
                guidance=(
                    "Automatic insertion was attempted once, but the captured target did not confirm it. The "
                    "complete text remains on the clipboard; inspect the target before any manual retry."
                ),
                paste_dispatched=True,
                paste_confirmed=False,
            )
    paste_command = _paste_command()
    if paste_command is None:
        return DeliveryReceipt(
            copied=True,
            pasted=False,
            guidance="Copied. No supported keyboard paste helper is ready; paste manually.",
        )
    time.sleep(0.12)
    if authorize_keyboard_paste is not None:
        try:
            keyboard_paste_allowed = authorize_keyboard_paste()
        except Exception:
            keyboard_paste_allowed = False
        if not keyboard_paste_allowed:
            return DeliveryReceipt(
                copied=True,
                pasted=False,
                guidance=(
                    "The captured target could not be revalidated immediately before keyboard delivery, so no "
                    "paste was attempted. The complete text remains on the clipboard."
                ),
            )
    try:
        subprocess.run(paste_command, check=True)
    except (OSError, subprocess.SubprocessError):
        return DeliveryReceipt(
            copied=True,
            pasted=False,
            guidance=(
                "Paste was attempted once, but the input injector did not report success. The complete text "
                "remains on the clipboard; inspect the target before any manual retry."
            ),
            paste_dispatched=True,
            paste_confirmed=None,
        )
    paste_confirmed = _confirm_paste(confirm_paste, confirmation_timeout_seconds)
    if paste_confirmed is True:
        return DeliveryReceipt(
            copied=True,
            pasted=True,
            guidance="Inserted once into the restored target application.",
            paste_dispatched=True,
            paste_confirmed=True,
        )
    if paste_confirmed is False:
        guidance = (
            "Paste was sent once, but the captured target did not confirm insertion. The complete text remains "
            "on the clipboard; inspect the target before any manual retry."
        )
    else:
        guidance = (
            "Paste was sent once, but this target could not confirm insertion. The complete text remains on the "
            "clipboard; inspect the target before any manual retry."
        )
    return DeliveryReceipt(
        copied=True,
        pasted=False,
        guidance=guidance,
        paste_dispatched=True,
        paste_confirmed=paste_confirmed,
    )


def _confirm_paste(
    confirm_paste: Callable[[], bool | None] | None,
    timeout_seconds: float,
) -> bool | None:
    """Poll one captured target without turning post-dispatch uncertainty into a retryable error."""
    if confirm_paste is None:
        return None
    deadline = time.monotonic() + timeout_seconds
    while True:
        try:
            confirmation = confirm_paste()
        except Exception:
            return None
        if confirmation is True or confirmation is None:
            return confirmation
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            return False
        time.sleep(min(0.05, remaining))


def keyboard_paste_available(environment: Mapping[str, str] | None = None) -> bool:
    """Return whether one keyboard paste backend is usable now, not merely installed."""
    return _paste_command(environment) is not None


def _paste_command(environment: Mapping[str, str] | None = None) -> list[str] | None:
    """Resolve the first ready input injector supported by the active desktop."""
    environment = os.environ if environment is None else environment
    if executable := shutil.which("wtype"):
        return [executable, "-M", "ctrl", "v", "-m", "ctrl"]
    if (executable := shutil.which("ydotool")) and _ydotool_socket_ready(environment):
        return [executable, "key", "29:1", "47:1", "47:0", "29:0"]
    if (executable := shutil.which("xdotool")) and _x11_keyboard_injection_available(environment):
        return [executable, "key", "--clearmodifiers", "ctrl+v"]
    return None


def _ydotool_socket_ready(environment: Mapping[str, str]) -> bool:
    """Verify a live owner-only ydotoold socket without emitting an input event."""
    socket_path = _ydotool_socket_path(environment)
    try:
        socket_metadata = socket_path.stat()
    except OSError:
        return False
    socket_mode = stat.S_IMODE(socket_metadata.st_mode)
    if (
        not stat.S_ISSOCK(socket_metadata.st_mode)
        or socket_metadata.st_uid != os.geteuid()
        or socket_mode & 0o077
        or (socket_mode & 0o600) != 0o600
    ):
        return False
    client = socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM)
    client.settimeout(0.1)
    try:
        client.connect(str(socket_path))
    except OSError:
        return False
    finally:
        client.close()
    return True


def _x11_keyboard_injection_available(environment: Mapping[str, str]) -> bool:
    """Reject xdotool under Wayland, where DISPLAY normally names only XWayland clients."""
    session_type = environment.get("XDG_SESSION_TYPE", "").casefold()
    if session_type:
        return session_type == "x11"
    return bool(environment.get("DISPLAY")) and not environment.get("WAYLAND_DISPLAY")


def _ydotool_socket_path(environment: Mapping[str, str]) -> Path:
    """Mirror ydotool's documented socket resolution without invoking the client."""
    configured_socket = environment.get("YDOTOOL_SOCKET")
    if configured_socket:
        return Path(configured_socket)
    runtime_directory = environment.get("XDG_RUNTIME_DIR")
    if runtime_directory:
        return Path(runtime_directory) / ".ydotool_socket"
    return Path("/tmp/.ydotool_socket")
