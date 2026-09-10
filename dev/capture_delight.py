"""Record the installed launch build on a private desktop, after a verified installation."""

import argparse
import hashlib
import json
import os
import shutil
import signal
import socket
import subprocess
from datetime import UTC, datetime
from pathlib import Path


def digest(path: Path) -> str:
    """Bind capture provenance to file contents."""
    return hashlib.sha256(path.read_bytes()).hexdigest()


def installed_snapshot(root: Path, runtime: Path) -> tuple[dict[str, str], str]:
    """Verify the installed payload and desktop icon against the frozen source."""
    if runtime == root / "linux":
        raise RuntimeError("Capture the installed build, not the working source directory")
    if subprocess.check_output(["git", "diff", "HEAD", "--", "linux"], cwd=root) or subprocess.check_output(
        ["git", "ls-files", "--others", "--exclude-standard", "linux"], cwd=root
    ):
        raise RuntimeError("Commit the tested runtime before capturing its installed payload")
    files = subprocess.check_output(["git", "ls-files", "linux"], cwd=root, text=True).splitlines()
    hashes = {}
    for name in files:
        relative = Path(name).relative_to("linux")
        is_payload = (
            (relative.parts[0] == "mluva_linux" and relative.suffix == ".py")
            or relative.parts[0] == "quickshell"
            or (
                relative.parts[:2] == ("gnome-extension", "recording-status@mluva.local")
                and relative.suffix in {".js", ".json", ".css", ".svg"}
            )
            or str(relative)
            in {
                "resources/mluva-input@.service",
                "mluva-shell",
                "pyproject.toml",
                "uv.lock",
                "configure-input-helper.sh",
                "configure-recording-overlay.sh",
                "uninstall.sh",
            }
        )
        if not is_payload:
            continue
        if not (runtime / relative).is_file() or digest(root / name) != digest(runtime / relative):
            raise RuntimeError(f"Installed runtime does not match the feature freeze: {relative}")
        hashes[str(relative)] = digest(runtime / relative)
    if not hashes:
        raise RuntimeError("No installed runtime was verified")
    installed_icon = runtime.parents[1] / "icons/hicolor/scalable/apps/com.mluva.Linux.svg"
    if digest(installed_icon) != digest(root / "linux/resources/com.mluva.Linux.svg"):
        raise RuntimeError("Installed desktop icon differs from the new logo")
    return hashes, digest(installed_icon)


def verify_installation_receipt(root: Path, runtime: Path, receipt: Path) -> str:
    """Reject a supplied receipt that identifies a different payload or source revision."""
    installation = json.loads(receipt.read_text())
    required = {
        "source_revision",
        "installed_runtime",
        "installed_at_utc",
        "entrypoint",
        "installed_files",
        "plugin_files",
    }
    if required - installation.keys() or installation.get("verified") is not True:
        raise RuntimeError("A verified live installation receipt with complete identity is required")
    if not Path(installation["entrypoint"]).is_file():
        raise RuntimeError("The installed entrypoint from the receipt does not exist")
    installed_at = datetime.fromisoformat(installation["installed_at_utc"].replace("Z", "+00:00"))
    if installed_at.tzinfo is None or installed_at > datetime.now(UTC):
        raise RuntimeError("The live installation must have completed before capture")
    if Path(installation["installed_runtime"]).resolve() != runtime:
        raise RuntimeError("Installation receipt identifies a different runtime directory")
    revision = subprocess.check_output(
        ["git", "rev-parse", installation["source_revision"] + "^{commit}"], cwd=root, text=True
    ).strip()
    recorded_tree = subprocess.check_output(["git", "rev-parse", revision + ":linux"], cwd=root, text=True).strip()
    frozen_tree = subprocess.check_output(["git", "rev-parse", "HEAD:linux"], cwd=root, text=True).strip()
    if recorded_tree != frozen_tree:
        raise RuntimeError("Installation receipt identifies a different Linux source tree")
    return revision


def main() -> None:
    """Require an installed matching runtime, isolate all desktop state, and retain evidence."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--runtime", required=True, type=Path, help="Installed application directory")
    parser.add_argument("--installation-receipt", required=True, type=Path)
    parser.add_argument("--compositor", required=True, type=Path)
    parser.add_argument("--pixel-ratio", type=int, choices=(1, 2), default=1)
    parser.add_argument("--source-take", type=Path, help="Reuse exact original and final draft from a real capture")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    output, runtime = args.output.resolve(), args.runtime.resolve(strict=True)
    output.relative_to(root / "tmp")
    receipt = args.installation_receipt.resolve(strict=True)
    if socket.gethostname() != "lenovo":
        raise RuntimeError("This capture is scoped to lenovo")
    if any(Path(path).exists() for path in ("/tmp/.X203-lock", "/tmp/.X11-unix/X203")):
        raise RuntimeError("Reserved private display :203 is occupied")
    hashes, installed_icon_sha256 = installed_snapshot(root, runtime)
    source_revision = verify_installation_receipt(root, runtime, receipt)
    output.mkdir(parents=True, exist_ok=False)
    output.chmod(0o700)
    harness = output / "harness"
    harness.mkdir()
    for name in ("capture_delight.py", "delight_session.py", "delight-stage.qml"):
        shutil.copy2(root / "dev" / name, harness / name)
    (output / "runtime-files.json").write_text(json.dumps(hashes, indent=2) + "\n")
    installation = {
        "receipt_file": str(receipt),
        "receipt_sha256": digest(receipt),
        "receipt_mtime_ns": receipt.stat().st_mtime_ns,
        "installed_runtime": str(runtime),
        "runtime_commit": source_revision,
        "checkout_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(),
        "runtime_patch_sha256": hashlib.sha256(
            subprocess.check_output(["git", "diff", "HEAD", "--", "linux"], cwd=root)
        ).hexdigest(),
        "installed_files_verified": len(hashes),
        "installed_icon_sha256": installed_icon_sha256,
    }
    (output / "installation.json").write_text(json.dumps(installation, indent=2) + "\n")
    environment = {
        "PATH": "/usr/bin:/bin",
        "HOME": str(Path.home()),
        "LANG": "C.UTF-8",
        "OFFSCREEN_DISPLAY_NUMBER": "203",
        "OFFSCREEN_SCREEN_SPEC": f"{1920 * args.pixel_ratio}x{1080 * args.pixel_ratio}x24",
        "GDK_SCALE": str(args.pixel_ratio),
        "QT_SCALE_FACTOR": str(args.pixel_ratio),
        "OFFSCREEN_ENABLE_ATSPI": "1",
        "PYTHONPATH": str(runtime),
        "MLUVA_DISABLE_GLOBAL_SHORTCUT": "1",
        "ADW_DISABLE_PORTAL": "1",
        "GSK_RENDERER": "cairo",
        "QT_QPA_PLATFORM": "xcb",
        "QT_QUICK_BACKEND": "software",
        "MAGICK_THREAD_LIMIT": "1",
        "DELIGHT_RUNTIME": str(runtime),
        "DELIGHT_PIXEL_RATIO": str(args.pixel_ratio),
        "DELIGHT_COMPOSITOR": str(args.compositor.resolve(strict=True)),
    }
    if args.source_take:
        take = args.source_take.resolve(strict=True)
        for name in ("recognition.json", "replies.json", "capture.json"):
            if not (take / name).is_file():
                raise RuntimeError(f"Source take is missing {name}")
        if json.loads((take / "capture.json").read_text())["errors"]:
            raise RuntimeError("The seed take did not complete cleanly")
        environment["DELIGHT_RECORDED_TAKE"] = str(take)
        (output / "seed-take.json").write_text(
            json.dumps(
                {
                    "take": str(take),
                    "files": {
                        name: digest(take / name) for name in ("recognition.json", "replies.json", "capture.json")
                    },
                    "use": "Exact original and final reply seed a separate deterministic native feature capture.",
                },
                indent=2,
            )
            + "\n"
        )
    helper = Path.home() / ".agents/skills/run-offscreen-linux-verification-daniel/scripts/run_isolated_x11.sh"
    command = [
        "bash",
        str(helper),
        str(output),
        "--",
        str(runtime / ".venv/bin/python"),
        str(root / "dev/delight_session.py"),
    ]
    with (output / "session.log").open("w") as log:
        child = subprocess.Popen(
            command, cwd=root, env=environment, stdout=log, stderr=subprocess.STDOUT, start_new_session=True
        )
        try:
            code = child.wait(timeout=160)
        except subprocess.TimeoutExpired:
            os.killpg(child.pid, signal.SIGTERM)
            try:
                child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(child.pid, signal.SIGKILL)
                child.wait()
            raise
    if installed_snapshot(root, runtime) != (hashes, installed_icon_sha256):
        raise RuntimeError("Installed runtime changed during capture")
    if code:
        raise RuntimeError(f"Capture failed; inspect {output}/session.log")
    subprocess.run(
        [
            "ffmpeg",
            "-nostdin",
            "-v",
            "error",
            "-y",
            "-i",
            str(output / "workflow.mkv"),
            "-c",
            "copy",
            "-movflags",
            "+faststart",
            str(output / "workflow.mp4"),
        ],
        check=True,
    )
    print(json.dumps({"output": str(output), "installed_runtime_verified": True, "display": ":203"}))


if __name__ == "__main__":
    main()
