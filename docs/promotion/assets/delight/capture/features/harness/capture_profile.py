"""Keep the launch capture's guest, isolation and encoder boundaries explicit."""

import socket
import tomllib
from pathlib import Path


def validate_host(expected: str, runtime: Path) -> None:
    """Reject a different machine or a guest payload outside its own installed home."""
    if socket.gethostname() != expected:
        raise RuntimeError(f"This capture requires the declared host {expected}")
    if expected == "claw-mini-capture":
        profile = tomllib.loads(Path("/capture/mac-capture.toml").read_text())
        if profile["guest_hostname"] != expected or Path.home() != Path("/home/developer"):
            raise RuntimeError("The prepared guest profile or home changed")
        if runtime != Path.home() / ".local/share/voice-scribe/app":
            raise RuntimeError("Capture the guest's own installed payload")


def encoder_arguments(encoder: str) -> list[str]:
    """Use the measured guest OpenH264 path or the original Lenovo x264 path."""
    if encoder == "libopenh264":
        return ["-c:v", encoder, "-b:v", "20M", "-threads", "3", "-pix_fmt", "yuv420p"]
    if encoder == "libx264":
        return ["-c:v", encoder, "-preset", "ultrafast", "-crf", "16", "-threads", "3", "-pix_fmt", "yuv420p"]
    raise ValueError("Unsupported capture encoder")
