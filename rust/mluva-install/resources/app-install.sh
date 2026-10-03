#!/usr/bin/env bash
# App-only entry point, including disposable-prefix installations.
set -euo pipefail
bundle_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
exec "$bundle_root/bin/mluva-install" "$@"
