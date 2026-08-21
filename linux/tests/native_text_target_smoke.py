"""Exercise Mluva's production AT-SPI insertion against a private GTK target."""

from __future__ import annotations

import json
import os
import subprocess
import sys
import time
from pathlib import Path

import gi

gi.require_version("Gtk", "4.0")
from gi.repository import GLib, Gtk  # noqa: E402

from voice_scribe_linux.text_target import (  # noqa: E402
    FocusedTextTargetTracker,
    system_accessibility_enabled,
)

INITIAL_TEXT = "Mluva: "
INSERTED_TEXT = os.environ.get("OFFSCREEN_INSERTED_TEXT", "Příliš žluťoučký kůň")
EXPECTED_TEXT = INITIAL_TEXT + INSERTED_TEXT


def run_target(evidence_dir: Path) -> int:
    """Expose one real GTK entry from a process separate from the AT-SPI client."""
    observed_path = evidence_dir / "target-observed.json"
    ready_path = evidence_dir / "target-ready"
    window = Gtk.Window(title="Mluva private insertion target")
    window.set_default_size(720, 180)
    entry = Gtk.Entry()
    entry.set_text(INITIAL_TEXT)
    entry.set_position(-1)
    window.set_child(entry)
    loop = GLib.MainLoop()

    def observe_text() -> bool:
        actual_text = entry.get_text()
        temporary_path = evidence_dir / "target-observed.tmp"
        temporary_path.write_text(
            json.dumps(
                {
                    "exact_text_match": actual_text == EXPECTED_TEXT,
                    "final_character_count": len(actual_text),
                    "actual_text": actual_text,
                },
                ensure_ascii=False,
                indent=2,
            )
            + "\n",
            encoding="utf-8",
        )
        temporary_path.replace(observed_path)
        return GLib.SOURCE_CONTINUE

    def settle_caret() -> bool:
        caret_offset = len(INITIAL_TEXT)
        entry.set_position(caret_offset)
        entry.select_region(caret_offset, caret_offset)
        ready_path.write_text("ready\n", encoding="utf-8")
        return GLib.SOURCE_REMOVE

    window.present()
    entry.grab_focus()
    GLib.timeout_add(250, settle_caret)
    GLib.timeout_add(25, observe_text)
    loop.run()
    return 0


def run_client(evidence_dir: Path) -> int:
    """Capture the external target through focus events and insert exact Unicode text."""
    result_path = evidence_dir / "native-text-insertion.json"
    screenshot_path = evidence_dir / "native-text-insertion.png"
    subprocess.run(
        ["gsettings", "set", "org.gnome.desktop.interface", "toolkit-accessibility", "true"],
        check=True,
    )
    if not system_accessibility_enabled():
        raise RuntimeError("The private session did not report AT-SPI as enabled")

    tracker = FocusedTextTargetTracker()
    target_process = subprocess.Popen(
        [sys.executable, str(Path(__file__).resolve()), "--target"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        text=True,
    )
    outcome: dict[str, object] = {
        "desktop_status_gate_enabled": True,
        "atspi_bus_private": bool(os.environ.get("AT_SPI_BUS_ADDRESS")),
        "display": os.environ.get("DISPLAY", ""),
        "focused_target_captured": False,
        "restored": False,
        "inserted": False,
        "exact_text_match": False,
        "unicode_payload": INSERTED_TEXT,
    }
    started_at = time.monotonic()
    loop = GLib.MainLoop()

    def stop_target() -> None:
        if target_process.poll() is None:
            target_process.terminate()
            try:
                target_process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                target_process.kill()
                target_process.wait(timeout=3)

    def finish_with_error(message: str) -> None:
        outcome["error"] = message
        loop.quit()

    def attempt_insertion() -> bool:
        if time.monotonic() - started_at > 12:
            finish_with_error("Timed out waiting for the real GTK entry focus event")
            return GLib.SOURCE_REMOVE
        if not (evidence_dir / "target-ready").is_file():
            return GLib.SOURCE_CONTINUE

        snapshot = tracker.capture_delivery_target()
        if snapshot is None:
            return GLib.SOURCE_CONTINUE

        outcome["focused_target_captured"] = True
        outcome["application_identifier_present"] = bool(snapshot.application_identifier)
        outcome["captured_caret_offset"] = snapshot.caret_offset
        outcome["selected_text_retained"] = snapshot.selected_text is not None
        outcome["selection_start"] = snapshot.selection_start
        outcome["selection_end"] = snapshot.selection_end
        outcome["editable_text_available"] = snapshot.editable_text is not None
        outcome["restored"] = snapshot.restore()
        outcome["inserted"] = snapshot.insert_text(INSERTED_TEXT)

        observed_path = evidence_dir / "target-observed.json"
        for _attempt in range(100):
            if observed_path.is_file():
                try:
                    observed = json.loads(observed_path.read_text(encoding="utf-8"))
                except json.JSONDecodeError:
                    observed = {}
                outcome["exact_text_match"] = bool(observed.get("exact_text_match"))
                outcome["final_character_count"] = observed.get("final_character_count")
                outcome["actual_text"] = observed.get("actual_text")
                if outcome["exact_text_match"]:
                    break
            time.sleep(0.02)
        outcome["eventual_confirmation"] = snapshot.confirm_insertion(INSERTED_TEXT)
        outcome["eventual_caret_offset"] = snapshot.text.get_caret_offset()
        loop.quit()
        return GLib.SOURCE_REMOVE

    try:
        GLib.timeout_add(50, attempt_insertion)
        loop.run()
        required = (
            outcome["focused_target_captured"],
            outcome["restored"],
            outcome["inserted"],
            outcome["exact_text_match"],
            outcome.get("eventual_confirmation"),
            outcome.get("editable_text_available"),
            outcome["atspi_bus_private"],
        )
        result_path.write_text(json.dumps(outcome, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        if not all(required):
            print(json.dumps(outcome, ensure_ascii=False, indent=2), file=sys.stderr)
            return 1
        subprocess.run(
            ["import", "-display", os.environ["DISPLAY"], "-window", "root", str(screenshot_path)],
            check=True,
        )
    finally:
        tracker.close()
        stop_target()

    print("native_atspi_focus_capture=passed")
    print("native_atspi_unicode_insertion=passed")
    print("native_atspi_exact_target_confirmation=passed")
    return 0


if __name__ == "__main__":
    artifact_dir = Path(os.environ["OFFSCREEN_ARTIFACT_DIR"])
    if len(sys.argv) == 2 and sys.argv[1] == "--target":
        raise SystemExit(run_target(artifact_dir))
    raise SystemExit(run_client(artifact_dir))
