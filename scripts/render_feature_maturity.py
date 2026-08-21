#!/usr/bin/env python3
"""Write or check the public feature-maturity matrix."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPOSITORY_ROOT / "linux"))

from voice_scribe_linux.feature_maturity import render_feature_maturity_markdown  # noqa: E402

OUTPUT_PATH = REPOSITORY_ROOT / "docs" / "feature-maturity.md"


def main() -> int:
    """Write the generated matrix or fail when the checked-in copy has drifted."""
    parser = argparse.ArgumentParser()
    action = parser.add_mutually_exclusive_group(required=True)
    action.add_argument("--check", action="store_true")
    action.add_argument("--write", action="store_true")
    arguments = parser.parse_args()
    expected = render_feature_maturity_markdown()
    if arguments.write:
        OUTPUT_PATH.write_text(expected, encoding="utf-8")
        return 0
    if not OUTPUT_PATH.is_file() or OUTPUT_PATH.read_text(encoding="utf-8") != expected:
        print("docs/feature-maturity.md is stale; run `make linux-feature-maturity`.", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
