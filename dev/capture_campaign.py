"""Launch one real-provider capture with credentials confined to the final GTK child."""

import argparse
import hashlib
import json
import os
import shutil
import signal
import socket
import struct
import subprocess
import threading
import wave
from pathlib import Path

from capture_delight import installed_snapshot, verify_installation_receipt


def main() -> None:
    """Broker only the authorized providers after starting a secret-free private session."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "scenario",
        choices=(
            "speech",
            "task",
            "layout",
            "layout-task",
            "saved-task",
            "saved-speech",
            "catalog",
        ),
    )
    parser.add_argument("output", type=Path)
    parser.add_argument("--audio", type=Path)
    parser.add_argument("--portrait", action="store_true")
    parser.add_argument("--saved-run", type=Path)
    parser.add_argument(
        "--runtime",
        type=Path,
        help="Explicit separate worktree for an unreleased experiment",
    )
    parser.add_argument(
        "--runtime-revision",
        default="v0.3.0",
        help="Exact tested revision; runtime files must match",
    )
    parser.add_argument("--min-characters", type=int, default=160)
    parser.add_argument("--live-interval", type=int, default=4)
    parser.add_argument("--rewrite-provider", choices=("litellm", "codex"), default="litellm")
    parser.add_argument("--rewrite-model")
    parser.add_argument("--installed-payload", type=Path, help="Verified installed app directory for launch footage")
    parser.add_argument("--installation-receipt", type=Path)
    parser.add_argument("--launch-film", action="store_true", help="Uncaptioned 1080p60 Omarchy launch framing on :203")
    parser.add_argument("--compositor", type=Path)
    parser.add_argument("--pixel-ratio", type=int, choices=(1, 2), default=1)
    parser.add_argument(
        "--experimental-dirty",
        action="store_true",
        help="Record the explicitly authorized frozen, uncommitted experiment",
    )
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    runtime = args.runtime.resolve() if args.runtime else root
    output = args.output.resolve()
    output.relative_to(root / "tmp")
    output.mkdir(parents=True, exist_ok=False)
    output.chmod(0o700)
    if socket.gethostname() != "lenovo":
        raise RuntimeError("This campaign is authorized only on lenovo")
    display_number = 203 if args.launch_film else 193
    if Path(f"/tmp/.X{display_number}-lock").exists() or Path(f"/tmp/.X11-unix/X{display_number}").exists():
        raise RuntimeError(f"Reserved display :{display_number} is already occupied")
    if args.launch_film and (not args.installed_payload or not args.installation_receipt or args.portrait):
        parser.error("Launch footage requires installed payload, installation receipt and landscape framing")
    if args.installed_payload and not args.installation_receipt:
        parser.error("An installed payload requires its installation receipt")
    if args.pixel_ratio != 1 and not args.launch_film:
        parser.error("Pixel scaling is supported only by launch-film framing")
    payload = args.installed_payload.resolve(strict=True) if args.installed_payload else runtime / "linux"
    installed_hashes = {}
    installed_revision = None
    if args.installed_payload:
        receipt_path = args.installation_receipt.resolve(strict=True)
        installed_hashes, icon_hash = installed_snapshot(runtime, payload)
        installed_revision = verify_installation_receipt(runtime, payload, receipt_path)
        (output / "installation.json").write_text(
            json.dumps(
                {
                    "receipt_file": str(receipt_path),
                    "receipt_sha256": hashlib.sha256(receipt_path.read_bytes()).hexdigest(),
                    "installed_payload": str(payload),
                    "installed_files_verified": len(installed_hashes),
                    "installed_icon_sha256": icon_hash,
                },
                indent=2,
            )
            + "\n"
        )
        (output / "installed-files.json").write_text(json.dumps(installed_hashes, indent=2) + "\n")
    commit = subprocess.check_output(["git", "rev-parse", "v0.3.0^{commit}"], cwd=root, text=True).strip()
    if commit != "4ce8dc49537da98d8f32f25726f63193774d3b91":
        raise RuntimeError("Unexpected release commit")
    runtime_commit = subprocess.check_output(
        ["git", "rev-parse", args.runtime_revision + "^{commit}"],
        cwd=runtime,
        text=True,
    ).strip()
    if installed_revision is not None and installed_revision != runtime_commit:
        raise RuntimeError("Select the installation receipt's exact source revision for the real capture")
    runtime_diff = subprocess.check_output(["git", "diff", runtime_commit, "--", "linux"], cwd=runtime)
    untracked = subprocess.check_output(["git", "ls-files", "--others", "--exclude-standard", "linux"], cwd=runtime)
    if untracked:
        raise RuntimeError("Remove untracked runtime files before freezing a capture")
    if runtime_diff and not (args.runtime and args.experimental_dirty):
        raise RuntimeError("Runtime differs from the selected revision")
    (output / "runtime.patch").write_bytes(runtime_diff)
    runtime_hashes = {
        str(path.relative_to(runtime)): hashlib.sha256(path.read_bytes()).hexdigest()
        for path in (runtime / "linux").rglob("*")
        if path.is_file()
        and not any(part in {".venv", "__pycache__", ".pytest_cache", ".ruff_cache"} for part in path.parts)
    }
    (output / "runtime-files.json").write_text(json.dumps(runtime_hashes, indent=2) + "\n")
    harness = output / "harness"
    harness.mkdir()
    for name in (
        "capture_campaign.py",
        "campaign_session.py",
        "campaign-stage.qml",
        "capture_delight.py",
        "delight-stage.qml",
    ):
        shutil.copy2(root / "dev" / name, harness / name)
    if args.scenario in {"speech", "task"}:
        if args.audio is None:
            parser.error("Real capture requires --audio")
        with wave.open(str(args.audio), "rb") as source:
            if (
                source.getnchannels(),
                source.getsampwidth(),
                source.getframerate(),
            ) != (1, 2, 16000):
                parser.error("Input must be mono PCM16 WAV at 16 kHz")
            if not 1 <= source.getnframes() / source.getframerate() <= 90:
                parser.error("Input must be between 1 and 90 seconds")
    credentials = {}
    if args.scenario in {"speech", "task"}:
        credentials["ELEVENLABS_API_KEY"] = os.environ["DAS_ITEM_ELEVEN_LABS_API_KEY__CREDENTIAL"]
    if args.scenario == "task" and args.rewrite_provider == "litellm":
        credentials["CAMPAIGN_REWRITE_KEY"] = os.environ["DAS_ITEM_ZAI_API_KEY__CREDENTIAL"]
    # A Linux abstract socket has no credential file and avoids long worktree paths.
    address = "mluva-campaign-" + os.urandom(12).hex()
    server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    server.bind("\0" + address)
    server.listen(1)
    server.settimeout(40)
    errors = []

    def serve() -> None:
        """Require the same user and the isolated capture entrypoint before sending once."""
        try:
            with server.accept()[0] as connection:
                pid, uid, _gid = struct.unpack(
                    "3i",
                    connection.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12),
                )
                command = Path(f"/proc/{pid}/cmdline").read_bytes().split(b"\0")
                if uid != os.getuid() or str(root / "dev/campaign_session.py").encode() not in command:
                    raise RuntimeError("Credential broker rejected an unexpected child")
                connection.sendall(json.dumps(credentials).encode())
        except (OSError, RuntimeError, ValueError) as error:
            errors.append(type(error).__name__)
        finally:
            server.close()

    worker = threading.Thread(target=serve, daemon=True)
    worker.start()
    environment = {
        "PATH": "/usr/bin:/bin",
        "LANG": "C.UTF-8",
        "HOME": str(Path.home()),
        "OFFSCREEN_DISPLAY_NUMBER": str(display_number),
        "OFFSCREEN_SCREEN_SPEC": "1080x1920x24"
        if args.portrait
        else f"{1920 * args.pixel_ratio}x{1080 * args.pixel_ratio}x24",
        "GDK_SCALE": str(args.pixel_ratio),
        "QT_SCALE_FACTOR": str(args.pixel_ratio),
        "OFFSCREEN_ENABLE_ATSPI": "1",
        "PYTHONPATH": str(payload),
        "MLUVA_DISABLE_GLOBAL_SHORTCUT": "1",
        "ADW_DISABLE_PORTAL": "1",
        "GSK_RENDERER": "cairo",
        "QT_QPA_PLATFORM": "xcb",
        "QT_QUICK_BACKEND": "software",
        "MAGICK_THREAD_LIMIT": "1",
        "CAMPAIGN_BROKER": address,
        "CAMPAIGN_RELEASE_COMMIT": commit,
        "CAMPAIGN_RUNTIME_ROOT": str(runtime),
        "CAMPAIGN_PAYLOAD_ROOT": str(payload),
        "CAMPAIGN_DISPLAY": ":" + str(display_number),
        "CAMPAIGN_LAUNCH_FILM": "1" if args.launch_film else "0",
        "CAMPAIGN_PIXEL_RATIO": str(args.pixel_ratio),
        "CAMPAIGN_COMPOSITOR": str(args.compositor.resolve(strict=True)) if args.compositor else "",
        "CAMPAIGN_RUNTIME_COMMIT": runtime_commit,
        "CAMPAIGN_BUILD_LABEL": "v0.3.0"
        if runtime_commit == commit and not runtime_diff
        else "UNRELEASED · " + (hashlib.sha256(runtime_diff).hexdigest()[:12] if runtime_diff else runtime_commit[:12]),
        "CAMPAIGN_SCENARIO": args.scenario,
        "CAMPAIGN_PORTRAIT": "1" if args.portrait else "0",
        "CAMPAIGN_AUDIO": str(args.audio.resolve()) if args.audio else "",
        "CAMPAIGN_SAVED_RUN": str(args.saved_run.resolve()) if args.saved_run else "",
        "CAMPAIGN_STT": "elevenlabs",
        "CAMPAIGN_MIN_CHARACTERS": str(args.min_characters),
        "CAMPAIGN_LIVE_INTERVAL": str(args.live_interval),
        "CAMPAIGN_REWRITE_PROVIDER": args.rewrite_provider,
        "CAMPAIGN_REWRITE_MODEL": args.rewrite_model or ("glm-4.7-flash" if args.rewrite_provider == "litellm" else ""),
        "CAMPAIGN_CODEX_BINARY": shutil.which("codex") or "",
    }
    helper = Path.home() / ".agents/skills/run-offscreen-linux-verification-daniel/scripts/run_isolated_x11.sh"
    python_command = (
        [str(payload / ".venv/bin/python")]
        if args.installed_payload
        else ["uv", "run", "--project", str(payload), "--locked", "python"]
    )
    command = [
        "bash",
        str(helper),
        str(output),
        "--",
        *python_command,
        str(root / "dev/campaign_session.py"),
    ]
    try:
        with (output / "session.log").open("w") as log:
            process = subprocess.Popen(
                command,
                cwd=root,
                env=environment,
                stdout=log,
                stderr=subprocess.STDOUT,
                start_new_session=True,
            )
            try:
                result = process.wait(timeout=230)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGTERM)
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()
                raise
    finally:
        server.close()
    worker.join(timeout=2)
    if args.installed_payload and installed_snapshot(runtime, payload) != (installed_hashes, icon_hash):
        raise RuntimeError("Installed payload changed during real capture")
    if any(
        hashlib.sha256((runtime / name).read_bytes()).hexdigest() != digest for name, digest in runtime_hashes.items()
    ):
        raise RuntimeError("Runtime files changed during capture")
    (output / "runtime-verification.json").write_text(
        json.dumps(
            {
                "all_recorded_runtime_files_unchanged": True,
                "file_count": len(runtime_hashes),
                "base_commit": runtime_commit,
                "runtime_patch_sha256": hashlib.sha256(runtime_diff).hexdigest(),
                "runtime_files_manifest_sha256": hashlib.sha256(
                    (output / "runtime-files.json").read_bytes()
                ).hexdigest(),
            },
            indent=2,
        )
        + "\n"
    )
    if result or errors:
        raise RuntimeError(f"Capture failed; inspect {output}/session.log; broker={errors}")
    print(f"Capture complete: {output}")


if __name__ == "__main__":
    main()
