"""Generate small real PNG images for independent storage and provider fixtures."""

import struct
import zlib


def png(red: int = 40) -> bytes:
    """Return two RGBA pixels whose encoded bytes change when an editor changes their color."""

    def chunk(kind: bytes, content: bytes) -> bytes:
        return struct.pack(">I", len(content)) + kind + content + struct.pack(">I", zlib.crc32(kind + content))

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", 2, 1, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(b"\0" + bytes((red, 60, 80, 255)) * 2))
        + chunk(b"IEND", b"")
    )
