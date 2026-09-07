"""Export one tagged Mluva plugin as a reviewable, installable repository directory."""

import argparse
import hashlib
import json
import re
import shutil
import subprocess
from pathlib import Path


def main() -> None:
    """Copy only the public plugin, license, provenance and reviewed preview asset."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    parser.add_argument("--release", default="v0.1.1")
    parser.add_argument("--preview", required=True, type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    destination = args.destination.resolve()
    if not destination.is_relative_to(root / "tmp"):
        parser.error("Export beneath this repository's tmp/ directory.")
    if not re.fullmatch(r"v\d+\.\d+\.\d+", args.release):
        parser.error("Use an explicit release tag such as v0.1.1.")
    commit = subprocess.check_output(["git", "rev-parse", f"{args.release}^{{commit}}"], cwd=root, text=True).strip()
    destination.mkdir(parents=True, exist_ok=False)
    hashes = {}
    for name in ("manifest.json", "Widget.qml", "RecordingOverlay.qml", "LICENSE"):
        source = name if name == "LICENSE" else f"linux/quickshell/mluva.dictation/{name}"
        contents = subprocess.check_output(["git", "show", f"{commit}:{source}"], cwd=root)
        (destination / name).write_bytes(contents)
        hashes[name] = hashlib.sha256(contents).hexdigest()
    template = (root / "dev/plugin-README.md").read_text()
    (destination / "README.md").write_text(template.replace("@RELEASE@", args.release))
    shutil.copyfile(args.preview, destination / "preview.png")
    (destination / "SOURCE.json").write_text(
        json.dumps(
            {
                "repository": "https://github.com/1vecera/Mluva",
                "release": args.release,
                "commit": commit,
                "directory": "linux/quickshell/mluva.dictation",
                "sha256": hashes,
            },
            indent=2,
        )
        + "\n"
    )
    print(destination)


if __name__ == "__main__":
    main()
