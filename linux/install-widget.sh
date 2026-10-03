#!/usr/bin/env bash
# Build the native widget command without app development libraries.
set -euo pipefail
source_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
exec bash "$source_dir/native-source-command.sh" widget "$@"
