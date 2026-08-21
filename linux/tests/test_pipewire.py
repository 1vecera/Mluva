"""Fixture-only coverage for PipeWire device metadata and stable targets."""

from pathlib import Path

import pytest

from voice_scribe_linux.pipewire import (
    PipeWireCatalogError,
    PipeWireDeviceCatalog,
    PipeWireDeviceKind,
    parse_pipewire_devices,
)


def test_catalog_reads_audio_targets_without_live_device_access() -> None:
    """Parse a fake `pw-dump` process and exclude monitors, disabled nodes, and video."""
    fake_dump = Path(__file__).with_name("fake_pw_dump.py")

    catalog = PipeWireDeviceCatalog.from_system(executable=str(fake_dump))

    assert tuple(device.target for device in catalog.microphones) == (
        "alsa_input.usb_primary",
        "alsa_input.usb_secondary",
    )
    assert tuple(device.name for device in catalog.microphones) == (
        "USB Microphone (alsa_input.usb_primary)",
        "USB Microphone (alsa_input.usb_secondary)",
    )
    assert tuple(device.target for device in catalog.system_outputs) == ("alsa_output.internal",)
    assert catalog.display_name(PipeWireDeviceKind.SYSTEM_OUTPUT, "alsa_output.internal") == "Built-in Audio"
    assert catalog.display_name(PipeWireDeviceKind.MICROPHONE, None) == "Default (automatic)"


def test_catalog_keeps_unavailable_persisted_target_visible() -> None:
    """Describe a disconnected configured node without pretending it is selectable now."""
    catalog = PipeWireDeviceCatalog()

    assert catalog.display_name(PipeWireDeviceKind.MICROPHONE, "missing-source") == (
        "Unavailable target: missing-source"
    )


@pytest.mark.parametrize("payload", [b"not-json", b"{}"])
def test_catalog_rejects_malformed_graph_roots(payload: bytes) -> None:
    """Reject invalid JSON and non-array graph roots with a controlled error."""
    with pytest.raises(PipeWireCatalogError):
        parse_pipewire_devices(payload)


def test_catalog_ignores_node_with_unusable_metadata() -> None:
    """Ignore one malformed node without discarding an otherwise valid graph snapshot."""
    payload = b'[{"type":"PipeWire:Interface:Node","info":[]}]'

    assert parse_pipewire_devices(payload) == PipeWireDeviceCatalog()
