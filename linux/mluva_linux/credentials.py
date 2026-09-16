"""Optional desktop keyring storage; API keys never enter Mluva's settings or logs."""

import subprocess
from functools import lru_cache


@lru_cache(maxsize=1)
def stored_speech_key() -> str:
    """Read the desktop secret service with a bounded, silent lookup."""
    try:
        result = subprocess.run(
            ["secret-tool", "lookup", "application", "mluva", "provider", "elevenlabs"],
            capture_output=True,
            text=True,
            timeout=2,
            check=False,
        )
        return result.stdout.strip() if result.returncode == 0 else ""
    except (OSError, subprocess.TimeoutExpired):
        return ""


def store_speech_key(key: str) -> None:
    """Send a user-entered key through stdin to the desktop keyring, never argv."""
    if not key.strip() or len(key) > 4096 or any(c.isspace() for c in key):
        raise ValueError("Enter a valid API key.")
    try:
        result = subprocess.run(
            [
                "secret-tool",
                "store",
                "--label=Mluva ElevenLabs API key",
                "application",
                "mluva",
                "provider",
                "elevenlabs",
            ],
            input=key,
            capture_output=True,
            text=True,
            timeout=60,
            check=False,
        )
        if result.returncode:
            raise RuntimeError(
                "Desktop keyring unavailable. Set ELEVENLABS_API_KEY in your launch environment instead."
            )
    except (OSError, subprocess.TimeoutExpired):
        raise RuntimeError(
            "Desktop keyring unavailable. Set ELEVENLABS_API_KEY in your launch environment instead."
        ) from None
    stored_speech_key.cache_clear()
