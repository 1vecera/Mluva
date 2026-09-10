"""Shared compact timestamps for conversation and recording history."""

from datetime import datetime


def history_timestamp(value: str, time_format: str = "24h") -> str:
    """Use local time with a weekday, unpadded day and explicit 12/24-hour preference."""
    local = datetime.fromisoformat(value).astimezone()
    clock = local.strftime("%H:%M") if time_format == "24h" else local.strftime("%I:%M %p").lstrip("0")
    return f"{local:%a} {local.day} {local:%b} · {clock}"
