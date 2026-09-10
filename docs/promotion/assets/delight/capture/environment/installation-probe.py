"""Verify the guest installation and actually launch its installed entrypoint in isolation."""

import json
import os
import socket
import subprocess
import sys
from datetime import UTC, datetime
from pathlib import Path

root = Path.cwd()
sys.path.insert(0, str(root / "dev"))
from capture_delight import digest, installed_snapshot

runtime = Path.home() / ".local/share/voice-scribe/app"
entrypoint = Path.home() / ".local/bin/mluva"
revision = "6022b05e4258768e7aa70305febf7dad0835b060"
assert socket.gethostname() == "claw-mini-capture"
assert subprocess.check_output(["git", "diff", revision, "--", "linux"]) == b""
hashes, icon_hash = installed_snapshot(root, runtime)
expected = (root / "linux/resources/mluva.in").read_text().replace("@APPLICATION_DIR@", str(runtime))
assert entrypoint.read_text() == expected
assert os.access(entrypoint, os.X_OK)
probe = root / "tmp/delight-film/installed-entrypoint-v2"
probe.mkdir()
driver = probe / "launch.sh"
driver.write_text('''#!/usr/bin/env bash
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
''')
environment = {
    "PATH": "/usr/local/bin:/usr/bin:/bin", "HOME": str(Path.home()), "LANG": "C.UTF-8",
    "OFFSCREEN_DISPLAY_NUMBER": "203", "OFFSCREEN_ENABLE_ATSPI": "1", "OFFSCREEN_SCREEN_SPEC": "1920x1080x24",
    "VOICE_SCRIBE_DISABLE_GLOBAL_SHORTCUT": "1", "ADW_DISABLE_PORTAL": "1", "GSK_RENDERER": "cairo",
    "PIPEWIRE_REMOTE": "disabled-install-probe", "PULSE_SERVER": "unix:/nonexistent-capture-pulse",
    "DBUS_SYSTEM_BUS_ADDRESS": "unix:path=/nonexistent-capture-system-bus",
}
with (probe / "session.log").open("w") as log:
    subprocess.run(["bash", str(root / "dev/run-isolated.sh"), str(probe), "--", "bash", str(driver)],
                   env=environment, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=25)
assert str(runtime / ".venv/bin/python") in (probe / "command.txt").read_text()
module = subprocess.check_output([str(runtime / ".venv/bin/python"), "-c",
    "import voice_scribe_linux; print(voice_scribe_linux.__file__)"],
    cwd=probe, env={**environment, "PYTHONPATH": str(runtime)}, text=True).strip()
assert Path(module).is_relative_to(runtime)
receipt = {
    "host": socket.gethostname(), "architecture": os.uname().machine, "source_revision": revision,
    "installed_runtime": str(runtime), "installed_at_utc": datetime.now(UTC).isoformat(),
    "entrypoint": str(entrypoint), "entrypoint_sha256": digest(entrypoint),
    "installed_files": hashes, "plugin_files": {k: v for k, v in hashes.items() if k.startswith("quickshell/")},
    "installed_icon_sha256": icon_hash, "imported_module": module,
    "entrypoint_probe": {"command": (probe / "command.txt").read_text().splitlines(),
                         "cwd": (probe / "cwd.txt").read_text().strip(),
                         "screenshot_sha256": digest(probe / "installed.png")},
    "verified": True,
    "boundary": "Actual per-user guest installation, launched outside the source checkout on private X11/D-Bus/AT-SPI.",
}
(root / "tmp/delight-film/guest-installation.json").write_text(json.dumps(receipt, indent=2) + "\n")
print(json.dumps({"installed_entrypoint_verified": True, "files": len(hashes), "host": receipt["host"]}))
