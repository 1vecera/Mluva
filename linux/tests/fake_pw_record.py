#!/usr/bin/env python3
"""Emulate Fedora pw-record's SIGINT finalization behavior for contract tests."""

import signal
import sys
import time
from array import array


def finalize(_signal_number: int, _frame: object) -> None:
    """Exit with the nonzero status emitted after a valid Fedora recording."""
    print("fake PipeWire finalization", file=sys.stderr)
    raise SystemExit(1)


def main() -> None:
    """Write deterministic little-endian PCM to stdout until the recorder stops it."""
    signal.signal(signal.SIGINT, finalize)
    samples = array("h", [1_000] * 1_600)
    if sys.byteorder != "little":
        samples.byteswap()
    chunk = samples.tobytes()
    while True:
        sys.stdout.buffer.write(chunk)
        sys.stdout.buffer.flush()
        time.sleep(0.02)


if __name__ == "__main__":
    main()
