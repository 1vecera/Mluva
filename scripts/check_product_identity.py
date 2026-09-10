"""Inventory retired identifiers in source or a built/installed Mluva payload."""

import argparse
import json
import re
import subprocess
from pathlib import Path

RETIRED = re.compile(rb"voice[-_ ]?scribe", re.IGNORECASE)
ROOT = Path(__file__).resolve().parents[1]
BOUNDARIES = {
    "linux/migrate_legacy.py",
    "linux/tests/test_legacy_migration.py",
    "scripts/check_product_identity.py",
    "docs/identity-migration.md",
}


def scan(paths: list[Path], root: Path, source: bool) -> dict[str, list[str]]:
    """Classify paths without logging state, credential contents or captured transcripts."""
    result: dict[str, list[str]] = {"violations": [], "migration_boundary": []}
    for path in sorted(paths):
        relative = path.relative_to(root).as_posix()
        if path.is_symlink():
            content = str(path.readlink()).encode()
        elif path.is_file():
            content = path.read_bytes()
        else:
            content = b""
        if not RETIRED.search(relative.encode()) and not RETIRED.search(content):
            continue
        category = "violations"
        if source and relative in BOUNDARIES:
            category = "migration_boundary"
        result[category].append(relative)
    return result


def main() -> int:
    """Scan the checked source or explicit package roots; nonempty violations fail."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package", type=Path, action="append", default=[])
    args = parser.parse_args()
    reports = {}
    if args.package:
        for package in args.package:
            package = package.resolve(strict=True)
            reports[str(package)] = scan(list(package.rglob("*")), package, source=False)
    else:
        paths = subprocess.check_output(
            ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=ROOT
        ).split(b"\x00")
        reports["source"] = scan(
            [ROOT / path.decode() for path in paths if path and (ROOT / path.decode()).exists()], ROOT, source=True
        )
    print(json.dumps(reports, indent=2))
    return int(any(report["violations"] for report in reports.values()))


if __name__ == "__main__":
    raise SystemExit(main())
