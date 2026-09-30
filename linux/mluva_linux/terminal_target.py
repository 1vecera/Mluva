"""Capture a Hyprland terminal without reading terminal contents or guessing a text field."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
from collections.abc import Mapping
from dataclasses import dataclass, field
from pathlib import Path

from mluva_linux.delivery import TERMINAL_EXECUTABLES


def hyprland_terminal_tracking_available(environment: Mapping[str, str] | None = None) -> bool:
    """Require an explicit Hyprland session before enabling content-free terminal capture."""
    environment = os.environ if environment is None else environment
    return bool(environment.get("HYPRLAND_INSTANCE_SIGNATURE") and shutil.which("hyprctl"))


@dataclass(frozen=True, slots=True)
class TerminalTargetSnapshot:
    """Authorize paste only while the original terminal window and process remain focused."""

    address: str
    process_id: int
    application_identifier: str
    hyprctl: str
    environment: Mapping[str, str] = field(repr=False)
    editable_text: None = field(default=None, init=False)

    def restore(self) -> bool:
        """Revalidate exact terminal identity without activating or moving any window."""
        return _focused_terminal(self.hyprctl, self.environment) == (
            self.address,
            self.process_id,
            self.application_identifier,
        )

    def insert_text(self, _inserted_text: str) -> None:
        """Decline native editing before mutation so delivery can use one keyboard paste."""
        return None

    def confirm_insertion(self, _inserted_text: str) -> None:
        """Leave terminal delivery unconfirmed because no accessible caret is available."""
        return None

    def without_selected_text(self) -> TerminalTargetSnapshot:
        """Retain the same content-free identity for an explicit delivery retry."""
        return self


def capture_hyprland_terminal_target() -> TerminalTargetSnapshot | None:
    """Capture only an allowlisted terminal executable in the current Hyprland session."""
    if not hyprland_terminal_tracking_available() or (hyprctl := shutil.which("hyprctl")) is None:
        return None
    environment = {
        key: value for key, value in os.environ.items() if key in ("HYPRLAND_INSTANCE_SIGNATURE", "XDG_RUNTIME_DIR")
    }
    identity = _focused_terminal(hyprctl, environment)
    if identity is None:
        return None
    address, process_id, application_identifier = identity
    return TerminalTargetSnapshot(address, process_id, application_identifier, hyprctl, environment)


def _focused_terminal(hyprctl: str, environment: Mapping[str, str]) -> tuple[str, int, str] | None:
    """Read compositor identity and verify its owning executable without retaining window metadata."""
    try:
        response = subprocess.run(
            [hyprctl, "-j", "activewindow"],
            env=environment,
            capture_output=True,
            text=True,
            check=True,
            timeout=0.5,
        )
        window = json.loads(response.stdout)
        address = window["address"]
        process_id = window["pid"]
        if not isinstance(address, str) or not address.startswith("0x") or int(address, 16) == 0:
            return None
        if not isinstance(process_id, int) or process_id <= 0 or not window["mapped"]:
            return None
        executable = os.readlink(Path("/proc") / str(process_id) / "exe").removesuffix(" (deleted)")
        if Path(executable).name.casefold() not in TERMINAL_EXECUTABLES:
            return None
        return address, process_id, executable
    except (OSError, subprocess.SubprocessError, ValueError, KeyError, TypeError):
        return None
