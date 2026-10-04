#!/usr/bin/env bash
# Remove an owned installation through the same reviewed native command.
set -euo pipefail
source_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
exec bash "$source_dir/native-source-command.sh" uninstall "$@"
