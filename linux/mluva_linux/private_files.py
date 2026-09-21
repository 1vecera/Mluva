"""Create private recovery documents before writing any user content."""

import os
import tempfile
from pathlib import Path


def atomic_write_private_text(path: Path, content: str) -> None:
    """Replace a document atomically with an unpredictable, owner-only temporary file."""
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    path.parent.chmod(0o700)
    descriptor, name = tempfile.mkstemp(prefix=f".{path.name}.", suffix=".tmp", dir=path.parent)
    temporary_path = Path(name)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8") as stream:
            stream.write(content)
        temporary_path.replace(path)
    finally:
        temporary_path.unlink(missing_ok=True)
