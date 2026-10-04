#!/usr/bin/env bash
# Combine the package-manager boundary with the existing private widget peer.
set -euo pipefail
if [[ "$1 $2 ${3:-}" == 'plugin enable --help' ]]; then
    printf 'manager-ready\n' >> "$MLUVA_SETUP_LOG"
    exit 0
fi
if [[ "$1 $2" == 'pkg add' ]]; then
    printf 'packages\n' >> "$MLUVA_SETUP_LOG"
    [[ "$MLUVA_ACTUAL_SETUP_CASE" != package-failure ]] || exit 7
    exit 0
fi
exec /usr/bin/bash "$MLUVA_WIDGET_PEER" "$@"
