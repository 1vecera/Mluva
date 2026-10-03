#!/usr/bin/env bash
# Install from the source checkout through the same reviewed native transaction.
set -euo pipefail
source_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
exec bash "$source_dir/native-source-command.sh" install "$@"
