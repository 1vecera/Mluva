"""Verify and launch the actual frozen per-user installation in the declared capture guest."""

import argparse
import json
import os
import socket
import subprocess
from datetime import UTC, datetime
from pathlib import Path

from capture_delight import digest, installed_snapshot
from capture_profile import validate_host


def main() -> None:
    """Record installed-file identity and a credential-free launcher probe outside the checkout."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="Fresh evidence directory below this checkout's tmp/")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    output = args.output.resolve()
    output.relative_to(root / "tmp")
    runtime = Path.home() / ".local/share/voice-scribe/app"
    validate_host("claw-mini-capture", runtime)
    revision = "6022b05e4258768e7aa70305febf7dad0835b060"
    if subprocess.check_output(["git", "diff", revision, "--", "linux"], cwd=root):
        raise RuntimeError("The film requires the frozen Linux source")
    hashes, icon_hash = installed_snapshot(root, runtime)
    entrypoint = Path.home() / ".local/bin/mluva"
    expected = (root / "linux/resources/mluva.in").read_text().replace("@APPLICATION_DIR@", str(runtime))
    if entrypoint.read_text() != expected or not os.access(entrypoint, os.X_OK):
        raise RuntimeError("The actual installed launcher differs from its source template")
    output.mkdir(parents=True, exist_ok=False)
    driver = output / "launch.sh"
    driver.write_text("""#!/usr/bin/env bash
set -euo pipefail
cd "$OFFSCREEN_SESSION_ROOT"
/home/developer/.local/bin/mluva > "$OFFSCREEN_ARTIFACT_DIR/app.log" 2>&1 &
app_pid=$!
trap 'kill "$app_pid" 2>/dev/null || true; wait "$app_pid" 2>/dev/null || true' EXIT
for attempt in {1..80}; do
    kill -0 "$app_pid"
    if xwininfo -root -tree | grep -q '"Mluva"'; then
        sleep 1
        magick import -silent -window root "$OFFSCREEN_ARTIFACT_DIR/installed.png"
        tr '\\0' '\\n' < "/proc/$app_pid/cmdline" > "$OFFSCREEN_ARTIFACT_DIR/command.txt"
        readlink "/proc/$app_pid/cwd" > "$OFFSCREEN_ARTIFACT_DIR/cwd.txt"
        exit 0
    fi
    sleep 0.1
done
exit 1
""")
    environment = {
        "PATH": "/usr/local/bin:/usr/bin:/bin",
        "HOME": str(Path.home()),
        "LANG": "C.UTF-8",
        "OFFSCREEN_DISPLAY_NUMBER": "203",
        "OFFSCREEN_ENABLE_ATSPI": "1",
        "OFFSCREEN_SCREEN_SPEC": "1920x1080x24",
        "VOICE_SCRIBE_DISABLE_GLOBAL_SHORTCUT": "1",
        "ADW_DISABLE_PORTAL": "1",
        "GSK_RENDERER": "cairo",
        "PIPEWIRE_REMOTE": "disabled-install-probe",
        "PULSE_SERVER": "unix:/nonexistent-capture-pulse",
        "DBUS_SYSTEM_BUS_ADDRESS": "unix:path=/nonexistent-capture-system-bus",
    }
    with (output / "session.log").open("w") as log:
        subprocess.run(
            ["bash", str(root / "dev/run-isolated.sh"), str(output), "--", "bash", str(driver)],
            cwd=root,
            env=environment,
            stdout=log,
            stderr=subprocess.STDOUT,
            check=True,
            timeout=25,
        )
    if str(runtime / ".venv/bin/python") not in (output / "command.txt").read_text():
        raise RuntimeError("The installed launcher selected a different interpreter")
    module = subprocess.check_output(
        [str(runtime / ".venv/bin/python"), "-c", "import voice_scribe_linux; print(voice_scribe_linux.__file__)"],
        cwd=output,
        env={**environment, "PYTHONPATH": str(runtime)},
        text=True,
    ).strip()
    Path(module).relative_to(runtime)
    receipt = {
        "host": socket.gethostname(),
        "architecture": os.uname().machine,
        "source_revision": revision,
        "installed_runtime": str(runtime),
        "installed_at_utc": datetime.fromtimestamp(entrypoint.stat().st_mtime, UTC).isoformat(),
        "verified_at_utc": datetime.now(UTC).isoformat(),
        "entrypoint": str(entrypoint),
        "entrypoint_sha256": digest(entrypoint),
        "installed_files": hashes,
        "plugin_files": {key: value for key, value in hashes.items() if key.startswith("quickshell/")},
        "installed_icon_sha256": icon_hash,
        "imported_module": module,
        "entrypoint_probe": {
            "command": (output / "command.txt").read_text().splitlines(),
            "cwd": (output / "cwd.txt").read_text().strip(),
            "screenshot_sha256": digest(output / "installed.png"),
        },
        "verified": True,
        "boundary": (
            "Actual per-user guest installation, launched outside the checkout on private X11/D-Bus/AT-SPI. "
            "No credential or provider request; the expected missing-credential setup banner remains visible."
        ),
    }
    (output / "installation.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(
        json.dumps(
            {"installed_entrypoint_verified": True, "files": len(hashes), "receipt": str(output / "installation.json")}
        )
    )


if __name__ == "__main__":
    main()
