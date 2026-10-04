#!/usr/bin/env bash
# Native conversation editing, navigation, scroll stability and desktop actions.
set -euo pipefail
exec bash "$(dirname -- "${BASH_SOURCE[0]}")/run_application_smoke.sh" conversation "$@"
