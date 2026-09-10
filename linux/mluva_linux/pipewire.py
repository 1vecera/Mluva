"""Read and validate PipeWire audio targets without opening a capture stream."""

from __future__ import annotations

import json
import shutil
import subprocess
from dataclasses import dataclass
from enum import StrEnum

MAX_PIPEWIRE_DUMP_BYTES = 20_000_000


class PipeWireCatalogError(RuntimeError):
    """Report unavailable or malformed PipeWire graph metadata."""


class PipeWireDeviceKind(StrEnum):
    """Separate microphone sources from Meeting system-output sinks."""

    MICROPHONE = "microphone"
    SYSTEM_OUTPUT = "system-output"


@dataclass(frozen=True, slots=True)
class PipeWireDevice:
    """Describe one stable `pw-record --target` choice and its local label."""

    target: str
    name: str
    kind: PipeWireDeviceKind


@dataclass(frozen=True, slots=True)
class PipeWireDeviceCatalog:
    """Hold a snapshot of selectable PipeWire nodes."""

    microphones: tuple[PipeWireDevice, ...] = ()
    system_outputs: tuple[PipeWireDevice, ...] = ()

    @classmethod
    def from_system(cls, executable: str | None = None) -> PipeWireDeviceCatalog:
        """Read one bounded `pw-dump` snapshot without opening or recording any device."""
        resolved_executable = executable or shutil.which("pw-dump")
        if resolved_executable is None:
            raise PipeWireCatalogError("pw-dump is required to list PipeWire audio devices.")
        try:
            result = subprocess.run(
                [resolved_executable, "--no-colors"],
                capture_output=True,
                check=False,
                timeout=5,
            )
        except (OSError, subprocess.TimeoutExpired) as error:
            raise PipeWireCatalogError("PipeWire audio devices could not be listed.") from error
        if result.returncode != 0:
            raise PipeWireCatalogError("PipeWire audio devices could not be listed.")
        if len(result.stdout) > MAX_PIPEWIRE_DUMP_BYTES:
            raise PipeWireCatalogError("PipeWire returned an unexpectedly large device graph.")
        return parse_pipewire_devices(result.stdout)

    def find(self, kind: PipeWireDeviceKind, target: str | None) -> PipeWireDevice | None:
        """Resolve one configured target from this immutable snapshot."""
        if target is None:
            return None
        devices = self.microphones if kind is PipeWireDeviceKind.MICROPHONE else self.system_outputs
        return next((device for device in devices if device.target == target), None)

    def display_name(self, kind: PipeWireDeviceKind, target: str | None) -> str:
        """Return an honest selected-device label, including unavailable persisted targets."""
        if target is None:
            return "Default (automatic)"
        device = self.find(kind, target)
        return device.name if device is not None else f"Unavailable target: {target}"


def parse_pipewire_devices(payload: bytes | str) -> PipeWireDeviceCatalog:
    """Parse only selectable audio nodes from an untrusted `pw-dump` JSON snapshot."""
    try:
        decoded = json.loads(payload)
    except (json.JSONDecodeError, UnicodeDecodeError) as error:
        raise PipeWireCatalogError("PipeWire returned invalid device metadata.") from error
    if not isinstance(decoded, list):
        raise PipeWireCatalogError("PipeWire device metadata must be a JSON array.")
    microphones: dict[str, str] = {}
    system_outputs: dict[str, str] = {}
    for value in decoded:
        if not isinstance(value, dict) or not _is_node(value.get("type")):
            continue
        info = value.get("info")
        if not isinstance(info, dict):
            continue
        properties = info.get("props")
        if not isinstance(properties, dict) or properties.get("node.disabled") is True:
            continue
        media_class = properties.get("media.class")
        target = properties.get("node.name")
        if not isinstance(target, str) or not _valid_target(target):
            continue
        description_value = properties.get("node.description") or properties.get("node.nick") or target
        description = description_value.strip() if isinstance(description_value, str) else target
        if not description:
            description = target
        if media_class == "Audio/Source" and not _is_monitor(properties, target):
            microphones.setdefault(target, description)
        elif media_class == "Audio/Sink":
            system_outputs.setdefault(target, description)
    return PipeWireDeviceCatalog(
        microphones=_devices(microphones, PipeWireDeviceKind.MICROPHONE),
        system_outputs=_devices(system_outputs, PipeWireDeviceKind.SYSTEM_OUTPUT),
    )


def _devices(values: dict[str, str], kind: PipeWireDeviceKind) -> tuple[PipeWireDevice, ...]:
    """Disambiguate duplicate labels and return stable case-insensitive ordering."""
    label_counts: dict[str, int] = {}
    for label in values.values():
        key = label.casefold()
        label_counts[key] = label_counts.get(key, 0) + 1
    devices = [
        PipeWireDevice(
            target=target,
            name=f"{label} ({target})" if label_counts[label.casefold()] > 1 else label,
            kind=kind,
        )
        for target, label in values.items()
    ]
    return tuple(sorted(devices, key=lambda device: (device.name.casefold(), device.target)))


def _is_node(value: object) -> bool:
    """Recognize the stable PipeWire node interface name."""
    return isinstance(value, str) and value.endswith(":Node")


def _is_monitor(properties: dict[str, object], target: str) -> bool:
    """Exclude sink-monitor pseudo-sources because Meeting selects the sink itself."""
    device_class = properties.get("device.class")
    return device_class == "monitor" or target.casefold().endswith(".monitor")


def _valid_target(value: str) -> bool:
    """Accept a bounded single-line node name safe for a direct argv element."""
    return value == value.strip() and 0 < len(value) <= 512 and all(ord(character) >= 32 for character in value)
