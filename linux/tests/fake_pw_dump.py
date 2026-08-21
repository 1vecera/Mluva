#!/usr/bin/env python3
"""Return a deterministic PipeWire graph without querying the live desktop."""

import json


def main() -> None:
    """Print representative source, monitor, sink, duplicate, and irrelevant nodes."""
    print(
        json.dumps(
            [
                {
                    "id": 10,
                    "type": "PipeWire:Interface:Node",
                    "info": {
                        "props": {
                            "media.class": "Audio/Source",
                            "node.name": "alsa_input.usb_primary",
                            "node.description": "USB Microphone",
                        }
                    },
                },
                {
                    "id": 11,
                    "type": "PipeWire:Interface:Node",
                    "info": {
                        "props": {
                            "media.class": "Audio/Source",
                            "node.name": "alsa_input.usb_secondary",
                            "node.description": "USB Microphone",
                        }
                    },
                },
                {
                    "id": 12,
                    "type": "PipeWire:Interface:Node",
                    "info": {
                        "props": {
                            "media.class": "Audio/Source",
                            "node.name": "alsa_output.internal.monitor",
                            "node.description": "Internal Monitor",
                            "device.class": "monitor",
                        }
                    },
                },
                {
                    "id": 20,
                    "type": "PipeWire:Interface:Node",
                    "info": {
                        "props": {
                            "media.class": "Audio/Sink",
                            "node.name": "alsa_output.internal",
                            "node.description": "Built-in Audio",
                        }
                    },
                },
                {
                    "id": 21,
                    "type": "PipeWire:Interface:Node",
                    "info": {
                        "props": {
                            "media.class": "Audio/Sink",
                            "node.name": "disabled-output",
                            "node.description": "Disabled",
                            "node.disabled": True,
                        }
                    },
                },
                {
                    "id": 30,
                    "type": "PipeWire:Interface:Node",
                    "info": {"props": {"media.class": "Video/Source", "node.name": "camera"}},
                },
            ]
        )
    )


if __name__ == "__main__":
    main()
