"""Run the launch take in claw-mini's prepared, separately owned ARM64 capture guest."""

import argparse
import json
import os
import socket
import subprocess
from pathlib import Path


def main() -> None:
    """Verify container ownership and forward only the real take's credentials over stdin."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("kind", choices=("preflight", "real", "features"))
    parser.add_argument("output", type=Path)
    parser.add_argument("--installation-receipt", required=True, type=Path)
    parser.add_argument("--source-take", type=Path)
    args = parser.parse_args()
    if socket.gethostname() != "claw-mini":
        raise RuntimeError("This host adapter is scoped to claw-mini")
    root = Path(__file__).resolve().parents[1]
    output = args.output.resolve()
    output.relative_to(root / "tmp")
    receipt = args.installation_receipt.resolve(strict=True)
    installation = json.loads(receipt.read_text())
    if installation["host"] != "claw-mini-capture":
        raise RuntimeError("Use the guest's actual installation receipt")
    docker = ["docker", "--context", "colima"]
    environment = {key: os.environ[key] for key in ("PATH", "HOME", "LANG") if key in os.environ}
    details = json.loads(subprocess.check_output(docker + ["inspect", "mluva-film"], env=environment, text=True))[0]
    if (
        details["Config"]["Labels"]["dev.mluva.capture-profile"] != "claw-mini-film"
        or details["Config"]["Hostname"] != installation["host"]
        or not details["State"]["Running"]
        or not any(mount.get("Name") == "mluva-film-home" for mount in details["Mounts"])
    ):
        raise RuntimeError("The prepared capture container or home volume changed")
    common = [
        "--installation-receipt",
        str(receipt),
        "--compositor",
        "/usr/bin/xcompmgr",
        "--capture-host",
        installation["host"],
        "--encoder",
        "libopenh264",
    ]
    provider_input = None
    if args.kind == "real":
        command = [
            "dev/capture_campaign.py",
            "task",
            str(output),
            "--runtime",
            str(root),
            "--runtime-revision",
            installation["source_revision"],
            "--installed-payload",
            installation["installed_runtime"],
            "--launch-film",
            "--audio",
            "docs/promotion/assets/delight/source/dictation-input.wav",
            "--rewrite-provider",
            "codex",
            "--credentials-stdin",
            *common,
        ]
        provider_input = json.dumps(
            {
                "ELEVENLABS_API_KEY": os.environ["DAS_ITEM_ELEVEN_LABS_API_KEY__CREDENTIAL"],
                "_codex_auth": json.loads((Path.home() / ".codex/auth.json").read_text()),
            }
        ).encode()
    else:
        command = ["dev/capture_delight.py", str(output), "--runtime", installation["installed_runtime"], *common]
        if args.kind == "features":
            if not args.source_take:
                parser.error("The feature take requires the exact completed real --source-take")
            command += ["--source-take", str(args.source_take.resolve(strict=True))]
    # The Docker client and guest desktop services receive no managed environment.
    subprocess.run(
        docker + ["exec", "-i", "--workdir", str(root), "mluva-film", "uv", "run", "--no-project", *command],
        input=provider_input,
        env=environment,
        cwd=root,
        check=True,
    )


if __name__ == "__main__":
    main()
