"""Capture labeled production UI fixtures without microphone, credentials or inference."""

import argparse
import hashlib
import json
import os
import shutil
import signal
import socket
import subprocess
from pathlib import Path


def main() -> None:
    """Give each feature its own private desktop and persistent evidence directory."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--compositor", required=True, type=Path)
    parser.add_argument("--scenario", choices=("polish", "floating", "expanded", "history", "export", "providers"))
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    output = args.output.resolve()
    output.relative_to(root / "tmp")
    if socket.gethostname() != "lenovo":
        raise RuntimeError("This capture is authorized on Lenovo only")
    if any(Path(path).exists() for path in ("/tmp/.X195-lock", "/tmp/.X11-unix/X195")):
        raise RuntimeError("Reserved display :195 is occupied")
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
    if subprocess.check_output(["git", "diff", "HEAD", "--", "linux"], cwd=root):
        raise RuntimeError("Commit runtime changes before capturing their identity")
    if subprocess.check_output(["git", "ls-files", "--others", "--exclude-standard", "linux"], cwd=root):
        raise RuntimeError("Untracked runtime files must not enter a capture")
    runtime_files = subprocess.check_output(["git", "ls-files", "linux"], cwd=root, text=True).splitlines()
    runtime_hashes = {name: hashlib.sha256((root / name).read_bytes()).hexdigest() for name in runtime_files}
    helper = Path.home() / ".agents/skills/run-offscreen-linux-verification-daniel/scripts/run_isolated_x11.sh"
    for scenario in (
        [args.scenario] if args.scenario else ["polish", "floating", "expanded", "history", "export", "providers"]
    ):
        destination = output / scenario
        destination.mkdir(parents=True, exist_ok=False)
        harness = destination / "harness"
        harness.mkdir()
        for name in ("capture_features.py", "feature_session.py", "feature-stage.qml"):
            shutil.copy2(root / "dev" / name, harness / name)
        (destination / "runtime-files.json").write_text(json.dumps(runtime_hashes, indent=2) + "\n")
        environment = {
            "PATH": "/usr/bin:/bin",
            "HOME": str(Path.home()),
            "LANG": "C.UTF-8",
            "OFFSCREEN_DISPLAY_NUMBER": "195",
            "OFFSCREEN_SCREEN_SPEC": "1920x1080x24",
            "OFFSCREEN_ENABLE_ATSPI": "1",
            "PYTHONPATH": str(root / "linux"),
            "VOICE_SCRIBE_DISABLE_GLOBAL_SHORTCUT": "1",
            "ADW_DISABLE_PORTAL": "1",
            "GSK_RENDERER": "cairo",
            "QT_QPA_PLATFORM": "xcb",
            "QT_QUICK_BACKEND": "software",
            "MAGICK_THREAD_LIMIT": "1",
            "FILM_SCENARIO": scenario,
            "FILM_COMPOSITOR": str(args.compositor.resolve(strict=True)),
            "FILM_REVISION": revision,
        }
        with (destination / "session.log").open("w") as log:
            process = subprocess.Popen(
                [
                    "bash",
                    str(helper),
                    str(destination),
                    "--",
                    "uv",
                    "run",
                    "--project",
                    str(root / "linux"),
                    "--locked",
                    "python",
                    str(root / "dev/feature_session.py"),
                ],
                cwd=root,
                env=environment,
                stdout=log,
                stderr=subprocess.STDOUT,
                start_new_session=True,
            )
            try:
                result = process.wait(timeout=75)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGTERM)
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()
                raise
        if result:
            raise RuntimeError(f"Feature capture failed; inspect {destination}/session.log")
        if any(
            hashlib.sha256((root / name).read_bytes()).hexdigest() != digest for name, digest in runtime_hashes.items()
        ):
            raise RuntimeError("Runtime changed during capture")
        receipt = json.loads((destination / "capture.json").read_text())
        print(json.dumps({"scenario": scenario, "checks": receipt["checks"], "output": str(destination)}), flush=True)


if __name__ == "__main__":
    main()
