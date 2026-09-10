"""Export one recorded Mluva revision as a reviewable, installable plugin directory."""

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
    revision = parser.add_mutually_exclusive_group(required=True)
    revision.add_argument("--release")
    revision.add_argument("--commit")
    parser.add_argument("--preview", required=True, type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    destination = args.destination.resolve()
    if not destination.is_relative_to(root / "tmp"):
        parser.error("Export beneath this repository's tmp/ directory.")
    if args.release and not re.fullmatch(r"v\d+\.\d+\.\d+", args.release):
        parser.error("Use an explicit release tag such as v1.0.0.")
    if args.commit and not re.fullmatch(r"[0-9a-f]{40}", args.commit):
        parser.error("Use a full 40-character commit ID.")
    source_revision = args.release or args.commit
    commit = subprocess.check_output(
        ["git", "rev-parse", f"{source_revision}^{{commit}}"], cwd=root, text=True
    ).strip()
    destination.mkdir(parents=True, exist_ok=False)
    hashes = {}
    plugin_directory = "linux/quickshell/mluva.dictation"
    files = subprocess.check_output(
        ["git", "ls-tree", "--name-only", f"{commit}:{plugin_directory}"],
        cwd=root,
        text=True,
    ).splitlines()
    for name in (
        "manifest.json",
        *(name for name in files if name.endswith(".qml")),
        "LICENSE",
    ):
        source = (
            name if name == "LICENSE" else f"linux/quickshell/mluva.dictation/{name}"
        )
        contents = subprocess.check_output(
            ["git", "show", f"{commit}:{source}"], cwd=root
        )
        (destination / name).write_bytes(contents)
        hashes[name] = hashlib.sha256(contents).hexdigest()
    template = (root / "dev/plugin-README.md").read_text()
    (destination / "README.md").write_text(
        template.replace("@REVISION@", args.release or commit[:7]).replace("@COMMIT@", commit)
    )
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
